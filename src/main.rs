//! `piggy` — CLI копилки. Каждая команда работает как вызов `solana …`:
//! прочитать «блокчейн» (`~/.config/piggy/ledger.json`), собрать и подписать
//! транзакцию, отправить её в рантайм, показать логи и сохранить результат.
//!
//! Установка в систему: `cargo install --path .` — дальше просто `piggy <команда>`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};

use piggy::client::{Keypair, Transaction, format_sol, parse_sol};
use piggy::runtime::{InstructionError, Ledger, TransactionError};
use piggy::sdk::instruction::Instruction;
use piggy::sdk::pubkey::Pubkey;
use piggy::sdk::rent::Rent;
use piggy::system_program::{self, SystemError};
use piggy::vault::error::VaultError;
use piggy::vault::state::{AccountState, Vault};
use piggy::vault::{self, VAULT_SEED};

#[derive(Parser)]
#[command(
    name = "piggy",
    version,
    about = "piggy — копилка SOL на Rust по модели Solana: кошельки, PDA, CPI, Borsh",
    // `piggy` без аргументов покажет справку, а не ошибку.
    arg_required_else_help = true
)]
struct Cli {
    /// Папка с «блокчейном» (ledger.json) и кошельками
    #[arg(long, global = true, env = "PIGGY_HOME", default_value_os_t = default_home())]
    home: PathBuf,

    #[command(subcommand)]
    command: Command,
}

/// Данные по умолчанию — в домашней папке, как у Solana CLI (`~/.config/solana`):
/// так `piggy` видит одни и те же кошельки, из какого бы каталога её ни запустили.
fn default_home() -> PathBuf {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE")) // Windows
        .map_or_else(|| PathBuf::from("."), PathBuf::from);
    home.join(".config").join("piggy")
}

/// Подкоманды: enum с данными, из которого clap сам строит разбор аргументов и `--help`.
#[derive(Subcommand)]
enum Command {
    /// Создать кошелёк (файл ключа в формате solana-keygen)
    Keygen {
        /// Имя кошелька
        name: String,
    },
    /// Кошельки, балансы и копилки
    Wallets,
    /// Начислить SOL «из воздуха» (как `solana airdrop` в devnet)
    Airdrop {
        /// Имя кошелька
        name: String,
        /// Сумма в SOL, например 1.5
        #[arg(value_parser = parse_sol)]
        sol: u64,
    },
    /// Показать, как вычисляется адрес копилки (PDA), — без транзакции
    Pda {
        /// Имя кошелька или адрес владельца
        owner: String,
    },
    /// Создать копилку
    Init {
        /// Владелец копилки (подписывает и платит rent-депозит)
        name: String,
        /// Сразу положить SOL — второй инструкцией в той же транзакции
        #[arg(long, value_parser = parse_sol)]
        deposit: Option<u64>,
    },
    /// Положить SOL в копилку (свою или чужую)
    Deposit {
        /// Кто вносит (подписывает транзакцию)
        name: String,
        /// Сумма в SOL, например 1.5
        #[arg(value_parser = parse_sol)]
        sol: u64,
        /// Чья копилка; по умолчанию — своя
        #[arg(long)]
        to: Option<String>,
    },
    /// Снять SOL из копилки (разрешено только владельцу)
    Withdraw {
        /// Кто снимает (подписывает транзакцию)
        name: String,
        /// Сумма в SOL, например 0.5
        #[arg(value_parser = parse_sol)]
        sol: u64,
        /// Из чьей копилки; попробуйте указать чужую
        #[arg(long)]
        from: Option<String>,
    },
    /// Закрыть копилку и вернуть владельцу всё, включая rent-резерв
    Close {
        /// Владелец копилки
        name: String,
    },
    /// Копилка: баланс, rent-резерв и разбор данных по байтам
    Show {
        /// Имя кошелька или адрес владельца
        owner: String,
    },
    /// Сырой аккаунт по адресу или имени кошелька (как `solana account`)
    Account {
        /// Адрес (base58) или имя кошелька
        address: String,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // `{:#}` у anyhow печатает всю цепочку причин: «не удалось …: причина».
            eprintln!("Ошибка: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    let mut app = App::open(&cli.home)?;

    match cli.command {
        Command::Keygen { name } => app.keygen(&name)?,
        Command::Wallets => app.list_wallets()?,
        Command::Airdrop { name, sol } => {
            let owner = app.wallet(&name)?.pubkey();
            app.ledger.airdrop(&owner, sol)?;
            app.save()?;
            println!(
                "Начислено {} SOL → «{name}». Баланс: {} SOL",
                format_sol(sol),
                format_sol(app.ledger.get_balance(&owner))
            );
        }
        Command::Pda { owner } => app.explain_pda(&owner)?,
        Command::Init { name, deposit } => {
            let owner = app.wallet(&name)?;
            let mut instructions = vec![vault::instruction::initialize(&owner.pubkey())];
            if let Some(lamports) = deposit {
                instructions.push(vault::instruction::deposit(
                    &owner.pubkey(),
                    &owner.pubkey(),
                    lamports,
                ));
            }
            app.send(&instructions, &[&owner])?;
        }
        Command::Deposit { name, sol, to } => {
            let depositor = app.wallet(&name)?;
            let vault_owner = match to {
                Some(to) => app.resolve(&to)?,
                None => depositor.pubkey(),
            };
            let instruction = vault::instruction::deposit(&depositor.pubkey(), &vault_owner, sol);
            app.send(&[instruction], &[&depositor])?;
        }
        Command::Withdraw { name, sol, from } => {
            let signer = app.wallet(&name)?;
            let vault_owner = match from {
                Some(from) => app.resolve(&from)?,
                None => signer.pubkey(),
            };
            let instruction = vault::instruction::withdraw(&signer.pubkey(), &vault_owner, sol);
            app.send(&[instruction], &[&signer])?;
        }
        Command::Close { name } => {
            let owner = app.wallet(&name)?;
            let instruction = vault::instruction::close(&owner.pubkey(), &owner.pubkey());
            app.send(&[instruction], &[&owner])?;
        }
        Command::Show { owner } => app.show_vault(&owner)?,
        Command::Account { address } => app.show_account(&address)?,
    }
    Ok(())
}

/// Состояние CLI: где лежат файлы и загруженный леджер.
struct App {
    home: PathBuf,
    ledger: Ledger,
}

impl App {
    fn open(home: &Path) -> Result<Self> {
        let ledger = Ledger::load_or_new(&home.join("ledger.json"))?;
        Ok(Self {
            home: home.to_path_buf(),
            ledger,
        })
    }

    fn save(&self) -> Result<()> {
        self.ledger.save(&self.home.join("ledger.json"))
    }

    fn wallets_dir(&self) -> PathBuf {
        self.home.join("wallets")
    }

    fn wallet_path(&self, name: &str) -> Result<PathBuf> {
        let valid = !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        if !valid {
            bail!("имя кошелька — латиница, цифры, «-» и «_», например alice");
        }
        Ok(self.wallets_dir().join(format!("{name}.json")))
    }

    fn wallet(&self, name: &str) -> Result<Keypair> {
        let path = self.wallet_path(name)?;
        if !path.exists() {
            bail!("кошелька «{name}» нет — создайте его: piggy keygen {name}");
        }
        Keypair::read_from_file(&path)
    }

    /// Имя кошелька или base58-адрес → адрес.
    fn resolve(&self, name_or_address: &str) -> Result<Pubkey> {
        if let Ok(path) = self.wallet_path(name_or_address) {
            if path.exists() {
                return Ok(Keypair::read_from_file(&path)?.pubkey());
            }
        }
        name_or_address
            .parse()
            .with_context(|| format!("«{name_or_address}» — не имя кошелька и не адрес"))
    }

    /// Все кошельки, отсортированные по имени.
    fn wallets(&self) -> Result<Vec<(String, Pubkey)>> {
        let dir = self.wallets_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut wallets = Vec::new();
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.extension().is_some_and(|ext| ext == "json") {
                let name = path
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
                    .unwrap_or_default();
                wallets.push((name, Keypair::read_from_file(&path)?.pubkey()));
            }
        }
        wallets.sort();
        Ok(wallets)
    }

    /// Человекочитаемое имя адреса, если оно известно.
    fn label(&self, address: &Pubkey) -> Option<String> {
        if system_program::check_id(address) {
            return Some("System Program".into());
        }
        if vault::check_id(address) {
            return Some("программа Копилка".into());
        }
        self.wallet_name(address)
            .map(|name| format!("кошелёк «{name}»"))
    }

    /// Имя кошелька по адресу, если такой кошелёк есть.
    fn wallet_name(&self, address: &Pubkey) -> Option<String> {
        self.wallets()
            .ok()?
            .into_iter()
            .find(|(_, key)| key == address)
            .map(|(name, _)| name)
    }

    fn keygen(&self, name: &str) -> Result<()> {
        let path = self.wallet_path(name)?;
        if path.exists() {
            bail!("кошелёк «{name}» уже есть: {}", path.display());
        }
        fs::create_dir_all(self.wallets_dir())?;
        let keypair = Keypair::new();
        keypair.write_to_file(&path)?;
        println!("Кошелёк «{name}» создан");
        println!("  адрес: {}", keypair.pubkey());
        println!("  ключ:  {}", path.display());
        Ok(())
    }

    fn list_wallets(&self) -> Result<()> {
        let wallets = self.wallets()?;
        if wallets.is_empty() {
            println!("Кошельков пока нет: piggy keygen alice");
            return Ok(());
        }
        println!(
            "{:<10} {:<44} {:>14}  копилка",
            "имя", "адрес", "баланс, SOL"
        );
        for (name, address) in wallets {
            let (vault_address, _) = vault::find_vault_address(&address, &vault::ID);
            let vault = match self.ledger.get_account(&vault_address) {
                Some(account) => format!("{} SOL", format_sol(account.lamports)),
                None => "—".into(),
            };
            let balance = format_sol(self.ledger.get_balance(&address));
            println!("{name:<10} {address:<44} {balance:>14}  {vault}");
        }
        Ok(())
    }

    /// Пошагово показать поиск PDA: какие bump отбрасываются и почему.
    fn explain_pda(&self, owner: &str) -> Result<()> {
        let owner = self.resolve(owner)?;
        println!("PDA = sha256(сиды ‖ bump ‖ program_id ‖ \"ProgramDerivedAddress\")");
        println!("  сиды:       [\"vault\", {owner}]");
        println!("  program_id: {}", vault::ID);
        println!("Перебираем bump от 255 вниз, пока адрес не окажется вне кривой ed25519:");
        for bump in (1..=u8::MAX).rev() {
            let seeds: &[&[u8]] = &[VAULT_SEED, owner.as_ref(), &[bump]];
            match Pubkey::create_program_address(seeds, &vault::ID) {
                Ok(address) => {
                    println!("  bump {bump:>3}: {address} — вне кривой ✓");
                    let status = if self.ledger.get_account(&address).is_some() {
                        "аккаунт существует"
                    } else {
                        "аккаунта пока нет — создаст `init`"
                    };
                    println!("Адрес копилки: {address} (канонический bump {bump}; {status})");
                    return Ok(());
                }
                Err(_) => println!(
                    "  bump {bump:>3}: точка на кривой — у неё мог бы быть приватный ключ, пропускаем"
                ),
            }
        }
        bail!("подходящий bump не найден (вероятность — примерно 2^-255)")
    }

    fn show_vault(&self, owner: &str) -> Result<()> {
        let owner = self.resolve(owner)?;
        let who = self
            .wallet_name(&owner)
            .map_or_else(|| owner.to_string(), |name| format!("«{name}»"));
        let (address, bump) = vault::find_vault_address(&owner, &vault::ID);
        let Some(account) = self.ledger.get_account(&address) else {
            bail!("у {who} нет копилки (её адрес был бы {address})");
        };
        let state = Vault::try_from_bytes(&account.data)?;
        let reserve = Rent::default().minimum_balance(account.data.len());

        println!("Копилка {who}");
        println!("  адрес (PDA)  {address}");
        println!("  сиды         [\"vault\", {owner}], bump {bump}");
        println!("  баланс       {} SOL", format_sol(account.lamports));
        println!(
            "    резерв     {} SOL — rent-минимум для {} байт данных, вернётся при закрытии",
            format_sol(reserve),
            account.data.len()
        );
        println!(
            "    доступно   {} SOL",
            format_sol(account.lamports.saturating_sub(reserve))
        );
        println!(
            "  внесено      {} SOL за всё время",
            format_sol(state.total_deposited)
        );
        println!(
            "  снято        {} SOL за всё время",
            format_sol(state.total_withdrawn)
        );

        // Разбор данных аккаунта по полям: так Borsh раскладывает структуру Vault.
        let data = &account.data;
        println!();
        println!("Данные аккаунта — {} байт (Borsh):", data.len());
        let rows: [(&str, std::ops::Range<usize>, String); 5] = [
            (
                "дискриминатор",
                0..8,
                "sha256(\"account:Vault\")[..8]".into(),
            ),
            ("owner", 8..40, state.owner.to_string()),
            ("bump", 40..41, state.bump.to_string()),
            (
                "total_deposited",
                41..49,
                format!("{} (u64 little-endian)", state.total_deposited),
            ),
            ("total_withdrawn", 49..57, state.total_withdrawn.to_string()),
        ];
        for (field, range, value) in rows {
            let bytes = &data[range.clone()];
            let shown = if bytes.len() > 8 {
                format!("{} … ({} байта)", hex_bytes(&bytes[..6]), bytes.len())
            } else {
                hex_bytes(bytes)
            };
            let range = format!("[{}..{}]", range.start, range.end);
            println!("  {range:<9} {shown:<32} {field} = {value}");
        }
        Ok(())
    }

    /// Как `solana account <адрес>`.
    fn show_account(&self, address: &str) -> Result<()> {
        let address = self.resolve(address)?;
        let Some(account) = self.ledger.get_account(&address) else {
            println!(
                "Аккаунта {address} нет: у него 0 лампортов (так выглядит любой неиспользованный адрес)"
            );
            return Ok(());
        };
        let label = |key: &Pubkey| {
            self.label(key)
                .map(|name| format!(" ({name})"))
                .unwrap_or_default()
        };
        println!("Public Key: {address}{}", label(&address));
        println!("Balance: {} SOL", format_sol(account.lamports));
        println!("Owner: {}{}", account.owner, label(&account.owner));
        println!("Executable: {}", account.executable);
        println!(
            "Length: {} (0x{:x}) bytes",
            account.data.len(),
            account.data.len()
        );
        for (line, chunk) in account.data.chunks(16).enumerate() {
            let ascii: String = chunk
                .iter()
                .map(|&b| if b.is_ascii_graphic() { b as char } else { '.' })
                .collect();
            println!("{:04x}:   {:<48}  {ascii}", line * 16, hex_bytes(chunk));
        }
        Ok(())
    }

    /// Подписать, отправить, показать логи. Леджер сохраняется и при ошибке:
    /// комиссию за упавшую транзакцию в Solana тоже списывают.
    fn send(&mut self, instructions: &[Instruction], signers: &[&Keypair]) -> Result<()> {
        let payer = signers
            .first()
            .context("нужен хотя бы один подписант")?
            .pubkey();
        let tx = Transaction::new_signed_with_payer(instructions, Some(&payer), signers);
        let result = self.ledger.send_transaction(&tx);
        self.save()?;

        let (meta, error) = match &result {
            Ok(meta) => (meta, None),
            Err(failed) => (&failed.meta, Some(&failed.err)),
        };
        let signature = &meta.signature;
        println!(
            "Транзакция {}…{}",
            &signature[..8],
            &signature[signature.len() - 8..]
        );
        for line in &meta.logs {
            println!("  {line}");
        }
        if meta.fee > 0 {
            println!("  комиссия: {} лампортов", meta.fee);
        }

        match error {
            None => {
                println!("✓ успешно");
                Ok(())
            }
            Some(error) => {
                if let Some(hint) = explain(error) {
                    println!("  → {hint}");
                }
                bail!("транзакция отклонена: {error}")
            }
        }
    }
}

/// Байты в hex через пробел: `d3 08 e8 2b`.
fn hex_bytes(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Расшифровать частые ошибки. Коды программ восстанавливаем через `TryFrom<u32>`:
/// в блокчейне от ошибки остаётся только число.
fn explain(error: &TransactionError) -> Option<String> {
    use InstructionError as Ix;
    match error {
        TransactionError::InstructionError(_, Ix::Custom(code)) => {
            if let Ok(vault_error) = VaultError::try_from(*code) {
                Some(format!(
                    "VaultError::{vault_error:?} ({code}): {vault_error}"
                ))
            } else if let Ok(system_error) = SystemError::try_from(*code) {
                Some(format!(
                    "SystemError::{system_error:?} ({code}): {system_error}"
                ))
            } else {
                None
            }
        }
        TransactionError::InstructionError(_, Ix::ExternalAccountLamportSpend) => Some(
            "программа списала лампорты с аккаунта, которым не владеет; \
             с кошелька может списать только System Program — через CPI"
                .into(),
        ),
        TransactionError::InstructionError(_, Ix::PrivilegeEscalation) => Some(
            "в CPI нельзя добавить подпись или право записи, которых нет у вызывающей программы"
                .into(),
        ),
        TransactionError::AccountNotFound => {
            Some("у плательщика комиссии нет SOL — сначала airdrop".into())
        }
        _ => None,
    }
}
