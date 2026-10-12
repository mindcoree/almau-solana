//! Программа «Копилка» (vault) — настоящая нативная Solana-программа.
//!
//! Это `src/vault/` из piggy (неделя 3), перенесённый с учебного мини-SDK на
//! настоящие крейты `solana-*`. Логика не менялась; что пришлось поменять и
//! почему — в README рядом.
//!
//! Правила:
//! * у каждого пользователя может быть одна копилка — PDA по сидам `["vault", owner]`;
//! * положить SOL может кто угодно (`Deposit`);
//! * снять может только владелец, и не больше, чем лежит сверх rent-резерва (`Withdraw`);
//! * закрыть копилку и забрать всё, включая резерв, может только владелец (`Close`).

pub mod error;
pub mod instruction;
pub mod processor;
pub mod state;

use solana_pubkey::Pubkey;

// Адрес программы — публичный ключ из keys/program-keypair.json
// (`solana-keygen new -o keys/program-keypair.json`). С этим ключом программа
// и деплоится: `solana program deploy … --program-id keys/program-keypair.json`.
solana_pubkey::declare_id!("DiN5bMhqftannN64aRSfAT1dRdavM5xaqgS9GmJrex9c");

// Точка входа. Настоящий макрос генерирует `extern "C" fn entrypoint(input: *mut u8)`,
// который разбирает буфер рантайма в (program_id, аккаунты, данные) и вызывает нашу функцию.
// Фича `no-entrypoint` нужна, когда программу подключают как библиотеку.
// Настоящий макрос принимает только имя функции (`ident`), а не путь `processor::…`,
// как принимал учебный — поэтому функцию сначала импортируем.
#[cfg(not(feature = "no-entrypoint"))]
use processor::process_instruction;
#[cfg(not(feature = "no-entrypoint"))]
solana_program_entrypoint::entrypoint!(process_instruction);

/// Первый сид PDA копилки.
pub const VAULT_SEED: &[u8] = b"vault";

/// Адрес копилки владельца и канонический bump.
/// Одна и та же функция нужна и программе, и клиенту.
pub fn find_vault_address(owner: &Pubkey, program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[VAULT_SEED, owner.as_ref()], program_id)
}

/// Проверить условие, иначе вернуть ошибку — как `require!` в Anchor.
/// В настоящем SDK такого макроса нет, поэтому он переехал сюда из piggy как есть.
#[macro_export]
macro_rules! require {
    ($condition:expr, $error:expr $(,)?) => {
        if !($condition) {
            return Err($error.into());
        }
    };
}
