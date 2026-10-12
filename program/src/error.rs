//! Коды ошибок программы.
//!
//! On-chain от ошибки остаётся только число: в логе будет
//! `custom program error: 0x1770`. Текст восстанавливает клиент по коду.

use solana_program_error::ProgramError;
use thiserror::Error;

/// Начинаем с 6000 (0x1770), как Anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[repr(u32)]
pub enum VaultError {
    #[error("снимать и закрывать копилку может только её владелец")]
    Unauthorized = 6000,
    #[error("адрес копилки не совпадает с PDA [\"vault\", owner]")]
    InvalidVaultAddress,
    #[error("сумма должна быть больше нуля")]
    ZeroAmount,
    #[error("в копилке недостаточно средств (rent-резерв снять нельзя, только закрыть копилку)")]
    InsufficientFunds,
}

impl VaultError {
    /// Все варианты — чтобы найти ошибку по коду без длинного `match`.
    pub const ALL: [VaultError; 4] = [
        Self::Unauthorized,
        Self::InvalidVaultAddress,
        Self::ZeroAmount,
        Self::InsufficientFunds,
    ];
}

/// Благодаря этому `?` и `require!` превращают `VaultError` в `ProgramError`.
impl From<VaultError> for ProgramError {
    fn from(error: VaultError) -> Self {
        ProgramError::Custom(error as u32)
    }
}

/// Код из лога → ошибка: `VaultError::try_from(0x1770)` == `Ok(Unauthorized)`.
impl TryFrom<u32> for VaultError {
    type Error = u32;

    fn try_from(code: u32) -> Result<Self, Self::Error> {
        Self::ALL
            .into_iter()
            .find(|error| *error as u32 == code)
            .ok_or(code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_roundtrip() {
        assert_eq!(VaultError::Unauthorized as u32, 0x1770);
        for error in VaultError::ALL {
            let ProgramError::Custom(code) = ProgramError::from(error) else {
                panic!("expected a custom error");
            };
            assert_eq!(VaultError::try_from(code), Ok(error));
        }
        assert_eq!(VaultError::try_from(42), Err(42));
    }
}
