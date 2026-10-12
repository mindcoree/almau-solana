//! Состояние, которое программа хранит в данных аккаунта-копилки.

use borsh::{BorshDeserialize, BorshSerialize};

use super::VAULT_SEED;
use super::error::VaultError;
use crate::sdk::account_info::AccountInfo;
use crate::sdk::program_error::{ProgramError, ProgramResult};
use crate::sdk::pubkey::Pubkey;

/// Данные копилки. Раскладка байт в аккаунте (57 байт):
///
/// | байты  | поле              | тип             |
/// |--------|-------------------|-----------------|
/// | 0..8   | дискриминатор     | `[u8; 8]`       |
/// | 8..40  | `owner`           | `Pubkey`        |
/// | 40     | `bump`            | `u8`            |
/// | 41..49 | `total_deposited` | `u64` LE        |
/// | 49..57 | `total_withdrawn` | `u64` LE        |
///
/// Размер фиксированный (нет `Vec`/`String`), поэтому место в аккаунте
/// выделяется один раз при создании.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Vault {
    /// Кто может снимать и закрывать.
    pub owner: Pubkey,
    /// Канонический bump PDA — сохраняем, чтобы не искать его заново при каждой проверке.
    pub bump: u8,
    /// Сколько лампортов положили за всё время.
    pub total_deposited: u64,
    /// Сколько лампортов сняли за всё время.
    pub total_withdrawn: u64,
}

/// Общий интерфейс «типизированного» аккаунта программы.
///
/// В Anchor всё это генерирует атрибут `#[account]`, в SPL похожую роль играет трейт
/// `Pack`. Здесь видно устройство: ассоциированные константы, методы по умолчанию
/// и супертрейты (`AccountState` можно реализовать только для Borsh-типа).
pub trait AccountState: BorshSerialize + BorshDeserialize {
    /// 8 байт в начале данных — «тип» аккаунта. Без него программу можно обмануть,
    /// подсунув аккаунт другого типа с подходящим размером (type confusion).
    const DISCRIMINATOR: [u8; 8];
    /// Размер полей без дискриминатора.
    const SIZE: usize;
    /// Полный размер данных аккаунта. Константа по умолчанию, вычисленная из других.
    const LEN: usize = 8 + Self::SIZE;

    /// Прочитать состояние из аккаунта: владелец → дискриминатор → Borsh.
    fn load(account: &AccountInfo, program_id: &Pubkey) -> Result<Self, ProgramError> {
        // Данным можно верить, только если аккаунт принадлежит нашей программе:
        // иначе кто угодно создаст аккаунт с «правильными» байтами в своей программе.
        if account.owner() != *program_id {
            return Err(ProgramError::IncorrectProgramId);
        }
        let data = account.try_borrow_data()?;
        Self::try_from_bytes(&data)
        // `data` (Ref) освобождается здесь — заимствование не «утекает» из функции.
    }

    /// Разобрать байты данных аккаунта.
    fn try_from_bytes(data: &[u8]) -> Result<Self, ProgramError> {
        let (discriminator, mut body) = data
            .split_at_checked(8)
            .ok_or(ProgramError::AccountDataTooSmall)?;
        if discriminator != Self::DISCRIMINATOR {
            return Err(ProgramError::InvalidAccountData);
        }
        // `deserialize` (а не `try_from_slice`) не требует, чтобы байты кончились
        // ровно на последнем поле: аккаунт может быть больше структуры.
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
        // `&mut [u8]` реализует `std::io::Write` — Borsh пишет прямо в память аккаунта.
        self.serialize(&mut body)
            .map_err(|_| ProgramError::BorshIoError)
    }
}

impl AccountState for Vault {
    /// `sha256("account:Vault")[..8]` — ровно так считает Anchor (проверено тестом).
    const DISCRIMINATOR: [u8; 8] = [211, 8, 232, 43, 2, 152, 117, 119];
    const SIZE: usize = 32 + 1 + 8 + 8;
}

impl Vault {
    /// Проверить, что `address` — действительно PDA этой копилки.
    ///
    /// Bump уже известен, поэтому `create_program_address` (один хеш) вместо
    /// `find_program_address` (перебор до 255 хешей) — это экономит compute units.
    pub fn assert_address(&self, address: &Pubkey, program_id: &Pubkey) -> ProgramResult {
        let expected = Pubkey::create_program_address(
            &[VAULT_SEED, self.owner.as_ref(), &[self.bump]],
            program_id,
        )?;
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
        assert_eq!(borsh::to_vec(&vault).unwrap().len(), Vault::SIZE);

        let mut data = Vault::DISCRIMINATOR.to_vec();
        data.extend(borsh::to_vec(&vault).unwrap());
        assert_eq!(data[40], 254); // bump лежит сразу после owner
        assert_eq!(Vault::try_from_bytes(&data), Ok(vault));
    }

    #[test]
    fn wrong_discriminator_is_rejected() {
        let data = [0u8; 57];
        assert_eq!(
            Vault::try_from_bytes(&data),
            Err(ProgramError::InvalidAccountData)
        );
        assert_eq!(
            Vault::try_from_bytes(&data[..3]),
            Err(ProgramError::AccountDataTooSmall)
        );
    }
}
