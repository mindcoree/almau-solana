//! `Pubkey` — 32-байтовый адрес аккаунта, а также PDA (Program Derived Address).
//!
//! Аналог `solana_pubkey::Pubkey` (в новых версиях SDK это псевдоним
//! `solana_address::Address`). Алгоритм PDA здесь тот же, что в Solana, байт в байт:
//! это проверяют тесты с эталонными значениями из настоящего крейта.

use std::fmt;
use std::str::FromStr;

use borsh::{BorshDeserialize, BorshSerialize};
use curve25519_dalek::edwards::CompressedEdwardsY;
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Длина адреса в байтах.
pub const PUBKEY_BYTES: usize = 32;
/// Не больше 16 сидов на один PDA (включая bump)…
pub const MAX_SEEDS: usize = 16;
/// …и каждый сид не длиннее 32 байт.
pub const MAX_SEED_LEN: usize = 32;
/// Максимальная длина адреса в base58 (32 байта → до 44 символов).
const MAX_BASE58_LEN: usize = 44;
/// Константа, которая дописывается в конец хеша при вычислении PDA.
const PDA_MARKER: &[u8; 21] = b"ProgramDerivedAddress";

/// Адрес аккаунта: просто 32 байта.
///
/// Для кошелька это публичный ключ ed25519. Для PDA — хеш сидов, у которого
/// нет приватного ключа (подписать за такой адрес может только программа).
///
/// `derive` бесплатно даёт сравнение, копирование, хеширование и Borsh-сериализацию:
/// в Borsh `Pubkey` — это ровно 32 байта, без длины и заголовков.
#[derive(
    Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize,
)]
pub struct Pubkey([u8; PUBKEY_BYTES]);

/// Ошибки вычисления PDA (как `solana_pubkey::PubkeyError`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum PubkeyError {
    #[error("Length of the seed is too long for address generation")]
    MaxSeedLengthExceeded,
    #[error("Provided seeds do not result in a valid address")]
    InvalidSeeds,
}

/// Ошибка разбора строки base58 в адрес.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ParsePubkeyError {
    #[error("String is the wrong size")]
    WrongSize,
    #[error("Invalid Base58 string")]
    Invalid,
}

impl Pubkey {
    /// Адрес из готового массива байт. `const fn` — можно вызывать в константах.
    pub const fn new_from_array(bytes: [u8; PUBKEY_BYTES]) -> Self {
        Self(bytes)
    }

    /// Байты адреса (копия: `[u8; 32]` — это `Copy`).
    pub const fn to_bytes(self) -> [u8; PUBKEY_BYTES] {
        self.0
    }

    /// Разобрать base58-строку **во время компиляции**.
    ///
    /// На этом построены `declare_id!` и `pubkey!`: опечатка в адресе программы —
    /// это ошибка компиляции, а не паника в рантайме. В `const fn` нельзя `for`
    /// и итераторы, поэтому здесь `while` и ручная «длинная арифметика»:
    /// `bytes = bytes * 58 + digit` для каждого символа.
    pub const fn from_str_const(s: &str) -> Self {
        const ALPHABET: &[u8; 58] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

        let input = s.as_bytes();
        assert!(input.len() <= MAX_BASE58_LEN, "address is too long");

        let mut bytes = [0u8; PUBKEY_BYTES];
        let mut i = 0;
        while i < input.len() {
            // Значение очередной «цифры» base58 (0..58).
            let mut digit = 0;
            while digit < ALPHABET.len() && ALPHABET[digit] != input[i] {
                digit += 1;
            }
            assert!(digit < ALPHABET.len(), "invalid base58 character");

            // bytes = bytes * 58 + digit, число хранится big-endian.
            let mut carry = digit as u32;
            let mut j = PUBKEY_BYTES;
            while j > 0 {
                j -= 1;
                carry += bytes[j] as u32 * 58;
                bytes[j] = carry as u8; // младшие 8 бит
                carry >>= 8;
            }
            assert!(carry == 0, "address does not fit into 32 bytes");
            i += 1;
        }
        Self(bytes)
    }

    /// Лежит ли адрес на кривой ed25519, т.е. может ли у него быть приватный ключ.
    /// У кошельков — да, у PDA — никогда.
    pub fn is_on_curve(&self) -> bool {
        CompressedEdwardsY(self.0).decompress().is_some()
    }

    /// Вычислить PDA по сидам **с уже известным bump** (bump — последний сид).
    ///
    /// `PDA = sha256(seed_1 ‖ … ‖ seed_n ‖ program_id ‖ "ProgramDerivedAddress")`,
    /// при условии что результат **не** лежит на кривой ed25519.
    ///
    /// Этой функцией программа дёшево проверяет адрес, если bump сохранён в
    /// аккаунте; так же рантайм проверяет «подпись» PDA в `invoke_signed`.
    pub fn create_program_address(
        seeds: &[&[u8]],
        program_id: &Pubkey,
    ) -> Result<Pubkey, PubkeyError> {
        if seeds.len() > MAX_SEEDS || seeds.iter().any(|seed| seed.len() > MAX_SEED_LEN) {
            return Err(PubkeyError::MaxSeedLengthExceeded);
        }

        let mut hasher = Sha256::new();
        for seed in seeds {
            hasher.update(seed);
        }
        hasher.update(program_id.0);
        hasher.update(PDA_MARKER);
        let hash: [u8; PUBKEY_BYTES] = hasher.finalize().into();

        let address = Pubkey(hash);
        if address.is_on_curve() {
            // Для точки на кривой мог бы существовать приватный ключ — такой адрес не PDA.
            return Err(PubkeyError::InvalidSeeds);
        }
        Ok(address)
    }

    /// Найти PDA и **канонический bump**: перебираем bump от 255 вниз и берём
    /// первый адрес вне кривой. Обычно это делает клиент или программа при
    /// создании аккаунта; дальше bump хранят и используют `create_program_address`.
    pub fn find_program_address(seeds: &[&[u8]], program_id: &Pubkey) -> (Pubkey, u8) {
        Self::try_find_program_address(seeds, program_id)
            .expect("Unable to find a viable program address bump seed")
    }

    /// То же, что [`Pubkey::find_program_address`], но без паники.
    pub fn try_find_program_address(seeds: &[&[u8]], program_id: &Pubkey) -> Option<(Pubkey, u8)> {
        for bump in (1..=u8::MAX).rev() {
            let bump_seed = [bump];
            // Копируем срез ссылок и дописываем bump последним сидом.
            let mut seeds_with_bump = seeds.to_vec();
            seeds_with_bump.push(&bump_seed);

            match Self::create_program_address(&seeds_with_bump, program_id) {
                Ok(address) => return Some((address, bump)),
                Err(PubkeyError::InvalidSeeds) => continue, // на кривой — пробуем следующий
                Err(PubkeyError::MaxSeedLengthExceeded) => return None,
            }
        }
        None
    }
}

// --- Трейты: как адрес превращается в байты, строку и обратно -------------------

impl AsRef<[u8]> for Pubkey {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<[u8; PUBKEY_BYTES]> for Pubkey {
    fn from(bytes: [u8; PUBKEY_BYTES]) -> Self {
        Self(bytes)
    }
}

/// Адреса в Solana показывают в base58 — и в `{}`, и в `{:?}`.
impl fmt::Display for Pubkey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&bs58::encode(self.0).into_string())
    }
}

impl fmt::Debug for Pubkey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// `"Vau1t…".parse::<Pubkey>()` — разбор base58 во время выполнения.
impl FromStr for Pubkey {
    type Err = ParsePubkeyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() > MAX_BASE58_LEN {
            return Err(ParsePubkeyError::WrongSize);
        }
        let bytes = bs58::decode(s)
            .into_vec()
            .map_err(|_| ParsePubkeyError::Invalid)?;
        // Vec<u8> → [u8; 32]: получится, только если длина ровно 32.
        let array: [u8; PUBKEY_BYTES] =
            bytes.try_into().map_err(|_| ParsePubkeyError::WrongSize)?;
        Ok(Self(array))
    }
}

/// Объявить адрес программы: `declare_id!("Vau1t…");`
///
/// Создаёт константу `ID` и функции `id()` / `check_id()` — так же, как настоящий
/// `solana_program::declare_id!`. Строка разбирается при компиляции.
#[macro_export]
macro_rules! declare_id {
    ($address:expr) => {
        /// Адрес (program id) этой программы.
        pub const ID: $crate::sdk::pubkey::Pubkey =
            $crate::sdk::pubkey::Pubkey::from_str_const($address);

        /// Адрес программы в виде функции.
        pub fn id() -> $crate::sdk::pubkey::Pubkey {
            ID
        }

        /// Совпадает ли `id` с адресом этой программы.
        pub fn check_id(id: &$crate::sdk::pubkey::Pubkey) -> bool {
            *id == ID
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn random_bytes() -> [u8; 32] {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).unwrap();
        bytes
    }

    #[test]
    fn system_program_id_is_all_zeros() {
        let id = Pubkey::from_str_const("11111111111111111111111111111111");
        assert_eq!(id.to_bytes(), [0u8; 32]);
    }

    #[test]
    fn const_parser_matches_bs58() {
        for _ in 0..200 {
            let key = Pubkey::new_from_array(random_bytes());
            let text = key.to_string();
            assert_eq!(Pubkey::from_str_const(&text), key);
            assert_eq!(text.parse::<Pubkey>(), Ok(key));
        }
    }

    #[test]
    fn parse_rejects_bad_strings() {
        assert_eq!("0OIl".parse::<Pubkey>(), Err(ParsePubkeyError::Invalid));
        assert_eq!("abc".parse::<Pubkey>(), Err(ParsePubkeyError::WrongSize));
    }

    #[test]
    fn find_program_address_returns_canonical_bump() {
        let program_id = Pubkey::new_from_array(random_bytes());
        let owner = Pubkey::new_from_array(random_bytes());
        let seeds: &[&[u8]] = &[b"vault", owner.as_ref()];

        let (pda, bump) = Pubkey::find_program_address(seeds, &program_id);

        // С найденным bump create_program_address даёт тот же адрес…
        let with_bump = Pubkey::create_program_address(&[seeds[0], seeds[1], &[bump]], &program_id);
        assert_eq!(with_bump, Ok(pda));
        // …а все bump больше канонического дают точки на кривой.
        for higher in (bump..=u8::MAX).skip(1) {
            let result =
                Pubkey::create_program_address(&[seeds[0], seeds[1], &[higher]], &program_id);
            assert_eq!(result, Err(PubkeyError::InvalidSeeds));
        }
        assert!(!pda.is_on_curve());
    }

    #[test]
    fn seed_longer_than_32_bytes_is_rejected() {
        let program_id = Pubkey::default();
        let long_seed = [7u8; MAX_SEED_LEN + 1];
        assert_eq!(
            Pubkey::create_program_address(&[&long_seed], &program_id),
            Err(PubkeyError::MaxSeedLengthExceeded)
        );
    }
}
