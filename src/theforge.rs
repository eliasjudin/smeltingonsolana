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
        smelter.last_smelt_time = 0; // Initialize to 0, will be updated on first smelt
        smelter.bump = *ctx.bumps.get("smelter").unwrap();

        Ok(())
    }

    pub fn smelt(ctx: Context<Smelt>, ore_amount: u64, coal_amount: u64) -> Result<()> {
        let smelter = &ctx.accounts.smelter;

        // Input validation
        require!(ore_amount > 0, SmelterError::InvalidAmount);
        require!(
            coal_amount >= smelter.minimum_coal_amount,
            SmelterError::InsufficientCoal
        );

        // Check cooldown
        let clock = Clock::get()?;
        require!(
            clock.unix_timestamp >= smelter.last_smelt_time + smelter.cooldown_period,
            SmelterError::CooldownPeriodNotMet
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

        // Check if smelting is successful
        let random_number = get_random_number(ctx.accounts.recent_blockhashes.to_account_info())?;
        if random_number % 100 < smelter.smelting_success_rate as u64 {
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
        } else {
            emit!(SmeltingFailed {
                user: ctx.accounts.user_authority.key(),
                coal_amount,
            });
        }

        // Update last smelt time
        let mut smelter = &mut ctx.accounts.smelter;
        smelter.last_smelt_time = clock.unix_timestamp;

        Ok(())
    }

    pub fn unsmelt(ctx: Context<Unsmelt>, ingot_amount: u64) -> Result<()> {
        let smelter = &ctx.accounts.smelter;

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

        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = authority,
        // - 8 bytes for the account discriminator
        // - 32 bytes each for authority, coal_mint, ore_mint, and ingot_mint (4 * 32 = 128 bytes)
        // - 1 byte for smelting_success_rate
        // - 8 bytes each for minimum_coal_amount, cooldown_period, and last_smelt_time (3 * 8 = 24 bytes)
        // - 1 byte for bump
        space = 8 + 32 + 32 + 32 + 32 + 1 + 8 + 8 + 8 + 1 //162 bytes
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
    /// CHECK: This is safe because we only read from it
    pub recent_blockhashes: AccountInfo<'info>,
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

fn get_random_number(recent_blockhashes: AccountInfo) -> Result<u64> {
    let data = recent_blockhashes.try_borrow_data()?;
    let most_recent = array_ref![data, 0, 8];
    Ok(u64::from_le_bytes(*most_recent))
}
