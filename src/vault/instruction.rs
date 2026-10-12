//! Инструкции программы: что можно вызвать, с какими аккаунтами и данными.
//!
//! Данные инструкции кодируются Borsh: первый байт — номер варианта `enum`,
//! дальше поля варианта в little-endian. Например,
//! `Deposit { amount: 1_000_000_000 }` → `[1, 0, 202, 154, 59, 0, 0, 0, 0]`.
//! (Anchor вместо одного байта пишет 8-байтовый дискриминатор `sha256("global:deposit")[..8]`.)

use borsh::{BorshDeserialize, BorshSerialize};

use super::{ID, find_vault_address};
use crate::sdk::instruction::{AccountMeta, Instruction};
use crate::sdk::pubkey::Pubkey;
use crate::system_program;

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum VaultInstruction {
    /// Создать копилку.
    ///
    /// Аккаунты:
    /// 0. `[signer, writable]` владелец — он же платит rent-депозит
    /// 1. `[writable]` копилка — PDA `["vault", owner]`
    /// 2. `[]` System Program
    Initialize,

    /// Положить `amount` лампортов в чужую или свою копилку.
    ///
    /// 0. `[signer, writable]` вносящий
    /// 1. `[writable]` копилка
    /// 2. `[]` System Program
    Deposit { amount: u64 },

    /// Снять `amount` лампортов. Только владелец.
    ///
    /// 0. `[signer, writable]` владелец
    /// 1. `[writable]` копилка
    Withdraw { amount: u64 },

    /// Закрыть копилку: все лампорты, включая rent-резерв, — владельцу.
    ///
    /// 0. `[signer, writable]` владелец
    /// 1. `[writable]` копилка
    Close,
}

// --- Конструкторы для клиента ------------------------------------------------------
// Клиент должен перечислить аккаунты в том же порядке, в каком их ждёт программа.
// В Anchor такие функции генерируются из IDL; здесь пишем руками.

pub fn initialize(owner: &Pubkey) -> Instruction {
    let (vault, _bump) = find_vault_address(owner, &ID);
    Instruction::new_with_borsh(
        ID,
        &VaultInstruction::Initialize,
        vec![
            AccountMeta::new(*owner, true),
            AccountMeta::new(vault, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
    )
}

/// `depositor` кладёт `amount` лампортов в копилку `vault_owner`.
pub fn deposit(depositor: &Pubkey, vault_owner: &Pubkey, amount: u64) -> Instruction {
    let (vault, _bump) = find_vault_address(vault_owner, &ID);
    Instruction::new_with_borsh(
        ID,
        &VaultInstruction::Deposit { amount },
        vec![
            AccountMeta::new(*depositor, true),
            AccountMeta::new(vault, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
    )
}

/// `signer` снимает `amount` из копилки `vault_owner`. Программа пропустит,
/// только если `signer == vault_owner` — но собрать такую инструкцию может кто угодно.
pub fn withdraw(signer: &Pubkey, vault_owner: &Pubkey, amount: u64) -> Instruction {
    let (vault, _bump) = find_vault_address(vault_owner, &ID);
    Instruction::new_with_borsh(
        ID,
        &VaultInstruction::Withdraw { amount },
        vec![
            AccountMeta::new(*signer, true),
            AccountMeta::new(vault, false),
        ],
    )
}

/// `signer` закрывает копилку `vault_owner`.
pub fn close(signer: &Pubkey, vault_owner: &Pubkey) -> Instruction {
    let (vault, _bump) = find_vault_address(vault_owner, &ID);
    Instruction::new_with_borsh(
        ID,
        &VaultInstruction::Close,
        vec![
            AccountMeta::new(*signer, true),
            AccountMeta::new(vault, false),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn borsh_wire_format() {
        let bytes = borsh::to_vec(&VaultInstruction::Deposit {
            amount: 1_000_000_000,
        })
        .unwrap();
        // [номер варианта][u64 little-endian]
        assert_eq!(bytes, [1, 0, 202, 154, 59, 0, 0, 0, 0]);
        assert_eq!(borsh::to_vec(&VaultInstruction::Close).unwrap(), [3]);

        let decoded = VaultInstruction::try_from_slice(&bytes).unwrap();
        assert_eq!(
            decoded,
            VaultInstruction::Deposit {
                amount: 1_000_000_000
            }
        );
    }

    #[test]
    fn garbage_is_rejected() {
        assert!(VaultInstruction::try_from_slice(&[9]).is_err()); // нет такого варианта
        assert!(VaultInstruction::try_from_slice(&[1, 0, 0]).is_err()); // обрезанный u64
        assert!(VaultInstruction::try_from_slice(&[3, 0]).is_err()); // лишние байты
    }
}
