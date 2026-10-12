//! `AccountInfo` — как программа видит аккаунт во время исполнения.
//! Аналог `solana_account_info::AccountInfo`.
//!
//! Здесь собраны две «трудные» для новичка вещи Rust, без которых в Solana никуда:
//!
//! * **время жизни `'a`**. Память аккаунтов принадлежит рантайму, программа
//!   получает лишь ссылки на неё. `AccountInfo<'a>` не может пережить эту память —
//!   это и выражает `'a`;
//! * **`Rc<RefCell<…>>`**. Один и тот же аккаунт может прийти в инструкцию
//!   несколько раз (дубликаты) и передаваться в CPI, при этом его баланс и данные
//!   меняются через `&self`. `Rc` даёт нескольким клонам общий доступ, а `RefCell`
//!   проверяет правила заимствования во время выполнения: повторный
//!   `try_borrow_mut_*` на тот же аккаунт вернёт `AccountBorrowFailed`, а «голый»
//!   `borrow_mut()` в той же ситуации упадёт с паникой.

use std::cell::{Ref, RefCell, RefMut};
use std::fmt;
use std::rc::Rc;

use super::program_error::ProgramError;
use super::pubkey::Pubkey;

/// На сколько байт программа может увеличить данные аккаунта за один свой вызов
/// (как в Solana). Проверяет рантайм после вызова; через CPI это ещё и предел
/// размера нового аккаунта.
pub const MAX_PERMITTED_DATA_INCREASE: usize = 10 * 1024;

/// Аккаунт глазами программы.
///
/// Отличия от настоящего SDK (только ради того, чтобы обойтись без `unsafe`):
///
/// | поле    | здесь                           | в `solana_account_info`     |
/// |---------|---------------------------------|-----------------------------|
/// | `data`  | `Rc<RefCell<&'a mut Vec<u8>>>`  | `Rc<RefCell<&'a mut [u8]>>` |
/// | `owner` | `Rc<RefCell<&'a mut Pubkey>>`   | `&'a Pubkey`                |
///
/// Настоящие `resize()` и `assign()` меняют длину данных и владельца `unsafe`-записью
/// прямо в память рантайма. Для кода программы разницы почти нет: данные читаются
/// как `&[u8]` через deref, а владелец — методом [`AccountInfo::owner`]
/// (в настоящем SDK — полем `account.owner`).
#[derive(Clone)]
pub struct AccountInfo<'a> {
    /// Адрес аккаунта.
    pub key: &'a Pubkey,
    /// Подписан ли этот адрес в транзакции. Флаг ставит рантайм после проверки
    /// подписи. Программа может поменять его у своего клона, но рантайм в CPI
    /// сверяется со своими записями, так что проверки флага в программе достаточно.
    pub is_signer: bool,
    /// Разрешено ли менять аккаунт в этой инструкции.
    pub is_writable: bool,
    /// Баланс в лампортах (1 SOL = 1 000 000 000 лампортов).
    pub lamports: Rc<RefCell<&'a mut u64>>,
    /// Данные аккаунта — просто байты; их формат (у нас Borsh) определяет программа.
    pub data: Rc<RefCell<&'a mut Vec<u8>>>,
    /// Программа-владелец. Только она может менять данные и списывать лампорты.
    pub owner: Rc<RefCell<&'a mut Pubkey>>,
    /// Хранит ли аккаунт исполняемый код программы.
    pub executable: bool,
}

impl<'a> AccountInfo<'a> {
    /// Собрать `AccountInfo` из ссылок на память аккаунта. В Solana это делает
    /// рантайм (и макрос `entrypoint!`), программа их только получает.
    pub fn new(
        key: &'a Pubkey,
        is_signer: bool,
        is_writable: bool,
        lamports: &'a mut u64,
        data: &'a mut Vec<u8>,
        owner: &'a mut Pubkey,
        executable: bool,
    ) -> Self {
        Self {
            key,
            is_signer,
            is_writable,
            lamports: Rc::new(RefCell::new(lamports)),
            data: Rc::new(RefCell::new(data)),
            owner: Rc::new(RefCell::new(owner)),
            executable,
        }
    }

    /// Текущий баланс. `**`: `Ref<&mut u64>` → `&mut u64` → `u64`.
    pub fn lamports(&self) -> u64 {
        **self.lamports.borrow()
    }

    pub fn try_borrow_lamports(&self) -> Result<Ref<'_, &'a mut u64>, ProgramError> {
        self.lamports
            .try_borrow()
            .map_err(|_| ProgramError::AccountBorrowFailed)
    }

    /// Изменить баланс: `**account.try_borrow_mut_lamports()? -= amount;`
    pub fn try_borrow_mut_lamports(&self) -> Result<RefMut<'_, &'a mut u64>, ProgramError> {
        self.lamports
            .try_borrow_mut()
            .map_err(|_| ProgramError::AccountBorrowFailed)
    }

    pub fn data_len(&self) -> usize {
        self.data.borrow().len()
    }

    pub fn data_is_empty(&self) -> bool {
        self.data.borrow().is_empty()
    }

    /// Прочитать данные: `let data = account.try_borrow_data()?;` — дальше `&data[..]`.
    pub fn try_borrow_data(&self) -> Result<Ref<'_, &'a mut Vec<u8>>, ProgramError> {
        self.data
            .try_borrow()
            .map_err(|_| ProgramError::AccountBorrowFailed)
    }

    pub fn try_borrow_mut_data(&self) -> Result<RefMut<'_, &'a mut Vec<u8>>, ProgramError> {
        self.data
            .try_borrow_mut()
            .map_err(|_| ProgramError::AccountBorrowFailed)
    }

    /// Программа-владелец (в настоящем SDK — поле `account.owner`).
    pub fn owner(&self) -> Pubkey {
        **self.owner.borrow()
    }

    /// Изменить размер данных: обрезать или дополнить нулями. Лимиты (+10 КиБ за вызов,
    /// не больше 10 МиБ всего) проверит рантайм, когда программа завершится.
    pub fn resize(&self, new_len: usize) -> Result<(), ProgramError> {
        self.try_borrow_mut_data()?.resize(new_len, 0);
        Ok(())
    }

    /// Сменить владельца. Рантайм разрешит это, только если аккаунт writable,
    /// принадлежит вызывающей программе, а его данные пусты или обнулены. Так System
    /// Program «дарит» только что созданный аккаунт вашей программе.
    pub fn assign(&self, new_owner: &Pubkey) {
        **self.owner.borrow_mut() = *new_owner;
    }
}

/// `{:?}` для отладки. Настоящий SDK печатает ещё первые 64 байта данных,
/// здесь — только их длину.
impl fmt::Debug for AccountInfo<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AccountInfo")
            .field("key", self.key)
            .field("owner", &self.owner())
            .field("is_signer", &self.is_signer)
            .field("is_writable", &self.is_writable)
            .field("executable", &self.executable)
            .field("lamports", &self.lamports())
            .field("data.len", &self.data_len())
            .finish_non_exhaustive()
    }
}

/// Взять следующий аккаунт из итератора или вернуть `NotEnoughAccountKeys`.
///
/// Классический способ разобрать аккаунты в `process_instruction`:
///
/// ```
/// use piggy::sdk::account_info::{AccountInfo, next_account_info};
/// use piggy::sdk::program_error::ProgramError;
///
/// fn process(accounts: &[AccountInfo]) -> Result<(), ProgramError> {
///     let accounts_iter = &mut accounts.iter();
///     let _payer = next_account_info(accounts_iter)?;
///     let _vault = next_account_info(accounts_iter)?;
///     Ok(())
/// }
///
/// // Клиент не передал аккаунты — ошибка, а не паника на `accounts[0]`.
/// assert_eq!(process(&[]), Err(ProgramError::NotEnoughAccountKeys));
/// ```
///
/// Два времени жизни: `'a` — ссылки на `AccountInfo` в срезе,
/// `'b` — память самих аккаунтов (она живёт дольше среза).
pub fn next_account_info<'a, 'b, I: Iterator<Item = &'a AccountInfo<'b>>>(
    iter: &mut I,
) -> Result<I::Item, ProgramError> {
    iter.next().ok_or(ProgramError::NotEnoughAccountKeys)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clones_share_the_same_memory() {
        let key = Pubkey::default();
        let (mut lamports, mut data, mut owner) = (100, vec![0u8; 4], Pubkey::default());
        let account = AccountInfo::new(
            &key,
            false,
            true,
            &mut lamports,
            &mut data,
            &mut owner,
            false,
        );

        let clone = account.clone();
        **clone.try_borrow_mut_lamports().unwrap() -= 30;
        clone.try_borrow_mut_data().unwrap()[0] = 42;

        // Изменения через клон видны в оригинале: Rc указывает на одну и ту же ячейку.
        assert_eq!(account.lamports(), 70);
        assert_eq!(account.try_borrow_data().unwrap()[0], 42);
    }

    #[test]
    fn double_mutable_borrow_is_an_error_not_ub() {
        let key = Pubkey::default();
        let (mut lamports, mut data, mut owner) = (1, vec![], Pubkey::default());
        let account = AccountInfo::new(
            &key,
            false,
            true,
            &mut lamports,
            &mut data,
            &mut owner,
            false,
        );

        let _first = account.try_borrow_mut_lamports().unwrap();
        assert_eq!(
            account.try_borrow_mut_lamports().err(),
            Some(ProgramError::AccountBorrowFailed)
        );
    }

    #[test]
    fn next_account_info_reports_missing_accounts() {
        let accounts: [AccountInfo; 0] = [];
        let mut iter = accounts.iter();
        assert_eq!(
            next_account_info(&mut iter).err(),
            Some(ProgramError::NotEnoughAccountKeys)
        );
    }
}
