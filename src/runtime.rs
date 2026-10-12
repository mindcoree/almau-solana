//! Мини-валидатор: хранит аккаунты («леджер») и исполняет транзакции.
//!
//! Программа не может нарушить правила Solana, потому что их проверяет рантайм:
//!
//! 1. `is_signer = true` только у адресов с валидной подписью ed25519, а в CPI —
//!    ещё у PDA вызывающей программы. Права в CPI рантайм берёт из своих записей;
//! 2. списывать лампорты и менять данные может только программа-владелец аккаунта,
//!    а отдать аккаунт другой программе — только с пустыми или обнулёнными данными;
//! 3. менять можно только аккаунты, помеченные writable;
//! 4. сумма лампортов до и после каждого вызова одинакова — SOL не берутся из воздуха;
//! 5. изменённый аккаунт после транзакции либо пуст (0 лампортов — удаляется),
//!    либо держит rent-минимум; это касается и списания комиссии;
//! 6. транзакция атомарна: при ошибке изменения откатываются — кроме комиссии.
//!
//! API похож на LiteSVM (`airdrop`, `send_transaction`, `get_account`), чтобы потом
//! было легко перейти на настоящие тесты программ.

use std::any::Any;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::panic::{self, AssertUnwindSafe};
use std::path::Path;
use std::rc::Rc;

use anyhow::Context;
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

use crate::client::{Message, Transaction};
use crate::sdk::account_info::{AccountInfo, MAX_PERMITTED_DATA_INCREASE};
use crate::sdk::entrypoint::ProcessInstruction;
use crate::sdk::instruction::Instruction;
use crate::sdk::program_error::{ProgramError, ProgramResult};
use crate::sdk::pubkey::Pubkey;
use crate::sdk::rent::Rent;
use crate::{system_program, vault};

/// Комиссия за каждую подпись в транзакции (как в Solana).
pub const LAMPORTS_PER_SIGNATURE: u64 = 5_000;
/// Глубина вызовов: инструкция транзакции — уровень 1, каждый CPI — +1.
pub const MAX_INVOKE_DEPTH: usize = 5;

const NATIVE_LOADER_ID: Pubkey =
    Pubkey::from_str_const("NativeLoader1111111111111111111111111111111");
const BPF_LOADER_UPGRADEABLE_ID: Pubkey =
    Pubkey::from_str_const("BPFLoaderUpgradeab1e11111111111111111111111");

/// Программы, которые есть в сети с самого начала: адрес и точка входа.
fn builtin_programs() -> Vec<(Pubkey, ProcessInstruction)> {
    vec![
        (system_program::ID, system_program::process_instruction),
        (vault::ID, vault::ENTRYPOINT),
    ]
}

// === Аккаунты и леджер ================================================================

/// Аккаунт в хранилище. `Default` — «несуществующий» аккаунт: 0 лампортов, нет данных,
/// владелец `Pubkey::default()`, т.е. 32 нулевых байта — это и есть адрес System Program.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub lamports: u64,
    #[serde(with = "hex")]
    pub data: Vec<u8>,
    pub owner: Pubkey,
    pub executable: bool,
}

/// Все аккаунты «сети». Сохраняется в JSON между запусками CLI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ledger {
    accounts: BTreeMap<Pubkey, Account>,
    /// Код программ. Указатели на функции в JSON не сохраняются — после загрузки
    /// подставляются встроенные программы.
    #[serde(skip, default = "builtin_programs")]
    programs: Vec<(Pubkey, ProcessInstruction)>,
}

/// Результат успешной транзакции.
#[derive(Debug, Clone)]
pub struct TransactionMetadata {
    pub signature: String,
    pub logs: Vec<String>,
    pub fee: u64,
}

/// Результат неуспешной транзакции: ошибка + то, что успело попасть в логи.
#[derive(Debug, Clone)]
pub struct FailedTransactionMetadata {
    pub err: TransactionError,
    pub meta: TransactionMetadata,
}

pub type TransactionResult = Result<TransactionMetadata, FailedTransactionMetadata>;

/// Ошибки уровня транзакции. Тексты взяты у настоящего валидатора, с двумя
/// отличиями: `MissingSignature` в Solana ловит ещё клиент при подписании, а в
/// `InsufficientFundsForRent` настоящий валидатор пишет номер аккаунта, а не адрес.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TransactionError {
    #[error("Attempt to debit an account but found no record of a prior credit.")]
    AccountNotFound,
    #[error("This account may not be used to pay transaction fees")]
    InvalidAccountForFee,
    #[error("Insufficient funds for fee")]
    InsufficientFundsForFee,
    #[error("Transaction did not pass signature verification")]
    SignatureFailure,
    #[error("Transaction is missing a signature from {0}")]
    MissingSignature(Pubkey),
    #[error("Error processing Instruction {0}: {1}")]
    InstructionError(u8, InstructionError),
    #[error("Transaction results in an account ({0}) with insufficient funds for rent")]
    InsufficientFundsForRent(Pubkey),
}

/// Ошибки исполнения инструкции: ошибки программы + нарушения правил рантайма.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum InstructionError {
    #[error("custom program error: {0:#x}")]
    Custom(u32),
    /// Встроенная ошибка программы. Печатаем текст `ProgramError`; у настоящего
    /// валидатора на этом уровне свои формулировки (например,
    /// `missing required signature for instruction`).
    #[error("{0}")]
    Program(ProgramError),
    #[error("Cross-program invocation with unauthorized signer or writable account")]
    PrivilegeEscalation,
    #[error("instruction spent from the balance of an account it does not own")]
    ExternalAccountLamportSpend,
    #[error("instruction modified data of an account it does not own")]
    ExternalAccountDataModified,
    #[error("instruction illegally modified the program id of an account")]
    ModifiedProgramId,
    #[error("instruction changed the balance of a read-only account")]
    ReadonlyLamportChange,
    #[error("instruction modified data of a read-only account")]
    ReadonlyDataModified,
    #[error("sum of account balances before and after instruction do not match")]
    UnbalancedInstruction,
    #[error("Failed to reallocate account data")]
    InvalidRealloc,
    #[error("instruction left account with an outstanding borrowed reference")]
    AccountBorrowOutstanding,
    #[error("Cross-program invocation call depth too deep")]
    CallDepth,
    #[error("Cross-program invocation reentrancy not allowed for this instruction")]
    ReentrancyNotAllowed,
    #[error("An account required by the instruction is missing")]
    MissingAccount,
    #[error("Unsupported program id")]
    UnsupportedProgramId,
    #[error("Program failed to complete")]
    ProgramFailedToComplete,
}

impl From<ProgramError> for InstructionError {
    fn from(error: ProgramError) -> Self {
        match error {
            ProgramError::Custom(code) => Self::Custom(code),
            other => Self::Program(other),
        }
    }
}

impl Default for Ledger {
    fn default() -> Self {
        Self::new()
    }
}

impl Ledger {
    /// «Генезис»: в сети есть только встроенные программы.
    pub fn new() -> Self {
        let mut ledger = Self {
            accounts: BTreeMap::new(),
            programs: Vec::new(),
        };
        for (program_id, entrypoint) in builtin_programs() {
            ledger.add_program(program_id, entrypoint);
        }
        ledger
    }

    /// «Задеплоить» программу — как `LiteSVM::add_program`, только вместо байткода SBF
    /// указатель на Rust-функцию. Так в тестах можно подключить свою программу,
    /// например «взломщика», и проверить, что рантайм его остановит.
    pub fn add_program(&mut self, program_id: Pubkey, entrypoint: ProcessInstruction) {
        let loader = if system_program::check_id(&program_id) {
            NATIVE_LOADER_ID
        } else {
            BPF_LOADER_UPGRADEABLE_ID
        };
        let account = Account {
            lamports: 1,
            data: Vec::new(),
            owner: loader,
            executable: true,
        };
        self.accounts.insert(program_id, account);
        self.programs.retain(|(id, _)| *id != program_id);
        self.programs.push((program_id, entrypoint));
    }

    /// Прочитать леджер из файла или начать новый.
    pub fn load_or_new(path: &Path) -> anyhow::Result<Self> {
        if !path.exists() {
            return Ok(Self::new());
        }
        let json = fs::read_to_string(path)
            .with_context(|| format!("не удалось прочитать {}", path.display()))?;
        serde_json::from_str(&json).with_context(|| format!("{} повреждён", path.display()))
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, serde_json::to_string_pretty(self)?)
            .with_context(|| format!("не удалось записать {}", path.display()))
    }

    pub fn get_account(&self, address: &Pubkey) -> Option<&Account> {
        self.accounts.get(address)
    }

    pub fn get_balance(&self, address: &Pubkey) -> u64 {
        self.get_account(address)
            .map_or(0, |account| account.lamports)
    }

    pub fn minimum_balance_for_rent_exemption(&self, data_len: usize) -> u64 {
        Rent::default().minimum_balance(data_len)
    }

    /// Начислить лампорты «из воздуха» — как кран (faucet) в devnet.
    pub fn airdrop(&mut self, to: &Pubkey, lamports: u64) -> Result<(), TransactionError> {
        let mut account = self.accounts.get(to).cloned().unwrap_or_default();
        account.lamports = account.lamports.saturating_add(lamports);
        if !Rent::default().is_exempt(account.lamports, account.data.len()) {
            return Err(TransactionError::InsufficientFundsForRent(*to));
        }
        self.accounts.insert(*to, account);
        Ok(())
    }

    /// Исполнить транзакцию.
    pub fn send_transaction(&mut self, tx: &Transaction) -> TransactionResult {
        let programs = self.programs.clone();
        CONTEXT.with_borrow_mut(|context| {
            *context = InvokeContext {
                programs,
                ..InvokeContext::default()
            }
        });

        let mut fee = 0;
        let result = self.process_transaction(tx, &mut fee);

        let logs = CONTEXT.with_borrow_mut(|context| std::mem::take(&mut context.logs));
        let meta = TransactionMetadata {
            signature: tx.signature(),
            logs,
            fee,
        };
        match result {
            Ok(()) => Ok(meta),
            Err(err) => Err(FailedTransactionMetadata { err, meta }),
        }
    }

    fn process_transaction(
        &mut self,
        tx: &Transaction,
        fee: &mut u64,
    ) -> Result<(), TransactionError> {
        let message = &tx.message;

        // 1. Подписи. Только после этой проверки рантайм пометит адрес как signer.
        let signers = verify_signatures(tx)?;
        let required = std::iter::once(&message.fee_payer).chain(
            message
                .instructions
                .iter()
                .flat_map(|instruction| &instruction.accounts)
                .filter(|meta| meta.is_signer)
                .map(|meta| &meta.pubkey),
        );
        for signer in required {
            if !signers.contains(signer) {
                return Err(TransactionError::MissingSignature(*signer));
            }
        }

        // 2. Комиссия: списывается сразу и не возвращается, даже если инструкция упадёт.
        //    Платить может только обычный кошелёк, и комиссия тоже не может оставить
        //    его ниже rent-минимума (ровно 0 — можно: такой аккаунт просто исчезнет).
        let tx_fee = LAMPORTS_PER_SIGNATURE * tx.signatures.len() as u64;
        let payer = self
            .accounts
            .get_mut(&message.fee_payer)
            .ok_or(TransactionError::AccountNotFound)?;
        if !system_program::check_id(&payer.owner) || !payer.data.is_empty() {
            return Err(TransactionError::InvalidAccountForFee);
        }
        let remaining = payer
            .lamports
            .checked_sub(tx_fee)
            .ok_or(TransactionError::InsufficientFundsForFee)?;
        if remaining > 0 && !Rent::default().is_exempt(remaining, 0) {
            return Err(TransactionError::InsufficientFundsForRent(
                message.fee_payer,
            ));
        }
        payer.lamports = remaining;
        if remaining == 0 {
            self.accounts.remove(&message.fee_payer);
        }
        *fee = tx_fee;

        // 3. Рабочие копии аккаунтов. Сам леджер изменится, только если всё пройдёт успешно.
        let mut working: Vec<(Pubkey, Account)> = account_keys(message)
            .into_iter()
            .map(|key| (key, self.accounts.get(&key).cloned().unwrap_or_default()))
            .collect();

        // 4. Инструкции — по очереди, в одном «мире» аккаунтов.
        execute_instructions(message, &mut working)?;

        // 5. Rent: изменённый аккаунт либо закрыт (0 лампортов), либо rent-exempt.
        let rent = Rent::default();
        for (key, account) in &working {
            let before = self.accounts.get(key).cloned().unwrap_or_default();
            let changed = *account != before;
            if changed
                && account.lamports > 0
                && !rent.is_exempt(account.lamports, account.data.len())
            {
                return Err(TransactionError::InsufficientFundsForRent(*key));
            }
        }

        // 6. Фиксируем. Аккаунт с нулевым балансом удаляется — так в Solana «закрывают» аккаунты.
        for (key, account) in working {
            if account.lamports == 0 {
                self.accounts.remove(&key);
            } else {
                self.accounts.insert(key, account);
            }
        }
        Ok(())
    }
}

/// Все адреса транзакции без повторов: плательщик, аккаунты инструкций, программы.
fn account_keys(message: &Message) -> Vec<Pubkey> {
    let mut keys = vec![message.fee_payer];
    for instruction in &message.instructions {
        let metas = instruction.accounts.iter().map(|meta| meta.pubkey);
        for key in metas.chain([instruction.program_id]) {
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
    }
    keys
}

/// Проверить все подписи; вернуть список подписантов.
fn verify_signatures(tx: &Transaction) -> Result<Vec<Pubkey>, TransactionError> {
    let message = tx.message.to_bytes();
    tx.signatures
        .iter()
        .map(|(signer, signature)| {
            let key = VerifyingKey::from_bytes(&signer.to_bytes())
                .map_err(|_| TransactionError::SignatureFailure)?;
            key.verify_strict(&message, &Signature::from_bytes(signature))
                .map_err(|_| TransactionError::SignatureFailure)?;
            Ok(*signer)
        })
        // Iterator<Item = Result<T, E>> → Result<Vec<T>, E>: первая ошибка останавливает сбор.
        .collect()
}

fn execute_instructions(
    message: &Message,
    working: &mut [(Pubkey, Account)],
) -> Result<(), TransactionError> {
    // По одному AccountInfo на аккаунт транзакции — ссылки прямо в рабочие копии.
    // Время жизни этих AccountInfo привязано к заимствованию `working`.
    let infos: Vec<AccountInfo> = working
        .iter_mut()
        .map(|(key, account)| {
            let Account {
                lamports,
                data,
                owner,
                executable,
            } = account;
            AccountInfo::new(key, false, false, lamports, data, owner, *executable)
        })
        .collect();

    for (index, instruction) in message.instructions.iter().enumerate() {
        // Аккаунты в порядке инструкции и с её флагами. Клоны делят память с `infos`.
        let accounts: Vec<AccountInfo> = instruction
            .accounts
            .iter()
            .map(|meta| {
                let mut account = infos
                    .iter()
                    .find(|info| *info.key == meta.pubkey)
                    .expect("все адреса транзакции загружены")
                    .clone();
                account.is_signer = meta.is_signer;
                account.is_writable = meta.is_writable;
                account
            })
            .collect();

        invoke_program(&instruction.program_id, &accounts, &instruction.data)
            .map_err(|error| TransactionError::InstructionError(index as u8, error))?;
    }
    Ok(())
}

// === Контекст исполнения: стек вызовов, логи, «системные вызовы» =======================
//
// Программы вызывают `msg!` и `invoke_signed`, не передавая никакого контекста.
// В Solana это системные вызовы виртуальной машины. Здесь — thread-local
// переменная: у каждого потока свой контекст, поэтому тесты не мешают друг другу.

#[derive(Default)]
struct InvokeContext {
    /// Программы, которые можно вызывать (копия из леджера на время транзакции).
    programs: Vec<(Pubkey, ProcessInstruction)>,
    logs: Vec<String>,
    /// Стек вызовов: транзакция → программа → CPI → …
    stack: Vec<Frame>,
    /// Ошибка упавшего CPI: программа не может её «проглотить».
    cpi_error: Option<InstructionError>,
}

/// Уровень стека: какая программа выполняется и какие аккаунты ей выдал рантайм.
struct Frame {
    program_id: Pubkey,
    accounts: BTreeMap<Pubkey, FrameAccount>,
    /// Сумма лампортов в начале вызова — к концу вызова она должна совпасть.
    lamports_at_start: u128,
    /// Ограничивать ли рост данных за вызов (+10 КиБ). Не ограничиваем только System
    /// Program на верхнем уровне: так создают аккаунты размером до 10 МиБ.
    limit_growth: bool,
}

/// Аккаунт, выданный программе рантаймом.
///
/// Права рантайм берёт отсюда, а не из `AccountInfo`, который программа передаёт
/// в CPI: поля `is_signer` / `is_writable` публичные, и у своего клона программа
/// может выставить что угодно.
struct FrameAccount {
    is_signer: bool,
    is_writable: bool,
    /// Адреса ячеек `Rc` с лампортами, данными и владельцем. По ним рантайм отличает
    /// настоящий `AccountInfo` (или его клон) от «муляжа», собранного программой.
    memory: [usize; 3],
    /// Состояние на момент начала вызова (или последнего CPI).
    snapshot: AccountSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AccountSnapshot {
    lamports: u64,
    data: Vec<u8>,
    owner: Pubkey,
}

/// Баланс аккаунта. `try_borrow`, а не `borrow`: если программа оставила
/// заимствование (например, через `mem::forget`), это её ошибка, а не паника валидатора.
fn lamports_of(account: &AccountInfo) -> Result<u64, InstructionError> {
    account
        .lamports
        .try_borrow()
        .map(|lamports| **lamports)
        .map_err(|_| InstructionError::AccountBorrowOutstanding)
}

impl AccountSnapshot {
    fn of(account: &AccountInfo) -> Result<Self, InstructionError> {
        let outstanding = |_| InstructionError::AccountBorrowOutstanding;
        Ok(Self {
            lamports: lamports_of(account)?,
            data: account.data.try_borrow().map_err(outstanding)?.to_vec(),
            owner: **account.owner.try_borrow().map_err(outstanding)?,
        })
    }
}

fn memory_of(account: &AccountInfo) -> [usize; 3] {
    [
        Rc::as_ptr(&account.lamports).addr(),
        Rc::as_ptr(&account.data).addr(),
        Rc::as_ptr(&account.owner).addr(),
    ]
}

/// Сумма лампортов по аккаунтам без повторов. `u128` — чтобы сумма многих `u64`
/// точно не переполнилась.
fn lamports_sum(accounts: &[AccountInfo]) -> Result<u128, InstructionError> {
    let mut seen = BTreeSet::new();
    let mut sum = 0u128;
    for account in accounts {
        if seen.insert(*account.key) {
            sum += u128::from(lamports_of(account)?);
        }
    }
    Ok(sum)
}

impl Frame {
    fn new(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        limit_growth: bool,
    ) -> Result<Self, InstructionError> {
        let mut map: BTreeMap<Pubkey, FrameAccount> = BTreeMap::new();
        for account in accounts {
            if let Some(entry) = map.get_mut(account.key) {
                // Аккаунт пришёл повторно — права объединяем.
                entry.is_signer |= account.is_signer;
                entry.is_writable |= account.is_writable;
                continue;
            }
            let entry = FrameAccount {
                is_signer: account.is_signer,
                is_writable: account.is_writable,
                memory: memory_of(account),
                snapshot: AccountSnapshot::of(account)?,
            };
            map.insert(*account.key, entry);
        }
        let lamports_at_start = map
            .values()
            .map(|entry| u128::from(entry.snapshot.lamports))
            .sum();
        Ok(Self {
            program_id: *program_id,
            accounts: map,
            lamports_at_start,
            limit_growth,
        })
    }

    /// Проверить изменения аккаунтов с момента снимка — от имени программы этого кадра.
    fn verify(&self, accounts: &[AccountInfo]) -> Result<(), InstructionError> {
        for account in accounts {
            if let Some(entry) = self.accounts.get(account.key) {
                let post = AccountSnapshot::of(account)?;
                entry.verify(&self.program_id, &post, self.limit_growth)?;
            }
        }
        Ok(())
    }
}

impl FrameAccount {
    /// Правила владения: что программа могла изменить в аккаунте, а что — нет.
    fn verify(
        &self,
        program_id: &Pubkey,
        post: &AccountSnapshot,
        limit_growth: bool,
    ) -> Result<(), InstructionError> {
        let pre = &self.snapshot;
        if post == pre {
            return Ok(());
        }
        if !self.is_writable {
            return Err(if post.lamports != pre.lamports {
                InstructionError::ReadonlyLamportChange
            } else {
                InstructionError::ReadonlyDataModified
            });
        }
        // Пополнять можно любой writable-аккаунт, а списывать и менять данные —
        // только аккаунт, которым программа владеет.
        let owned = pre.owner == *program_id;
        if post.lamports < pre.lamports && !owned {
            return Err(InstructionError::ExternalAccountLamportSpend);
        }
        if post.data != pre.data && !owned {
            return Err(InstructionError::ExternalAccountDataModified);
        }
        // Отдать аккаунт другой программе можно, только если его данные пусты или
        // обнулены: иначе новый владелец получил бы готовые байты чужого формата.
        if post.owner != pre.owner && (!owned || post.data.iter().any(|&byte| byte != 0)) {
            return Err(InstructionError::ModifiedProgramId);
        }
        // Размер данных: не больше 10 МиБ всего и не больше +10 КиБ за вызов
        // (столько свободного места настоящий рантайм оставляет после данных аккаунта).
        let too_big = post.data.len() as u64 > system_program::MAX_PERMITTED_DATA_LENGTH;
        let grew_too_much =
            limit_growth && post.data.len() > pre.data.len() + MAX_PERMITTED_DATA_INCREASE;
        if too_big || grew_too_much {
            return Err(InstructionError::InvalidRealloc);
        }
        Ok(())
    }
}

thread_local! {
    static CONTEXT: RefCell<InvokeContext> = RefCell::new(InvokeContext::default());
}

/// «Системный вызов» логирования (его вызывает `msg!`).
pub(crate) fn syscall_log(message: &str) {
    CONTEXT.with_borrow_mut(|context| context.logs.push(message.to_owned()));
}

/// «Системный вызов» CPI (его вызывает `invoke_signed`).
pub(crate) fn syscall_invoke_signed(
    instruction: &Instruction,
    account_infos: &[AccountInfo],
    signers_seeds: &[&[&[u8]]],
) -> ProgramResult {
    cpi(instruction, account_infos, signers_seeds).map_err(|error| {
        // В настоящей Solana упавший CPI сразу прерывает вызывающую программу.
        // Мы возвращаем ей ошибку, но запоминаем настоящую причину: даже если
        // программа проигнорирует `Err`, инструкция завершится именно ею.
        let returned = match &error {
            InstructionError::Custom(code) => ProgramError::Custom(*code),
            InstructionError::Program(program_error) => program_error.clone(),
            _ => ProgramError::InvalidArgument,
        };
        CONTEXT.with_borrow_mut(|context| context.cpi_error = Some(error));
        returned
    })
}

fn cpi(
    instruction: &Instruction,
    account_infos: &[AccountInfo],
    signers_seeds: &[&[&[u8]]],
) -> Result<(), InstructionError> {
    // Что рантайм знает о вызывающей программе: её адрес, программы ниже по стеку
    // и выданные ей аккаунты — с правами и «отпечатками» памяти.
    let (caller, outer_programs, issued) = CONTEXT.with_borrow(|context| {
        let (frame, outer) = context
            .stack
            .split_last()
            .expect("CPI возможен только изнутри программы");
        let issued: BTreeMap<Pubkey, (bool, bool, [usize; 3])> = frame
            .accounts
            .iter()
            .map(|(key, entry)| (*key, (entry.is_signer, entry.is_writable, entry.memory)))
            .collect();
        let outer: Vec<Pubkey> = outer.iter().map(|frame| frame.program_id).collect();
        (frame.program_id, outer, issued)
    });

    // Вызвать саму себя (A → A) можно, вернуться в программу ниже по стеку (A → B → A) — нет.
    if instruction.program_id != caller && outer_programs.contains(&instruction.program_id) {
        return Err(InstructionError::ReentrancyNotAllowed);
    }

    // Каждый AccountInfo должен быть выдан рантаймом (или быть клоном выданного).
    // «Муляж» с тем же адресом, но своей памятью подсунул бы вызываемой программе
    // фальшивые баланс и данные.
    for info in account_infos {
        let genuine = issued
            .get(info.key)
            .is_some_and(|(_, _, memory)| *memory == memory_of(info));
        if !genuine {
            syscall_log(&format!(
                "Invalid account info for {}: not issued by the runtime",
                info.key
            ));
            return Err(InstructionError::MissingAccount);
        }
    }

    // Адреса, за которые «подписалась» программа, — PDA из её сидов. В расчёте
    // участвует program id ВЫЗЫВАЮЩЕЙ программы, поэтому чужой PDA не подписать.
    let pda_signers = signers_seeds
        .iter()
        .map(|seeds| Pubkey::create_program_address(seeds, &caller))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            syscall_log(&format!(
                "Could not create program address with signer seeds: {error}"
            ));
            InstructionError::Program(error.into())
        })?;

    // Аккаунты для вызываемой программы. Права можно только сохранить или понизить,
    // причём права вызывающей берём из записей рантайма, а не из флагов её AccountInfo.
    let mut callee_accounts = Vec::with_capacity(instruction.accounts.len());
    for meta in &instruction.accounts {
        let account = account_infos
            .iter()
            .find(|info| *info.key == meta.pubkey)
            .ok_or(InstructionError::MissingAccount)?;
        let &(caller_signed, caller_writable, _) = issued
            .get(&meta.pubkey)
            .ok_or(InstructionError::MissingAccount)?;
        if meta.is_writable && !caller_writable {
            syscall_log(&format!("{}'s writable privilege escalated", meta.pubkey));
            return Err(InstructionError::PrivilegeEscalation);
        }
        if meta.is_signer && !caller_signed && !pda_signers.contains(&meta.pubkey) {
            syscall_log(&format!("{}'s signer privilege escalated", meta.pubkey));
            return Err(InstructionError::PrivilegeEscalation);
        }
        let mut callee_account = account.clone();
        callee_account.is_signer = meta.is_signer;
        callee_account.is_writable = meta.is_writable;
        callee_accounts.push(callee_account);
    }

    // Изменения, которые вызывающая программа успела сделать ДО вызова, проверяем
    // сейчас и от её имени: после CPI их уже не отличить от изменений вызванной.
    CONTEXT.with_borrow(|context| {
        let frame = context.stack.last().expect("кадр вызывающей программы");
        frame.verify(account_infos)
    })?;

    let result = invoke_program(&instruction.program_id, &callee_accounts, &instruction.data);

    // Изменения вызванной программы уже проверены — обновляем снимки вызывающей.
    let refreshed: Vec<(Pubkey, AccountSnapshot)> = callee_accounts
        .iter()
        .filter_map(|account| Some((*account.key, AccountSnapshot::of(account).ok()?)))
        .collect();
    CONTEXT.with_borrow_mut(|context| {
        if let Some(frame) = context.stack.last_mut() {
            for (key, snapshot) in refreshed {
                if let Some(entry) = frame.accounts.get_mut(&key) {
                    entry.snapshot = snapshot;
                }
            }
        }
    });
    result
}

/// Вызвать программу: и для инструкции транзакции (уровень 1), и для CPI.
fn invoke_program(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    data: &[u8],
) -> Result<(), InstructionError> {
    let (entrypoint, depth) = CONTEXT.with_borrow(|context| {
        let entrypoint = context
            .programs
            .iter()
            .find(|(id, _)| id == program_id)
            .map(|(_, entrypoint)| *entrypoint);
        (entrypoint, context.stack.len() + 1)
    });
    let entrypoint = entrypoint.ok_or(InstructionError::UnsupportedProgramId)?;
    if depth > MAX_INVOKE_DEPTH {
        return Err(InstructionError::CallDepth);
    }

    let limit_growth = depth > 1 || !system_program::check_id(program_id);
    let frame = Frame::new(program_id, accounts, limit_growth)?;
    syscall_log(&format!("Program {program_id} invoke [{depth}]"));
    CONTEXT.with_borrow_mut(|context| context.stack.push(frame));

    // Паника в программе не роняет валидатор: ловим её и превращаем в ошибку инструкции.
    let outcome = panic::catch_unwind(AssertUnwindSafe(|| entrypoint(program_id, accounts, data)));

    let (frame, cpi_error) = CONTEXT.with_borrow_mut(|context| {
        let frame = context.stack.pop().expect("стек вызовов сбалансирован");
        (frame, context.cpi_error.take())
    });

    let result = match (outcome, cpi_error) {
        (Err(panic), _) => {
            syscall_log(&format!(
                "Program log: panicked: {}",
                panic_message(&*panic)
            ));
            Err(InstructionError::ProgramFailedToComplete)
        }
        (Ok(_), Some(cpi_error)) => Err(cpi_error),
        (Ok(program_result), None) => program_result.map_err(InstructionError::from),
    }
    .and_then(|()| frame.verify(accounts))
    .and_then(|()| {
        // SOL не появляются и не исчезают: сумма балансов та же, что в начале вызова.
        if lamports_sum(accounts)? == frame.lamports_at_start {
            Ok(())
        } else {
            Err(InstructionError::UnbalancedInstruction)
        }
    });

    match &result {
        Ok(()) => syscall_log(&format!("Program {program_id} success")),
        Err(InstructionError::ProgramFailedToComplete) => {
            syscall_log(&format!(
                "Program {program_id} failed: SBF program panicked"
            ));
        }
        Err(error) => syscall_log(&format!("Program {program_id} failed: {error}")),
    }
    result
}

fn panic_message(payload: &(dyn Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "unknown panic".to_owned()
    }
}

// === Хранение: адреса в JSON — base58-строками ========================================
// Это off-chain забота, поэтому реализации serde живут здесь, а не в `sdk`.

impl Serialize for Pubkey {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Pubkey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}
