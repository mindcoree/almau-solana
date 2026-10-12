//! Инструкции программы: что можно вызвать, с какими аккаунтами и данными.
//!
//! Данные инструкции кодируются Borsh: первый байт — номер варианта `enum`,
//! дальше поля варианта в little-endian. Например,
//! `Deposit { amount: 1_000_000_000 }` → `[1, 0, 202, 154, 59, 0, 0, 0, 0]`.
//! Клиент на JS (`client/demo.mjs`) собирает ровно эти байты.

use borsh::{BorshDeserialize, BorshSerialize};
use solana_instruction::{AccountMeta, Instruction};
use solana_pubkey::Pubkey;
use solana_system_interface::program as system_program;

use crate::{ID, find_vault_address};

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum VaultInstruction {
    /// Создать копилку.
    ///
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

// --- Конструкторы для Rust-клиента --------------------------------------------------

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
        assert_eq!(bytes, [1, 0, 202, 154, 59, 0, 0, 0, 0]);
        assert_eq!(borsh::to_vec(&VaultInstruction::Close).unwrap(), [3]);
    }

    #[test]
    fn garbage_is_rejected() {
        assert!(VaultInstruction::try_from_slice(&[9]).is_err());
        assert!(VaultInstruction::try_from_slice(&[1, 0, 0]).is_err());
        assert!(VaultInstruction::try_from_slice(&[3, 0]).is_err());
    }

    #[test]
    fn instruction_carries_program_id_and_accounts() {
        let owner = Pubkey::new_from_array([7; 32]);
        let ix = deposit(&owner, &owner, 5);
        assert_eq!(ix.program_id, ID);
        assert_eq!(ix.accounts.len(), 3);
        assert_eq!(ix.data, [1, 5, 0, 0, 0, 0, 0, 0, 0]);
    }
}
