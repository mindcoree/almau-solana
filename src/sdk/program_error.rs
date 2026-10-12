//! Ошибки программы. Аналог `solana_program_error::{ProgramError, ProgramResult}`.
//!
//! Программа возвращает `Result<(), ProgramError>`. Свои ошибки программа
//! заворачивает в `ProgramError::Custom(код)` через `impl From<MyError> for ProgramError`
//! — тогда работает оператор `?` (см. `vault/error.rs`).

use thiserror::Error;

use super::pubkey::PubkeyError;

/// Результат инструкции: `Ok(())` или ошибка.
pub type ProgramResult = Result<(), ProgramError>;

/// Встроенные ошибки (подмножество настоящего `ProgramError`, тексты те же).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProgramError {
    /// Ошибка конкретной программы: только число. Текст ищут по коду на клиенте.
    #[error("Custom program error: {0:#x}")]
    Custom(u32),
    #[error("The arguments provided to a program instruction were invalid")]
    InvalidArgument,
    #[error("An instruction's data contents was invalid")]
    InvalidInstructionData,
    #[error("An account's data contents was invalid")]
    InvalidAccountData,
    #[error("An account's data was too small")]
    AccountDataTooSmall,
    #[error("An account's balance was too small to complete the instruction")]
    InsufficientFunds,
    #[error("The account did not have the expected program id")]
    IncorrectProgramId,
    #[error("A signature was required but not found")]
    MissingRequiredSignature,
    #[error("An initialize instruction was sent to an account that has already been initialized")]
    AccountAlreadyInitialized,
    #[error("An attempt to operate on an account that hasn't been initialized")]
    UninitializedAccount,
    #[error("The instruction expected additional account keys")]
    NotEnoughAccountKeys,
    #[error("Failed to borrow a reference to account data, already borrowed")]
    AccountBorrowFailed,
    #[error("Length of the seed is too long for address generation")]
    MaxSeedLengthExceeded,
    #[error("Provided seeds do not result in a valid address")]
    InvalidSeeds,
    #[error("IO Error")]
    BorshIoError,
    #[error("An account does not have enough lamports to be rent-exempt")]
    AccountNotRentExempt,
    #[error("Provided owner is not allowed")]
    IllegalOwner,
    #[error("Account data reallocation was invalid")]
    InvalidRealloc,
    #[error("Program arithmetic overflowed")]
    ArithmeticOverflow,
}

/// Ошибки PDA превращаются в ошибки программы — и `?` работает без `map_err`.
impl From<PubkeyError> for ProgramError {
    fn from(error: PubkeyError) -> Self {
        match error {
            PubkeyError::MaxSeedLengthExceeded => Self::MaxSeedLengthExceeded,
            PubkeyError::InvalidSeeds => Self::InvalidSeeds,
        }
    }
}
