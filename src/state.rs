use solana_program::{
    program_error::ProgramError,
    program_pack::{IsInitialized, Pack, Sealed},
    pubkey::Pubkey,
};

pub const MAX_INGOT_SUPPLY: u64 = 21_000_000;
pub const UNSMELT_FEE_PERCENTAGE: u8 = 5;

pub struct SmeltingState {
    pub is_initialized: bool,
    pub authority: Pubkey,
    pub authority_bump: u8,
    pub ore_mint: Pubkey,
    pub ingot_mint: Pubkey,
    pub coal_mint: Pubkey,
    pub ore_vault: Pubkey,
    pub total_ingots_minted: u64,
    pub total_ore_locked: u64,
}

impl Sealed for SmeltingState {}

impl IsInitialized for SmeltingState {
    fn is_initialized(&self) -> bool {
        self.is_initialized
    }
}

impl Pack for SmeltingState {
    const LEN: usize = 1 + 32 + 1 + 32 + 32 + 32 + 32 + 8 + 8;

    fn unpack_from_slice(src: &[u8]) -> Result<Self, ProgramError> {
        let src = array_ref![src, 0, SmeltingState::LEN];
        let (
            is_initialized,
            authority,
            authority_bump,
            ore_mint,
            ingot_mint,
            coal_mint,
            ore_vault,
            total_ingots_minted,
            total_ore_locked,
        ) = array_refs![src, 1, 32, 1, 32, 32, 32, 32, 8, 8];

        Ok(SmeltingState {
            is_initialized: is_initialized[0] != 0,
            authority: Pubkey::new_from_array(*authority),
            authority_bump: authority_bump[0],
            ore_mint: Pubkey::new_from_array(*ore_mint),
            ingot_mint: Pubkey::new_from_array(*ingot_mint),
            coal_mint: Pubkey::new_from_array(*coal_mint),
            ore_vault: Pubkey::new_from_array(*ore_vault),
            total_ingots_minted: u64::from_le_bytes(*total_ingots_minted),
            total_ore_locked: u64::from_le_bytes(*total_ore_locked),
        })
    }

    fn pack_into_slice(&self, dst: &mut [u8]) {
        let dst = array_mut_ref![dst, 0, SmeltingState::LEN];
        let (
            is_initialized_dst,
            authority_dst,
            authority_bump_dst,
            ore_mint_dst,
            ingot_mint_dst,
            coal_mint_dst,
            ore_vault_dst,
            total_ingots_minted_dst,
            total_ore_locked_dst,
        ) = mut_array_refs![dst, 1, 32, 1, 32, 32, 32, 32, 8, 8];

        is_initialized_dst[0] = self.is_initialized as u8;
        authority_dst.copy_from_slice(self.authority.as_ref());
        authority_bump_dst[0] = self.authority_bump;
        ore_mint_dst.copy_from_slice(self.ore_mint.as_ref());
        ingot_mint_dst.copy_from_slice(self.ingot_mint.as_ref());
        coal_mint_dst.copy_from_slice(self.coal_mint.as_ref());
        ore_vault_dst.copy_from_slice(self.ore_vault.as_ref());
        *total_ingots_minted_dst = self.total_ingots_minted.to_le_bytes();
        *total_ore_locked_dst = self.total_ore_locked.to_le_bytes();
    }
}

impl SmeltingState {
    pub fn calculate_unsmelt_fee(amount: u64) -> u64 {
        amount.saturating_mul(UNSMELT_FEE_PERCENTAGE as u64) / 100
    }

    pub fn can_mint_ingot(&self, amount: u64) -> bool {
        self.total_ingots_minted.saturating_add(amount) <= MAX_INGOT_SUPPLY
    }

    pub fn update_on_successful_smelt(&mut self, amount: u64) -> ProgramResult {
        self.total_ingots_minted = self.total_ingots_minted.saturating_add(amount);
        self.total_ore_locked = self.total_ore_locked.saturating_add(amount);

        if self.total_ingots_minted > MAX_INGOT_SUPPLY {
            return Err(ProgramError::AccountDataTooSmall);
        }
        Ok(())
    }

    pub fn update_on_unsmelt(&mut self, amount: u64, fee: u64) {
        self.total_ingots_minted = self.total_ingots_minted.saturating_sub(amount);
        self.total_ore_locked = self
            .total_ore_locked
            .saturating_sub(amount)
            .saturating_add(fee);
    }
}
