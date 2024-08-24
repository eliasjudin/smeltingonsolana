use solana_program::{
    instruction::{AccountMeta, Instruction},
    program_error::ProgramError,
    pubkey::Pubkey,
};

#[derive(Debug)]
pub enum SmeltingInstruction {
    Smelt { amount: u64 },
    Unsmelt { amount: u64 },
    MintIngot { amount: u64 },
    TransferOre { amount: u64 },
    TransferIngot { amount: u64 },
}

impl SmeltingInstruction {
    pub fn unpack(input: &[u8]) -> Result<Self, ProgramError> {
        let (&tag, rest) = input
            .split_first()
            .ok_or(ProgramError::InvalidInstructionData)?;
        let amount = u64::from_le_bytes(rest.try_into().unwrap());

        Ok(match tag {
            0 => Self::Smelt { amount },
            1 => Self::Unsmelt { amount },
            2 => Self::MintIngot { amount },
            3 => Self::TransferOre { amount },
            4 => Self::TransferIngot { amount },
            _ => return Err(ProgramError::InvalidInstructionData),
        })
    }
}
