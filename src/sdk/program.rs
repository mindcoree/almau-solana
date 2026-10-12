//! CPI (Cross-Program Invocation) — вызов другой программы из своей.
//! Аналог `solana_cpi::{invoke, invoke_signed}`.
//!
//! Через CPI программа делает то, чего не может сама. Например, списать SOL
//! с кошелька пользователя может только System Program (она владелец кошелька),
//! поэтому `Deposit` вызывает её инструкцию `transfer`.

use super::account_info::AccountInfo;
use super::instruction::Instruction;
use super::program_error::ProgramResult;

/// Вызвать другую программу. Подписи, которые есть у нас (`is_signer = true`),
/// «пробрасываются» в вызов — новых подписей добавить нельзя.
pub fn invoke(instruction: &Instruction, account_infos: &[AccountInfo]) -> ProgramResult {
    invoke_signed(instruction, account_infos, &[])
}

/// Вызвать другую программу и «подписать» за свои PDA.
///
/// `signers_seeds` — список наборов сидов (у каждого набора последний сид — bump):
///
/// ```text
/// invoke_signed(&ix, &[payer, vault, system_program], &[&[b"vault", owner.as_ref(), &[bump]]])?;
/// ```
///
/// (Полный пример — `process_initialize` в `vault/processor.rs`.)
///
/// Приватного ключа у PDA нет. Рантайм сам пересчитает адрес из сидов и
/// **program id вызывающей программы** и, если он совпал с аккаунтом в
/// инструкции, пометит аккаунт подписанным. Чужие PDA так подписать нельзя.
///
/// Тип `&[&[&[u8]]]` читается изнутри наружу: `&[u8]` — один сид,
/// `&[&[u8]]` — набор сидов одного PDA, `&[&[&[u8]]]` — несколько PDA.
pub fn invoke_signed(
    instruction: &Instruction,
    account_infos: &[AccountInfo],
    signers_seeds: &[&[&[u8]]],
) -> ProgramResult {
    // Как в настоящем SDK: перед вызовом убеждаемся, что никто не держит
    // заимствований данных и лампортов аккаунтов, которые получит другая программа.
    // Типичная ошибка — держать `try_borrow_mut_data()` через CPI → AccountBorrowFailed.
    for meta in &instruction.accounts {
        if let Some(account) = account_infos.iter().find(|info| *info.key == meta.pubkey) {
            if meta.is_writable {
                let _ = account.try_borrow_mut_lamports()?;
                let _ = account.try_borrow_mut_data()?;
            } else {
                let _ = account.try_borrow_lamports()?;
                let _ = account.try_borrow_data()?;
            }
        }
    }

    // В Solana здесь системный вызов `sol_invoke_signed_rust` — дальше работает валидатор.
    crate::runtime::syscall_invoke_signed(instruction, account_infos, signers_seeds)
}
