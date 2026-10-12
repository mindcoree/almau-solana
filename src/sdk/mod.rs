//! Мини-`solana_program`: то, что видит программа во время исполнения on-chain.
//!
//! Имена, типы и сигнатуры совпадают с настоящими крейтами (`solana-pubkey`,
//! `solana-account-info`, `solana-program-error`, `solana-instruction`,
//! `solana-cpi`, `solana-msg`, `solana-rent`). Поэтому код из [`crate::vault`]
//! переносится на настоящую Solana почти одной заменой `use` — таблица
//! соответствий есть в README.

pub mod account_info;
pub mod entrypoint;
pub mod instruction;
pub mod log;
pub mod program;
pub mod program_error;
pub mod pubkey;
pub mod rent;

/// Проверить условие, иначе вернуть ошибку из функции — как `require!` в Anchor
/// (в `solana_program` такого макроса нет, но его пишут почти в каждом проекте).
///
/// `require!(amount > 0, VaultError::ZeroAmount);` разворачивается в
/// `if !(amount > 0) { return Err(VaultError::ZeroAmount.into()); }`.
///
/// `.into()` превращает любую ошибку, для которой есть `From<…> for ProgramError`,
/// в `ProgramError` — поэтому макрос принимает и `ProgramError`, и `VaultError`.
///
/// ```
/// use piggy::require;
/// use piggy::sdk::program_error::ProgramError;
/// use piggy::vault::error::VaultError;
///
/// fn check(amount: u64) -> Result<(), ProgramError> {
///     require!(amount > 0, VaultError::ZeroAmount);
///     Ok(())
/// }
///
/// assert_eq!(check(0), Err(ProgramError::Custom(6002)));
/// assert_eq!(check(5), Ok(()));
/// ```
#[macro_export]
macro_rules! require {
    ($condition:expr, $error:expr $(,)?) => {
        if !($condition) {
            return Err($error.into());
        }
    };
}
