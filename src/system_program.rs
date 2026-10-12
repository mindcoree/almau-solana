//! System Program — встроенная программа по адресу `11111111111111111111111111111111`.
//!
//! Она владеет всеми кошельками: создаёт аккаунты и переводит SOL. В настоящем
//! валидаторе она тоже написана на Rust внутри самого рантайма (builtin).
//!
//! Формат инструкций совместим с настоящим: номер инструкции — `u32` little-endian
//! (так кодирует bincode), затем поля. Байты здесь собираются и разбираются
//! вручную — полезный навык: в Pinocchio-программах Borsh часто не используют.

use thiserror::Error;

use crate::msg;
use crate::sdk::account_info::AccountInfo;
use crate::sdk::program_error::{ProgramError, ProgramResult};
use crate::sdk::pubkey::Pubkey;

crate::declare_id!("11111111111111111111111111111111");

/// Максимальный размер данных аккаунта — 10 МиБ.
pub const MAX_PERMITTED_DATA_LENGTH: u64 = 10 * 1024 * 1024;

/// Ошибки System Program. Номера совпадают с настоящими: `custom program error: 0x1`
/// в логах — это как раз «не хватает SOL» при переводе.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[repr(u32)]
pub enum SystemError {
    #[error("an account with the same address already exists")]
    AccountAlreadyInUse = 0,
    #[error("account does not have enough SOL to perform the operation")]
    ResultWithNegativeLamports = 1,
    #[error("cannot allocate account data of this length")]
    InvalidAccountDataLength = 3,
}

impl From<SystemError> for ProgramError {
    fn from(error: SystemError) -> Self {
        ProgramError::Custom(error as u32)
    }
}

/// Код ошибки из лога → понятная ошибка (для клиента).
impl TryFrom<u32> for SystemError {
    type Error = u32;

    fn try_from(code: u32) -> Result<Self, Self::Error> {
        match code {
            0 => Ok(Self::AccountAlreadyInUse),
            1 => Ok(Self::ResultWithNegativeLamports),
            3 => Ok(Self::InvalidAccountDataLength),
            other => Err(other),
        }
    }
}

/// Инструкции, которые мы поддерживаем (в настоящей System Program их больше десятка).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SystemInstruction {
    /// Создать аккаунт: перевести `lamports`, выделить `space` байт, отдать во владение `owner`.
    CreateAccount {
        lamports: u64,
        space: u64,
        owner: Pubkey,
    },
    /// Перевести лампорты.
    Transfer { lamports: u64 },
}

impl SystemInstruction {
    const CREATE_ACCOUNT: u32 = 0;
    const TRANSFER: u32 = 2;

    /// Закодировать в байты: `[номер: u32 LE][поля: LE]`.
    pub fn pack(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(52);
        match self {
            Self::CreateAccount {
                lamports,
                space,
                owner,
            } => {
                data.extend_from_slice(&Self::CREATE_ACCOUNT.to_le_bytes());
                data.extend_from_slice(&lamports.to_le_bytes());
                data.extend_from_slice(&space.to_le_bytes());
                data.extend_from_slice(owner.as_ref());
            }
            Self::Transfer { lamports } => {
                data.extend_from_slice(&Self::TRANSFER.to_le_bytes());
                data.extend_from_slice(&lamports.to_le_bytes());
            }
        }
        data
    }

    /// Разобрать байты. Любая нехватка байт → `InvalidInstructionData`, без паник.
    pub fn unpack(data: &[u8]) -> Result<Self, ProgramError> {
        let (tag, rest) = data
            .split_first_chunk::<4>()
            .ok_or(ProgramError::InvalidInstructionData)?;

        match u32::from_le_bytes(*tag) {
            Self::CREATE_ACCOUNT => {
                let (lamports, rest) = read_u64(rest)?;
                let (space, rest) = read_u64(rest)?;
                // Срез → массив фиксированной длины: try_into() проверит длину.
                let owner: [u8; 32] = rest
                    .get(..32)
                    .and_then(|bytes| bytes.try_into().ok())
                    .ok_or(ProgramError::InvalidInstructionData)?;
                Ok(Self::CreateAccount {
                    lamports,
                    space,
                    owner: Pubkey::new_from_array(owner),
                })
            }
            Self::TRANSFER => {
                let (lamports, _rest) = read_u64(rest)?;
                Ok(Self::Transfer { lamports })
            }
            _ => Err(ProgramError::InvalidInstructionData),
        }
    }
}

/// Прочитать `u64` (8 байт little-endian) и вернуть остаток среза.
fn read_u64(input: &[u8]) -> Result<(u64, &[u8]), ProgramError> {
    let (bytes, rest) = input
        .split_first_chunk::<8>()
        .ok_or(ProgramError::InvalidInstructionData)?;
    Ok((u64::from_le_bytes(*bytes), rest))
}

/// Конструкторы инструкций — как `solana_system_interface::instruction`.
/// В программе их обычно импортируют так:
/// `use crate::system_program::instruction as system_instruction;`
pub mod instruction {
    use super::{ID, SystemInstruction};
    use crate::sdk::instruction::{AccountMeta, Instruction};
    use crate::sdk::pubkey::Pubkey;

    /// Создать аккаунт `to`. Подписать должны оба: `from` платит, `to` — новый адрес.
    pub fn create_account(
        from: &Pubkey,
        to: &Pubkey,
        lamports: u64,
        space: u64,
        owner: &Pubkey,
    ) -> Instruction {
        let data = SystemInstruction::CreateAccount {
            lamports,
            space,
            owner: *owner,
        }
        .pack();
        Instruction::new_with_bytes(
            ID,
            &data,
            vec![AccountMeta::new(*from, true), AccountMeta::new(*to, true)],
        )
    }

    /// Перевести `lamports` с `from` (подписант) на `to`.
    pub fn transfer(from: &Pubkey, to: &Pubkey, lamports: u64) -> Instruction {
        let data = SystemInstruction::Transfer { lamports }.pack();
        Instruction::new_with_bytes(
            ID,
            &data,
            vec![AccountMeta::new(*from, true), AccountMeta::new(*to, false)],
        )
    }
}

/// Точка входа System Program (её вызывает рантайм).
pub fn process_instruction(
    _program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    match SystemInstruction::unpack(instruction_data)? {
        SystemInstruction::CreateAccount {
            lamports,
            space,
            owner,
        } => {
            let [from, to, ..] = accounts else {
                return Err(ProgramError::NotEnoughAccountKeys);
            };
            create_account(from, to, lamports, space, &owner)
        }
        SystemInstruction::Transfer { lamports } => {
            let [from, to, ..] = accounts else {
                return Err(ProgramError::NotEnoughAccountKeys);
            };
            transfer(from, to, lamports)
        }
    }
}

fn create_account(
    from: &AccountInfo,
    to: &AccountInfo,
    lamports: u64,
    space: u64,
    owner: &Pubkey,
) -> ProgramResult {
    // Новый адрес тоже должен «подписать»: обычный — ключом, PDA — через invoke_signed.
    if !to.is_signer {
        msg!("Allocate: 'to' account {} must sign", to.key);
        return Err(ProgramError::MissingRequiredSignature);
    }
    // Аккаунт уже существует, если у него есть лампорты, данные или другой владелец.
    if to.lamports() > 0 || !to.data_is_empty() || to.owner() != ID {
        msg!("Create Account: account {} already in use", to.key);
        return Err(SystemError::AccountAlreadyInUse.into());
    }
    if space > MAX_PERMITTED_DATA_LENGTH {
        msg!(
            "Allocate: requested {}, max allowed {}",
            space,
            MAX_PERMITTED_DATA_LENGTH
        );
        return Err(SystemError::InvalidAccountDataLength.into());
    }

    transfer(from, to, lamports)?;
    let space = usize::try_from(space).map_err(|_| SystemError::InvalidAccountDataLength)?;
    to.resize(space)?;
    to.assign(owner);
    Ok(())
}

fn transfer(from: &AccountInfo, to: &AccountInfo, lamports: u64) -> ProgramResult {
    if !from.is_signer {
        msg!("Transfer: `from` account {} must sign", from.key);
        return Err(ProgramError::MissingRequiredSignature);
    }
    // С аккаунта, у которого есть данные (например, PDA вашей программы), System
    // Program переводить не станет — такие аккаунты «опустошает» их владелец сам.
    if !from.data_is_empty() {
        msg!("Transfer: `from` must not carry data");
        return Err(ProgramError::InvalidArgument);
    }
    if lamports > from.lamports() {
        msg!(
            "Transfer: insufficient lamports {}, need {}",
            from.lamports(),
            lamports
        );
        return Err(SystemError::ResultWithNegativeLamports.into());
    }

    **from.try_borrow_mut_lamports()? -= lamports;
    let mut to_lamports = to.try_borrow_mut_lamports()?;
    **to_lamports = to_lamports
        .checked_add(lamports)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_format_matches_bincode_layout() {
        // Эти байты совпадают с `solana_system_interface::instruction::transfer(…).data`.
        let transfer = SystemInstruction::Transfer {
            lamports: 1_000_000,
        }
        .pack();
        assert_eq!(transfer, [2, 0, 0, 0, 64, 66, 15, 0, 0, 0, 0, 0]);

        let owner = Pubkey::new_from_array([9; 32]);
        let create = SystemInstruction::CreateAccount {
            lamports: 5,
            space: 57,
            owner,
        };
        let bytes = create.pack();
        assert_eq!(bytes.len(), 4 + 8 + 8 + 32);
        assert_eq!(SystemInstruction::unpack(&bytes), Ok(create));
    }

    #[test]
    fn unpack_rejects_truncated_data() {
        assert_eq!(
            SystemInstruction::unpack(&[2, 0, 0, 0, 1]),
            Err(ProgramError::InvalidInstructionData)
        );
        assert_eq!(
            SystemInstruction::unpack(&[]),
            Err(ProgramError::InvalidInstructionData)
        );
    }
}
