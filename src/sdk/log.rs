//! Логи программы. Аналог `solana_msg::{msg, sol_log}`.
//!
//! Логи — главный инструмент отладки on-chain: `println!` там не работает,
//! а строки из `msg!` попадают в лог транзакции («Program log: …»),
//! который видно в эксплорере и в `solana logs`.

/// Записать строку в лог транзакции. В Solana это системный вызов `sol_log_`,
/// у нас — запись в журнал нашего рантайма.
pub fn sol_log(message: &str) {
    crate::runtime::syscall_log(&format!("Program log: {message}"));
}

/// `msg!("Deposited {} lamports", amount);`
///
/// Две ветки `macro_rules!` — ровно как в настоящем `solana_msg::msg!`:
/// одно выражение уходит в лог как есть, иначе аргументы идут в `format!`.
///
/// Осторожно, известная ловушка: `msg!("amount = {amount}")` — это *одно*
/// выражение, оно попадёт в первую ветку и в лог уйдёт буквально `{amount}`.
/// Пишите `msg!("amount = {}", amount)`.
#[macro_export]
macro_rules! msg {
    ($message:expr) => {
        $crate::sdk::log::sol_log($message)
    };
    ($($arg:tt)*) => {
        $crate::sdk::log::sol_log(&format!($($arg)*))
    };
}
