//! Точка входа программы. Аналог `solana_program_entrypoint`.

use super::account_info::AccountInfo;
use super::program_error::ProgramResult;
use super::pubkey::Pubkey;

/// Сигнатура `process_instruction` любой Solana-программы — тип «указатель на функцию».
///
/// Рантайм ничего не знает о типах конкретной программы: он передаёт адрес
/// программы, срез аккаунтов и сырые байты данных инструкции.
pub type ProcessInstruction = fn(&Pubkey, &[AccountInfo], &[u8]) -> ProgramResult;

/// `entrypoint!(process_instruction);`
///
/// В Solana макрос генерирует `extern "C" fn entrypoint(input: *mut u8) -> u64`,
/// который разбирает входной буфер в `(program_id, Vec<AccountInfo>, data)` и
/// вызывает вашу функцию. Нашему рантайму достаточно константы с указателем.
#[macro_export]
macro_rules! entrypoint {
    ($process_instruction:path) => {
        /// Точка входа программы — её вызывает рантайм.
        pub const ENTRYPOINT: $crate::sdk::entrypoint::ProcessInstruction = $process_instruction;
    };
}
