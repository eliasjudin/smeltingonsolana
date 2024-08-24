use anchor_lang::prelude::*;
use anchor_spl::token::{self, Burn, MintTo, Token, TokenAccount, Transfer};
use solana_program::native_token::LAMPORTS_PER_SOL;

declare_id!("AA6AAAAAAAAAAAAAAAAA6A2AAA7AAAAAAA476AAAAAAA");

#[program]
pub mod theforge {
    use super::*;

    pub fn initialize(
        ctx: Context<Initialize>,
        smelting_success_rate: u8,
        minimum_coal_amount: u64,
        cooldown_period: i64,
    ) -> Result<()> {
        require!(
            smelting_success_rate <= 100,
            SmelterError::InvalidSmeltingSuccessRate
        );
        require!(minimum_coal_amount > 0, SmelterError::InvalidAmount);
        require!(cooldown_period >= 0, SmelterError::InvalidCooldownPeriod);

        let smelter = &mut ctx.accounts.smelter;
        smelter.authority = ctx.accounts.authority.key();
        smelter.coal_mint = ctx.accounts.coal_mint.key();
        smelter.ore_mint = ctx.accounts.ore_mint.key();
        smelter.ingot_mint = ctx.accounts.ingot_mint.key();
        smelter.smelting_success_rate = smelting_success_rate;
        smelter.minimum_coal_amount = minimum_coal_amount;
        smelter.cooldown_period = cooldown_period;
        smelter.last_smelt_time = 0;
        smelter.bump = *ctx.bumps.get("smelter").unwrap();
        smelter.is_processing = false;

        let ore_token = token::Mint::unpack(&ctx.accounts.ore_mint.data.borrow())?;
        smelter.token_decimals = ore_token.decimals;

        msg!(
            "Smelter initialized with success rate: {}",
            smelting_success_rate
        );
        Ok(())
    }

    pub fn smelt(ctx: Context<Smelt>, ore_amount: u64, coal_amount: u64) -> Result<()> {
        let smelter = &mut ctx.accounts.smelter;
        require!(!smelter.is_processing, SmelterError::ReentrancyDetected);
        smelter.is_processing = true;

        require!(ore_amount > 0, SmelterError::InvalidAmount);
        require!(
            coal_amount >= smelter.minimum_coal_amount,
            SmelterError::InsufficientCoal
        );

        let clock = Clock::get()?;
        require!(
            clock.unix_timestamp >= smelter.last_smelt_time + smelter.cooldown_period,
            SmelterError::CooldownPeriodNotMet
        );

        // Check if user has enough SOL to cover transaction fees
        let rent = Rent::get()?;
        let minimum_balance = rent.minimum_balance(0);
        require!(
            ctx.accounts.user_authority.lamports() > minimum_balance,
            SmelterError::InsufficientFunds
        );

        // Burn COAL tokens
        let cpi_accounts = Burn {
            mint: ctx.accounts.coal_mint.to_account_info(),
            from: ctx.accounts.user_coal_account.to_account_info(),
            authority: ctx.accounts.user_authority.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::burn(cpi_ctx, coal_amount)?;

        if smelter.smelting_success_rate as u64 > calculate_percentage(100, 0) {
            // Transfer ORE tokens from user to program-owned account
            let cpi_accounts = Transfer {
                from: ctx.accounts.user_ore_account.to_account_info(),
                to: ctx.accounts.program_ore_account.to_account_info(),
                authority: ctx.accounts.user_authority.to_account_info(),
            };
            let cpi_program = ctx.accounts.token_program.to_account_info();
            let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
            token::transfer(cpi_ctx, ore_amount)?;

            // Mint INGOT tokens to user
            let cpi_accounts = MintTo {
                mint: ctx.accounts.ingot_mint.to_account_info(),
                to: ctx.accounts.user_ingot_account.to_account_info(),
                authority: ctx.accounts.smelter.to_account_info(),
            };
            let cpi_program = ctx.accounts.token_program.to_account_info();
            let smelter_key = smelter.key();
            let seeds = &[smelter_key.as_ref(), &[smelter.bump]];
            let signer = &[&seeds[..]];
            let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
            token::mint_to(cpi_ctx, ore_amount)?;

            emit!(SmeltingSuccessful {
                user: ctx.accounts.user_authority.key(),
                ore_amount,
                coal_amount,
                ingot_amount: ore_amount,
            });
            msg!(
                "Smelting successful: {} ORE converted to {} INGOT",
                ore_amount,
                ore_amount
            );
        } else {
            emit!(SmeltingFailed {
                user: ctx.accounts.user_authority.key(),
                coal_amount,
            });
            msg!("Smelting failed: {} COAL burned", coal_amount);
        }

        smelter.last_smelt_time = clock.unix_timestamp;
        smelter.is_processing = false;

        Ok(())
    }

    pub fn unsmelt(ctx: Context<Unsmelt>, ingot_amount: u64) -> Result<()> {
        let smelter = &mut ctx.accounts.smelter;
        require!(!smelter.is_processing, SmelterError::ReentrancyDetected);
        smelter.is_processing = true;

        require!(ingot_amount > 0, SmelterError::InvalidAmount);
        require!(
            ctx.accounts.user_ingot_account.amount >= ingot_amount,
            SmelterError::InsufficientIngot
        );
        require!(
            ctx.accounts.program_ore_account.amount >= ingot_amount,
            SmelterError::InsufficientProgramOre
        );

        // Check if user has enough SOL to cover transaction fees
        let rent = Rent::get()?;
        let minimum_balance = rent.minimum_balance(0);
        require!(
            ctx.accounts.user_authority.lamports() > minimum_balance,
            SmelterError::InsufficientFunds
        );

        // Burn INGOT tokens
        let cpi_accounts = Burn {
            mint: ctx.accounts.ingot_mint.to_account_info(),
            from: ctx.accounts.user_ingot_account.to_account_info(),
            authority: ctx.accounts.user_authority.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::burn(cpi_ctx, ingot_amount)?;

        // Transfer ORE tokens from program-owned account to user
        let cpi_accounts = Transfer {
            from: ctx.accounts.program_ore_account.to_account_info(),
            to: ctx.accounts.user_ore_account.to_account_info(),
            authority: ctx.accounts.smelter.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let smelter_key = smelter.key();
        let seeds = &[smelter_key.as_ref(), &[smelter.bump]];
        let signer = &[&seeds[..]];
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
        token::transfer(cpi_ctx, ingot_amount)?;

        emit!(UnsmeltingSuccessful {
            user: ctx.accounts.user_authority.key(),
            ingot_amount,
            ore_amount: ingot_amount,
        });
        msg!(
            "Unsmelting successful: {} INGOT converted back to {} ORE",
            ingot_amount,
            ingot_amount
        );

        smelter.is_processing = false;

        Ok(())
    }

    pub fn initialize_multi_sig(
        ctx: Context<InitializeMultiSig>,
        signers: Vec<Pubkey>,
        threshold: u8,
    ) -> Result<()> {
        require!(threshold > 0, SmelterError::InvalidThreshold);
        require!(
            signers.len() >= threshold as usize,
            SmelterError::InvalidSignerCount
        );
        require!(signers.len() <= 10, SmelterError::TooManySigners); // Arbitrary limit

        let multi_sig = &mut ctx.accounts.multi_sig_authority;
        multi_sig.signers = signers;
        multi_sig.threshold = threshold;
        multi_sig.nonce = 0;

        let smelter = &mut ctx.accounts.smelter;
        smelter.multi_sig_authority = multi_sig.key();

        Ok(())
    }

    pub fn propose_parameter_update(
        ctx: Context<ProposeUpdate>,
        new_success_rate: Option<u8>,
        new_minimum_coal: Option<u64>,
        new_cooldown_period: Option<i64>,
    ) -> Result<()> {
        let multi_sig = &mut ctx.accounts.multi_sig_authority;
        let proposal = &mut ctx.accounts.proposal;

        require!(
            multi_sig.signers.contains(&ctx.accounts.proposer.key()),
            SmelterError::UnauthorizedProposer
        );

        proposal.proposed_by = ctx.accounts.proposer.key();
        proposal.new_success_rate = new_success_rate;
        proposal.new_minimum_coal = new_minimum_coal;
        proposal.new_cooldown_period = new_cooldown_period;
        proposal.approvals = vec![false; multi_sig.signers.len()];
        proposal.is_executed = false;
        proposal.nonce = multi_sig.nonce;

        multi_sig.nonce = multi_sig.nonce.checked_add(1).unwrap();

        emit!(ProposalCreated {
            proposal: proposal.key(),
            proposer: proposal.proposed_by,
        });

        Ok(())
    }

    pub fn approve_parameter_update(ctx: Context<ApproveUpdate>) -> Result<()> {
        let multi_sig = &ctx.accounts.multi_sig_authority;
        let proposal = &mut ctx.accounts.proposal;
        let signer = &ctx.accounts.signer;

        require!(!proposal.is_executed, SmelterError::ProposalAlreadyExecuted);

        let signer_index = multi_sig
            .signers
            .iter()
            .position(|&s| s == signer.key())
            .ok_or(SmelterError::UnauthorizedSigner)?;

        require!(
            !proposal.approvals[signer_index],
            SmelterError::AlreadyApproved
        );

        proposal.approvals[signer_index] = true;

        emit!(ProposalApproved {
            proposal: proposal.key(),
            signer: signer.key(),
        });

        if proposal.approvals.iter().filter(|&&x| x).count() >= multi_sig.threshold as usize {
            let smelter = &mut ctx.accounts.smelter;

            if let Some(rate) = proposal.new_success_rate {
                require!(rate <= 100, SmelterError::InvalidSmeltingSuccessRate);
                smelter.smelting_success_rate = rate;
            }

            if let Some(coal) = proposal.new_minimum_coal {
                require!(coal > 0, SmelterError::InvalidAmount);
                smelter.minimum_coal_amount = coal;
            }

            if let Some(period) = proposal.new_cooldown_period {
                require!(period >= 0, SmelterError::InvalidCooldownPeriod);
                smelter.cooldown_period = period;
            }

            proposal.is_executed = true;

            emit!(ProposalExecuted {
                proposal: proposal.key(),
            });
        }

        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + 32 + 32 + 32 + 32 + 1 + 8 + 8 + 8 + 1 + 1 + 1
    )]
    pub smelter: Account<'info, Smelter>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub coal_mint: Account<'info, token::Mint>,
    pub ore_mint: Account<'info, token::Mint>,
    pub ingot_mint: Account<'info, token::Mint>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct Smelt<'info> {
    #[account(mut)]
    pub smelter: Account<'info, Smelter>,
    #[account(mut)]
    pub user_authority: Signer<'info>,
    #[account(mut, constraint = user_coal_account.owner == user_authority.key())]
    pub user_coal_account: Account<'info, TokenAccount>,
    #[account(mut, constraint = user_ore_account.owner == user_authority.key())]
    pub user_ore_account: Account<'info, TokenAccount>,
    #[account(mut, constraint = user_ingot_account.owner == user_authority.key())]
    pub user_ingot_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub program_ore_account: Account<'info, TokenAccount>,
    #[account(mut, address = smelter.coal_mint)]
    pub coal_mint: Account<'info, token::Mint>,
    #[account(mut, address = smelter.ore_mint)]
    pub ore_mint: Account<'info, token::Mint>,
    #[account(mut, address = smelter.ingot_mint)]
    pub ingot_mint: Account<'info, token::Mint>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct Unsmelt<'info> {
    #[account(mut)]
    pub smelter: Account<'info, Smelter>,
    pub user_authority: Signer<'info>,
    #[account(mut, constraint = user_ingot_account.owner == user_authority.key())]
    pub user_ingot_account: Account<'info, TokenAccount>,
    #[account(mut, constraint = user_ore_account.owner == user_authority.key())]
    pub user_ore_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub program_ore_account: Account<'info, TokenAccount>,
    #[account(mut, address = smelter.ingot_mint)]
    pub ingot_mint: Account<'info, token::Mint>,
    #[account(address = smelter.ore_mint)]
    pub ore_mint: Account<'info, token::Mint>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct UpdateSmelterParams<'info> {
    #[account(mut, has_one = authority)]
    pub smelter: Account<'info, Smelter>,
    pub authority: Signer<'info>,
}

#[derive(Accounts)]
pub struct InitializeMultiSig<'info> {
    #[account(mut)]
    pub smelter: Account<'info, Smelter>,
    #[account(
        init,
        payer = payer,
        space = 8 + 32 * 10 + 1 + 8 // Adjust space calculation based on actual fields
    )]
    pub multi_sig_authority: Account<'info, MultiSigAuthority>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ProposeUpdate<'info> {
    #[account(has_one = multi_sig_authority)]
    pub smelter: Account<'info, Smelter>,
    pub multi_sig_authority: Account<'info, MultiSigAuthority>,
    #[account(
        init,
        payer = proposer,
        space = 8 + 32 + 1 + 8 + 8 + 10 + 1 + 8 // Adjust space calculation based on actual fields
    )]
    pub proposal: Account<'info, Proposal>,
    #[account(mut)]
    pub proposer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ApproveUpdate<'info> {
    #[account(mut, has_one = multi_sig_authority)]
    pub smelter: Account<'info, Smelter>,
    pub multi_sig_authority: Account<'info, MultiSigAuthority>,
    #[account(mut)]
    pub proposal: Account<'info, Proposal>,
    pub signer: Signer<'info>,
}

#[account]
pub struct MultiSigAuthority {
    pub signers: Vec<Pubkey>,
    pub threshold: u8,
    pub nonce: u64,
}

#[account]
pub struct Proposal {
    pub proposed_by: Pubkey,
    pub new_success_rate: Option<u8>,
    pub new_minimum_coal: Option<u64>,
    pub new_cooldown_period: Option<i64>,
    pub approvals: Vec<bool>,
    pub is_executed: bool,
    pub nonce: u64,
}

#[account]
pub struct Smelter {
    pub authority: Pubkey,
    pub coal_mint: Pubkey,
    pub ore_mint: Pubkey,
    pub ingot_mint: Pubkey,
    pub smelting_success_rate: u8,
    pub minimum_coal_amount: u64,
    pub cooldown_period: i64,
    pub last_smelt_time: i64,
    pub bump: u8,
    pub token_decimals: u8,
    pub is_processing: bool,
    pub multi_sig_authority: Pubkey,
}

#[error_code]
pub enum SmelterError {
    #[msg("Invalid smelting success rate. Must be between 0 and 100.")]
    InvalidSmeltingSuccessRate,
    #[msg("Invalid amount. Must be greater than zero.")]
    InvalidAmount,
    #[msg("Insufficient coal for smelting.")]
    InsufficientCoal,
    #[msg("Cooldown period not met.")]
    CooldownPeriodNotMet,
    #[msg("Invalid cooldown period. Must be non-negative.")]
    InvalidCooldownPeriod,
    #[msg("Insufficient INGOT balance for unsmelting.")]
    InsufficientIngot,
    #[msg("Insufficient ORE in program account for unsmelting.")]
    InsufficientProgramOre,
    #[msg("Insufficient funds to cover transaction fees.")]
    InsufficientFunds,
    #[msg("Reentrancy detected.")]
    ReentrancyDetected,
    #[msg("Invalid threshold for multi-sig authority.")]
    InvalidThreshold,
    #[msg("Invalid number of signers for multi-sig authority.")]
    InvalidSignerCount,
    #[msg("Too many signers specified for multi-sig authority.")]
    TooManySigners,
    #[msg("Unauthorized proposer.")]
    UnauthorizedProposer,
    #[msg("Unauthorized signer.")]
    UnauthorizedSigner,
    #[msg("Proposal has already been executed.")]
    ProposalAlreadyExecuted,
    #[msg("Signer has already approved this proposal.")]
    AlreadyApproved,
}

#[event]
pub struct SmeltingSuccessful {
    pub user: Pubkey,
    pub ore_amount: u64,
    pub coal_amount: u64,
    pub ingot_amount: u64,
}

#[event]
pub struct SmeltingFailed {
    pub user: Pubkey,
    pub coal_amount: u64,
}

#[event]
pub struct UnsmeltingSuccessful {
    pub user: Pubkey,
    pub ingot_amount: u64,
    pub ore_amount: u64,
}

#[event]
pub struct ProposalCreated {
    pub proposal: Pubkey,
    pub proposer: Pubkey,
}

#[event]
pub struct ProposalApproved {
    pub proposal: Pubkey,
    pub signer: Pubkey,
}

#[event]
pub struct ProposalExecuted {
    pub proposal: Pubkey,
}

// Helper function for fixed-point percentage calculation
fn calculate_percentage(value: u64, percentage: u64) -> u64 {
    (value * percentage) / 100
}
