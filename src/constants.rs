pub mod constants {
    pub const SMELTING_SUCCESS_RATE: u8 = 80;
    pub const WRAPPED_MINT_SEED: &[u8] = b"mint";
    pub const BACKPOINTER_SEED: &[u8] = b"backpointer";
    pub const AUTHORITY_SEED: &[u8] = b"authority";
    pub const MAX_AMOUNT: u64 = 1_000_000_000; // 1 billion tokens
    pub const MAX_INGOT_SUPPLY: u64 = 21_000_000;
    pub const UNSMELT_FEE_PERCENTAGE: u8 = 5;
}
