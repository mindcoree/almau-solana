//! Инструкция — «вызов функции» программы: кого вызываем, с какими аккаунтами и данными.
//! Аналог `solana_instruction::{Instruction, AccountMeta}`.

use borsh::{BorshDeserialize, BorshSerialize};

use super::pubkey::Pubkey;

/// Аккаунт в инструкции и права на него. Программа получит его как `AccountInfo`
/// с теми же флагами `is_signer` / `is_writable`.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AccountMeta {
    pub pubkey: Pubkey,
    pub is_signer: bool,
    pub is_writable: bool,
}

impl AccountMeta {
    /// Изменяемый аккаунт.
    pub fn new(pubkey: Pubkey, is_signer: bool) -> Self {
        Self {
            pubkey,
            is_signer,
            is_writable: true,
        }
    }

    /// Аккаунт только для чтения.
    pub fn new_readonly(pubkey: Pubkey, is_signer: bool) -> Self {
        Self {
            pubkey,
            is_signer,
            is_writable: false,
        }
    }
}

/// Все аккаунты, которые программа прочитает или изменит, должны быть перечислены
/// заранее — поэтому Solana может исполнять транзакции с разными аккаунтами параллельно.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Instruction {
    pub program_id: Pubkey,
    pub accounts: Vec<AccountMeta>,
    pub data: Vec<u8>,
}

impl Instruction {
    /// Данные уже закодированы вручную (так кодирует инструкции System Program).
    pub fn new_with_bytes(program_id: Pubkey, data: &[u8], accounts: Vec<AccountMeta>) -> Self {
        Self {
            program_id,
            accounts,
            data: data.to_vec(),
        }
    }

    /// Данные — любой тип с `BorshSerialize` (обобщённая функция с ограничением трейта).
    pub fn new_with_borsh<T: BorshSerialize>(
        program_id: Pubkey,
        data: &T,
        accounts: Vec<AccountMeta>,
    ) -> Self {
        Self {
            program_id,
            accounts,
            data: borsh::to_vec(data).expect("Borsh serialization into Vec never fails"),
        }
    }
}
