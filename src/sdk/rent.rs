//! Rent: сколько лампортов должно лежать на аккаунте, чтобы он мог существовать.
//! Аналог `solana_rent::Rent`.
//!
//! Место в состоянии сети не бесплатное. Аккаунт обязан держать неснимаемый
//! минимум, пропорциональный размеру данных («rent-exempt»). Этот депозит
//! возвращается целиком, когда аккаунт закрывают.

use super::program_error::ProgramError;

/// Служебные байты, которые сеть учитывает для любого аккаунта сверх данных.
pub const ACCOUNT_STORAGE_OVERHEAD: u64 = 128;
/// Лампорты за байт — значение по умолчанию в `solana-rent` 4.x.
pub const DEFAULT_LAMPORTS_PER_BYTE: u64 = 6_960;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rent {
    pub lamports_per_byte: u64,
}

impl Default for Rent {
    fn default() -> Self {
        Self {
            lamports_per_byte: DEFAULT_LAMPORTS_PER_BYTE,
        }
    }
}

impl Rent {
    /// Прочитать текущие параметры rent.
    ///
    /// В настоящей программе это sysvar: `use solana_program::sysvar::Sysvar;`
    /// и затем `Rent::get()?` (системный вызов к рантайму). Здесь всегда значения
    /// по умолчанию.
    pub fn get() -> Result<Self, ProgramError> {
        Ok(Self::default())
    }

    /// Минимальный баланс аккаунта с `data_len` байтами данных:
    /// `(128 + data_len) * 6960`. Для 0 байт — 890 880 лампортов (≈0.00089 SOL).
    pub fn minimum_balance(&self, data_len: usize) -> u64 {
        (ACCOUNT_STORAGE_OVERHEAD + data_len as u64) * self.lamports_per_byte
    }

    /// Хватает ли баланса, чтобы аккаунт с такими данными мог существовать.
    pub fn is_exempt(&self, balance: u64, data_len: usize) -> bool {
        balance >= self.minimum_balance(data_len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_solana_defaults() {
        let rent = Rent::default();
        assert_eq!(rent.minimum_balance(0), 890_880);
        assert!(rent.is_exempt(890_880, 0));
        assert!(!rent.is_exempt(890_879, 0));
    }
}
