//! Программа «Копилка» (vault) — то, что в настоящем проекте компилируется
//! в байткод SBF (`cargo build-sbf`) и деплоится в сеть.
//!
//! Правила:
//! * у каждого пользователя может быть одна копилка — PDA по сидам `["vault", owner]`;
//! * положить SOL может кто угодно (`Deposit`);
//! * снять может только владелец, и не больше, чем лежит сверх rent-резерва (`Withdraw`);
//! * закрыть копилку и забрать всё, включая резерв, может только владелец (`Close`).
//!
//! Файлы устроены как в типичной нативной Solana-программе:
//! `instruction.rs` (что можно вызвать) → `processor.rs` (логика) →
//! `state.rs` (что хранится в аккаунте) → `error.rs` (коды ошибок).

pub mod error;
pub mod instruction;
pub mod processor;
pub mod state;

use crate::sdk::pubkey::Pubkey;

// Адрес программы. В настоящем проекте его выдаёт `solana-keygen new -o program-keypair.json`.
crate::declare_id!("Vau1tkczJmsqv3E7sVPgDnNZu3W2HjmSidktauy74DJ");

// Точка входа: рантайм будет вызывать `processor::process_instruction`.
crate::entrypoint!(processor::process_instruction);

/// Первый сид PDA копилки. Байтовая строка `b"…"` имеет тип `&[u8; 5]`
/// и приводится к срезу `&[u8]`.
pub const VAULT_SEED: &[u8] = b"vault";

/// Адрес копилки владельца и канонический bump.
/// Одна и та же функция нужна и программе, и клиенту.
pub fn find_vault_address(owner: &Pubkey, program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[VAULT_SEED, owner.as_ref()], program_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Эталоны посчитаны настоящим `solana_pubkey::Pubkey::find_program_address` (4.4).
    #[test]
    fn pda_matches_real_solana_vectors() {
        let vectors = [
            (1u8, "7ASgPZhNpHDCYFcDNJXvMg6xvCytVW9fJbvUkgTh8cBH", 252),
            (2, "FRQewVW16WxTEum7xBM2iDKvcgurWVRMevEBKcVBigxp", 255),
            (42, "CKhnAgjXc9w8DjrB9Vb14FnkmEUXMuq5jSGyKrabjeP5", 253),
        ];
        for (fill, expected_address, expected_bump) in vectors {
            let owner = Pubkey::new_from_array([fill; 32]);
            let (address, bump) = find_vault_address(&owner, &ID);
            assert_eq!(address.to_string(), expected_address);
            assert_eq!(bump, expected_bump);
        }
    }
}
