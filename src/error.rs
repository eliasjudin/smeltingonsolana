use solana_program::program_error::ProgramError;
use thiserror::Error;

#[derive(Error, Debug, Copy, Clone)]
pub enum SmeltingError {
    #[error("Max supply of INGOT tokens exceeded")]
    MaxSupplyExceeded,
    #[error("Insufficient balance")]
    InsufficientBalance,
    #[error("Invalid instruction")]
    InvalidInstruction,
}

impl From<SmeltingError> for ProgramError {
    fn from(e: SmeltingError) -> Self {
        ProgramError::Custom(e as u32)
    }
}
