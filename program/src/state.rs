//! Состояние, которое программа хранит в данных аккаунта-копилки.

use borsh::{BorshDeserialize, BorshSerialize};
use solana_account_info::AccountInfo;
use solana_program_error::{ProgramError, ProgramResult};
use solana_pubkey::Pubkey;

use crate::VAULT_SEED;
use crate::error::VaultError;

/// Данные копилки. Раскладка байт в аккаунте (57 байт):
///
/// | байты  | поле              | тип             |
/// |--------|-------------------|-----------------|
/// | 0..8   | дискриминатор     | `[u8; 8]`       |
/// | 8..40  | `owner`           | `Pubkey`        |
/// | 40     | `bump`            | `u8`            |
/// | 41..49 | `total_deposited` | `u64` LE        |
/// | 49..57 | `total_withdrawn` | `u64` LE        |
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Vault {
    pub owner: Pubkey,
    pub bump: u8,
    pub total_deposited: u64,
    pub total_withdrawn: u64,
}

/// Общий интерфейс «типизированного» аккаунта программы (в Anchor — `#[account]`).
pub trait AccountState: BorshSerialize + BorshDeserialize {
    const DISCRIMINATOR: [u8; 8];
    const SIZE: usize;
    const LEN: usize = 8 + Self::SIZE;

    /// Прочитать состояние из аккаунта: владелец → дискриминатор → Borsh.
    fn load(account: &AccountInfo, program_id: &Pubkey) -> Result<Self, ProgramError> {
        // Было в piggy: `account.owner()`. В настоящем `AccountInfo` владелец —
        // поле `owner: &Pubkey`, которое рантайм выставляет из своих записей.
        if account.owner != program_id {
            return Err(ProgramError::IncorrectProgramId);
        }
        let data = account.try_borrow_data()?;
        Self::try_from_bytes(&data)
    }

    /// Разобрать байты данных аккаунта.
    fn try_from_bytes(data: &[u8]) -> Result<Self, ProgramError> {
        let (discriminator, mut body) = data
            .split_at_checked(8)
            .ok_or(ProgramError::AccountDataTooSmall)?;
        if discriminator != Self::DISCRIMINATOR {
            return Err(ProgramError::InvalidAccountData);
        }
        Self::deserialize(&mut body).map_err(|_| ProgramError::InvalidAccountData)
    }

    /// Записать состояние в данные аккаунта.
    fn save(&self, account: &AccountInfo) -> ProgramResult {
        let mut data = account.try_borrow_mut_data()?;
        if data.len() < Self::LEN {
            return Err(ProgramError::AccountDataTooSmall);
        }
        let (discriminator, mut body) = data.split_at_mut(8);
        discriminator.copy_from_slice(&Self::DISCRIMINATOR);
        self.serialize(&mut body)
            .map_err(|_| ProgramError::BorshIoError)
    }
}

impl AccountState for Vault {
    /// `sha256("account:Vault")[..8]` — ровно так считает Anchor.
    const DISCRIMINATOR: [u8; 8] = [211, 8, 232, 43, 2, 152, 117, 119];
    const SIZE: usize = 32 + 1 + 8 + 8;
}

impl Vault {
    /// Проверить, что `address` — действительно PDA этой копилки.
    pub fn assert_address(&self, address: &Pubkey, program_id: &Pubkey) -> ProgramResult {
        let expected = Pubkey::create_program_address(
            &[VAULT_SEED, self.owner.as_ref(), &[self.bump]],
            program_id,
        )
        .map_err(|_| ProgramError::InvalidSeeds)?;
        if expected != *address {
            return Err(VaultError::InvalidVaultAddress.into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use sha2::{Digest, Sha256};

    use super::*;

    #[test]
    fn discriminator_is_anchor_style_hash() {
        let hash = Sha256::digest(b"account:Vault");
        assert_eq!(hash[..8], Vault::DISCRIMINATOR);
    }

    #[test]
    fn layout_is_57_bytes_and_roundtrips() {
        let vault = Vault {
            owner: Pubkey::new_from_array([1; 32]),
            bump: 254,
            total_deposited: 2_500_000_000,
            total_withdrawn: 500_000_000,
        };
        assert_eq!(Vault::LEN, 57);
        let mut data = Vault::DISCRIMINATOR.to_vec();
        data.extend(borsh::to_vec(&vault).unwrap());
        assert_eq!(Vault::try_from_bytes(&data), Ok(vault));
    }

    #[test]
    fn wrong_discriminator_is_rejected() {
        assert_eq!(
            Vault::try_from_bytes(&[0u8; 57]),
            Err(ProgramError::InvalidAccountData)
        );
        assert_eq!(
            Vault::try_from_bytes(&[0u8; 3]),
            Err(ProgramError::AccountDataTooSmall)
        );
    }
}
