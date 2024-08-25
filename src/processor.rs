use solana_program::hash::hash;

;set_upgrade_authority_upgradeable::bpf_loader solana_program::useuse crate::{
    constants::{AUTHORITY_SEED, MAX_AMOUNT, SMELTING_SUCCESS_RATE},
    error::SmeltingError,
    instruction::SmeltingInstruction,
    state::{SmeltingState, MAX_INGOT_SUPPLY},
};
use solana_program::program_error::ProgramError;

use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    msg,
    program::{invoke, invoke_signed},
    program_error::ProgrsamError,
    pubkey::Pubkey,
    system_instruction,
    sysvar::{clock::Clock, rent::Rent, Sysvar},
};
use spl_token::state::Account as TokenAccount;

pub struct Processor;

impl Processor {
    pub fn process(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        instruction_data: &[u8],
    ) -> ProgramResult {
        let instruction = SmeltingInstruction::unpack(instruction_data)?;

        match instruction {
            SmeltingInstruction::Smelt { amount } => {
                if amount == 0 || amount > MAX_AMOUNT {
                    return Err(ProgramError::InvalidInstructionData.into());
                }
                Self::process_smelt(accounts, amount, program_id)
            }
            SmeltingInstruction::Unsmelt { amount } => {
                if amount == 0 || amount > MAX_AMOUNT {
                    return Err(ProgramError::InvalidInstructionData.into());
                }
                Self::process_unsmelt(accounts, amount, program_id)
            }
            SmeltingInstruction::MintIngot { amount } => {
                if amount == 0 || amount > MAX_AMOUNT {
                    return Err(ProgramError::InvalidInstructionData.into());
                }
                Self::process_mint_ingot(accounts, amount, program_id)
            }
            SmeltingInstruction::TransferOre { amount } => {
                if amount == 0 || amount > MAX_AMOUNT {
                    return Err(ProgramError::InvalidInstructionData.into());
                }
                Self::process_transfer_ore(accounts, amount, program_id)
            }
            SmeltingInstruction::TransferIngot { amount } => {
                if amount == 0 || amount > MAX_AMOUNT {
                    return Err(ProgramError::InvalidInstructionData.into());
                }
                Self::process_transfer_ingot(accounts, amount, program_id)
            }
            SmeltingInstruction::UpgradeProgram => {
                Self::process_upgrade_program(accounts, program_id)
            }
        }
    }

    fn process_smelt(accounts: &[AccountInfo], amount: u64, program_id: &Pubkey) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();
        let user_account = next_account_info(account_info_iter)?;
        let ore_account = next_account_info(account_info_iter)?;
        let coal_account = next_account_info(account_info_iter)?;
        let ingot_account = next_account_info(account_info_iter)?;
        let smelting_state_account = next_account_info(account_info_iter)?;
        let token_program = next_account_info(account_info_iter)?;

        let mut smelting_state = SmeltingState::unpack(&smelting_state_account.data.borrow())?;

        // Check if smelting succeeds (80% chance)
        let clock = Clock::get()?;
        let seed = hash(&clock.slot.to_le_bytes()).to_bytes();
        let success = (seed[0] as u16) < ((SMELTING_SUCCESS_RATE as u16 * 256) / 100);

        // Burn COAL tokens
        let burn_instruction = spl_token::instruction::burn(
            token_program.key,
            coal_account.key,
            smelting_state.coal_mint,
            user_account.key,
            &[],
            amount,
        )?;
        invoke(
            &burn_instruction,
            &[
                coal_account.clone(),
                user_account.clone(),
                token_program.clone(),
            ],
        )?;

        if success {
            // Check if minting more INGOT tokens would exceed the maximum supply
            if !smelting_state.can_mint_ingot(amount) {
                return Err(SmeltingError::MaxSupplyExceeded.into());
            }

            // Transfer ORE tokens to program account
            let transfer_instruction = spl_token::instruction::transfer(
                token_program.key,
                ore_account.key,
                smelting_state.ore_vault,
                user_account.key,
                &[],
                amount,
            )?;
            invoke(
                &transfer_instruction,
                &[
                    ore_account.clone(),
                    user_account.clone(),
                    token_program.clone(),
                ],
            )?;

            // Mint INGOT tokens to user
            let mint_instruction = spl_token::instruction::mint_to(
                token_program.key,
                smelting_state.ingot_mint,
                ingot_account.key,
                &smelting_state.authority,
                &[],
                amount,
            )?;
            invoke_signed(
                &mint_instruction,
                &[ingot_account.clone(), token_program.clone()],
                &[&[AUTHORITY_SEED, &[smelting_state.authority_bump]]],
            )?;

            smelting_state.update_on_successful_smelt(amount)?;

            msg!("Successfully smelted {} ORE into INGOT", amount);
        } else {
            msg!("Smelting failed. COAL burned but no INGOT produced");
        }

        SmeltingState::pack(
            smelting_state,
            &mut smelting_state_account.data.borrow_mut(),
        )?;

        Ok(())
    }

    fn process_unsmelt(
        accounts: &[AccountInfo],
        amount: u64,
        program_id: &Pubkey,
    ) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();
        let user_account = next_account_info(account_info_iter)?;
        let ore_account = next_account_info(account_info_iter)?;
        let ingot_account = next_account_info(account_info_iter)?;
        let smelting_state_account = next_account_info(account_info_iter)?;
        let token_program = next_account_info(account_info_iter)?;

        let mut smelting_state = SmeltingState::unpack(&smelting_state_account.data.borrow())?;

        let fee = SmeltingState::calculate_unsmelt_fee(amount);
        let ore_to_return = amount.saturating_sub(fee);

        // Burn INGOT tokens
        let burn_instruction = spl_token::instruction::burn(
            token_program.key,
            ingot_account.key,
            smelting_state.ingot_mint,
            user_account.key,
            &[],
            amount,
        )?;
        invoke(
            &burn_instruction,
            &[
                ingot_account.clone(),
                user_account.clone(),
                token_program.clone(),
            ],
        )?;

        // Transfer ORE tokens from vault to user
        let transfer_instruction = spl_token::instruction::transfer(
            token_program.key,
            smelting_state.ore_vault,
            ore_account.key,
            &smelting_state.authority,
            &[],
            ore_to_return,
        )?;
        invoke_signed(
            &transfer_instruction,
            &[ore_account.clone(), token_program.clone()],
            &[&[AUTHORITY_SEED, &[smelting_state.authority_bump]]],
        )?;

        smelting_state.update_on_unsmelt(amount, fee);

        SmeltingState::pack(
            smelting_state,
            &mut smelting_state_account.data.borrow_mut(),
        )?;

        msg!(
            "Successfully unsmelted {} INGOT into {} ORE with a fee of {} ORE",
            amount,
            ore_to_return,
            fee
        );

        Ok(())
    }

    fn process_mint_ingot(
        accounts: &[AccountInfo],
        amount: u64,
        program_id: &Pubkey,
    ) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();
        let mint_authority = next_account_info(account_info_iter)?;
        let ingot_mint = next_account_info(account_info_iter)?;
        let ingot_account = next_account_info(account_info_iter)?;
        let smelting_state_account = next_account_info(account_info_iter)?;
        let token_program = next_account_info(account_info_iter)?;

        let mut smelting_state = SmeltingState::unpack(&smelting_state_account.data.borrow())?;

        if !mint_authority.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }

        if *mint_authority.key != smelting_state.authority {
            return Err(ProgramError::InvalidAccountData);
        }

        smelting_state.total_ingots_minted += amount;
        if smelting_state.total_ingots_minted > MAX_INGOT_SUPPLY {
            return Err(SmeltingError::MaxSupplyExceeded.into());
        }

        let mint_instruction = spl_token::instruction::mint_to(
            token_program.key,
            ingot_mint.key,
            ingot_account.key,
            mint_authority.key,
            &[],
            amount,
        )?;
        invoke_signed(
            &mint_instruction,
            &[
                ingot_mint.clone(),
                ingot_account.clone(),
                mint_authority.clone(),
                token_program.clone(),
            ],
            &[&[AUTHORITY_SEED, &[smelting_state.authority_bump]]],
        )?;

        msg!("Successfully minted {} INGOT", amount);

        SmeltingState::pack(
            smelting_state,
            &mut smelting_state_account.data.borrow_mut(),
        )?;

        Ok(())
    }

    fn process_transfer_ore(
        accounts: &[AccountInfo],
        amount: u64,
        program_id: &Pubkey,
    ) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();
        let source_account = next_account_info(account_info_iter)?;
        let destination_account = next_account_info(account_info_iter)?;
        let authority = next_account_info(account_info_iter)?;
        let token_program = next_account_info(account_info_iter)?;

        if !authority.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }

        let transfer_instruction = spl_token::instruction::transfer(
            token_program.key,
            source_account.key,
            destination_account.key,
            authority.key,
            &[],
            amount,
        )?;
        invoke(
            &transfer_instruction,
            &[
                source_account.clone(),
                destination_account.clone(),
                authority.clone(),
                token_program.clone(),
            ],
        )?;

        msg!("Successfully transferred {} ORE", amount);

        Ok(())
    }

    fn process_transfer_ingot(
        accounts: &[AccountInfo],
        amount: u64,
        program_id: &Pubkey,
    ) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();
        let source_account = next_account_info(account_info_iter)?;
        let destination_account = next_account_info(account_info_iter)?;
        let authority = next_account_info(account_info_iter)?;
        let token_program = next_account_info(account_info_iter)?;

        if !authority.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }

        let transfer_instruction = spl_token::instruction::transfer(
            token_program.key,
            source_account.key,
            destination_account.key,
            authority.key,
            &[],
            amount,
        )?;
        invoke(
            &transfer_instruction,
            &[
                source_account.clone(),
                destination_account.clone(),
                authority.clone(),
                token_program.clone(),
            ],
        )?;

        msg!("Successfully transferred {} INGOT", amount);

        Ok(())
    }
}
