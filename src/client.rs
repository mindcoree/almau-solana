//! Клиентская (off-chain) часть: ключи, транзакции, перевод SOL ↔ лампорты.
//! Аналог `solana-keypair`, `solana-transaction`, `solana-native-token`.
//!
//! Этот код работает на компьютере пользователя, а не в блокчейне, поэтому здесь
//! можно всё, что нельзя в программе: файлы, `anyhow`, `serde_json`.

use std::fmt;
use std::fs;
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result, bail};
use borsh::BorshSerialize;
use ed25519_dalek::{Signer, SigningKey};

use crate::sdk::instruction::Instruction;
use crate::sdk::pubkey::Pubkey;

/// 1 SOL = 10⁹ лампортов. Все суммы внутри — целые лампорты, никаких `f64`.
pub const LAMPORTS_PER_SOL: u64 = 1_000_000_000;

// === Ключи ===========================================================================

/// Пара ключей ed25519. Публичная половина — это адрес кошелька.
pub struct Keypair(SigningKey);

impl Keypair {
    /// Новый случайный ключ (секрет берётся из генератора случайных чисел ОС).
    pub fn new() -> Self {
        let mut secret = [0u8; 32];
        getrandom::fill(&mut secret).expect("OS random number generator is unavailable");
        Self(SigningKey::from_bytes(&secret))
    }

    pub fn pubkey(&self) -> Pubkey {
        Pubkey::new_from_array(self.0.verifying_key().to_bytes())
    }

    /// Подписать байты сообщения. Подпись ed25519 — 64 байта.
    pub fn sign_message(&self, message: &[u8]) -> [u8; 64] {
        self.0.sign(message).to_bytes()
    }

    /// 64 байта: секрет (32) + публичный ключ (32) — формат `solana-keygen`.
    pub fn to_bytes(&self) -> [u8; 64] {
        self.0.to_keypair_bytes()
    }

    pub fn try_from_bytes(bytes: &[u8]) -> Result<Self> {
        let bytes: &[u8; 64] = bytes
            .try_into()
            .context("ключ должен состоять ровно из 64 байт")?;
        let key = SigningKey::from_keypair_bytes(bytes)
            .context("публичная половина ключа не соответствует секретной")?;
        Ok(Self(key))
    }

    /// Сохранить как `solana-keygen`: JSON-массив из 64 чисел.
    /// Существующий файл не перезаписывается — потерять ключ хуже, чем получить ошибку.
    pub fn write_to_file(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string(&self.to_bytes().to_vec())?;
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        // На Unix права 0600 (только владелец) ставятся сразу при создании файла,
        // как у solana-keygen, — секрет ни мгновения не лежит открытым для других.
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(path)
            .with_context(|| format!("не удалось создать {}", path.display()))?;
        file.write_all(json.as_bytes())?;
        Ok(())
    }

    pub fn read_from_file(path: &Path) -> Result<Self> {
        let json = fs::read_to_string(path)
            .with_context(|| format!("не удалось прочитать {}", path.display()))?;
        let bytes: Vec<u8> = serde_json::from_str(&json)
            .with_context(|| format!("{} — не файл ключа solana-keygen", path.display()))?;
        Self::try_from_bytes(&bytes)
    }
}

impl Default for Keypair {
    fn default() -> Self {
        Self::new()
    }
}

/// Секрет никогда не печатаем — даже в `{:?}`.
impl fmt::Debug for Keypair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Keypair({})", self.pubkey())
    }
}

// === Транзакции ======================================================================

/// Что подписывают: кто платит комиссию и какие инструкции выполнить.
///
/// В настоящей Solana сообщение ещё содержит `recent_blockhash` (защита от
/// повторной отправки) и компактный список аккаунтов — здесь это опущено.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize)]
pub struct Message {
    pub fee_payer: Pubkey,
    pub instructions: Vec<Instruction>,
}

impl Message {
    /// Байты, которые подписывает каждый подписант.
    pub fn to_bytes(&self) -> Vec<u8> {
        borsh::to_vec(self).expect("Borsh serialization into Vec never fails")
    }
}

/// Транзакция = сообщение + подписи. Все инструкции выполняются атомарно:
/// либо все, либо ни одной.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transaction {
    pub message: Message,
    /// Пары (адрес, подпись ed25519 под `message`).
    pub signatures: Vec<(Pubkey, [u8; 64])>,
}

impl Transaction {
    /// Собрать и подписать транзакцию — та же сигнатура, что в Solana,
    /// только без `recent_blockhash`.
    pub fn new_signed_with_payer(
        instructions: &[Instruction],
        payer: Option<&Pubkey>,
        signers: &[&Keypair],
    ) -> Self {
        let fee_payer = payer
            .copied()
            .or_else(|| signers.first().map(|keypair| keypair.pubkey()))
            .expect("транзакции нужен плательщик комиссии");
        let message = Message {
            fee_payer,
            instructions: instructions.to_vec(),
        };
        let bytes = message.to_bytes();
        let signatures = signers
            .iter()
            .map(|keypair| (keypair.pubkey(), keypair.sign_message(&bytes)))
            .collect();
        Self {
            message,
            signatures,
        }
    }

    /// Идентификатор транзакции — первая подпись в base58 (как в эксплорере).
    pub fn signature(&self) -> String {
        self.signatures
            .first()
            .map(|(_, signature)| bs58::encode(signature).into_string())
            .unwrap_or_default()
    }
}

// === SOL ↔ лампорты =================================================================

/// `"1.5"` → `1_500_000_000`. Строку разбираем как десятичную дробь, без `f64`:
/// `0.1 + 0.2 != 0.3` — не то, что хочется видеть в деньгах.
pub fn parse_sol(input: &str) -> Result<u64> {
    let input = input.trim();
    let (whole, fraction) = input.split_once('.').unwrap_or((input, ""));

    let is_digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    if (whole.is_empty() && fraction.is_empty()) || !is_digits(whole) || !is_digits(fraction) {
        bail!("«{input}» — не сумма в SOL (пример: 1.5)");
    }
    if fraction.len() > 9 {
        bail!("не больше 9 знаков после точки: 1 лампорт = 0.000000001 SOL");
    }

    let whole: u64 = if whole.is_empty() { 0 } else { whole.parse()? };
    // "5" → "500000000": дополняем дробную часть нулями справа до 9 знаков.
    let fraction: u64 = format!("{fraction:0<9}").parse()?;

    whole
        .checked_mul(LAMPORTS_PER_SOL)
        .and_then(|lamports| lamports.checked_add(fraction))
        .context("слишком большая сумма")
}

/// `1_500_000_000` → `"1.5"`.
pub fn format_sol(lamports: u64) -> String {
    let whole = lamports / LAMPORTS_PER_SOL;
    let fraction = lamports % LAMPORTS_PER_SOL;
    if fraction == 0 {
        whole.to_string()
    } else {
        let text = format!("{whole}.{fraction:09}");
        text.trim_end_matches('0').to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sol_parsing() {
        assert_eq!(parse_sol("1").unwrap(), 1_000_000_000);
        assert_eq!(parse_sol("1.5").unwrap(), 1_500_000_000);
        assert_eq!(parse_sol(".25").unwrap(), 250_000_000);
        assert_eq!(parse_sol("0.000000001").unwrap(), 1);
        assert!(parse_sol("0.0000000001").is_err()); // меньше лампорта
        assert!(parse_sol("-1").is_err());
        assert!(parse_sol("1e9").is_err());
        assert!(parse_sol(".").is_err());
        assert!(parse_sol("99999999999").is_err()); // переполнение u64
    }

    #[test]
    fn sol_formatting() {
        assert_eq!(format_sol(1_500_000_000), "1.5");
        assert_eq!(format_sol(2_001_287_600), "2.0012876");
        assert_eq!(format_sol(0), "0");
        assert_eq!(format_sol(1), "0.000000001");
    }

    #[test]
    fn keypair_bytes_roundtrip_and_signature() {
        let keypair = Keypair::new();
        let restored = Keypair::try_from_bytes(&keypair.to_bytes()).unwrap();
        assert_eq!(restored.pubkey(), keypair.pubkey());
        // Публичный ключ кошелька всегда лежит на кривой — в отличие от PDA.
        assert!(keypair.pubkey().is_on_curve());
        // Подпись детерминирована: те же байты → та же подпись.
        assert_eq!(keypair.sign_message(b"hi"), restored.sign_message(b"hi"));
    }
}
