//! Логика программы.
//!
//! Главное правило on-chain кода: **не доверять ничему, что пришло в инструкции**.
//! Адреса, флаги и данные подставляет клиент — а клиентом может быть кто угодно.
//! Поэтому каждая инструкция сначала проверяет: кто подписал, чей это аккаунт,
//! тот ли это адрес — и только потом что-то меняет.

use borsh::BorshDeserialize;

use super::error::VaultError;
use super::instruction::VaultInstruction;
use super::state::{AccountState, Vault};
use super::{VAULT_SEED, find_vault_address};
use crate::sdk::account_info::{AccountInfo, next_account_info};
use crate::sdk::program::{invoke, invoke_signed};
use crate::sdk::program_error::{ProgramError, ProgramResult};
use crate::sdk::pubkey::Pubkey;
use crate::sdk::rent::Rent;
use crate::system_program;
use crate::system_program::instruction as system_instruction;
use crate::{msg, require};

/// Точка входа: рантайм передаёт адрес программы, аккаунты и байты инструкции.
pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    // 1. Байты → enum. Мусор во входных данных — это ошибка, а не паника.
    let instruction = VaultInstruction::try_from_slice(instruction_data)
        .map_err(|_| ProgramError::InvalidInstructionData)?;

    // 2. `match` по enum: забыть обработать вариант не даст компилятор.
    match instruction {
        VaultInstruction::Initialize => {
            msg!("Instruction: Initialize");
            process_initialize(program_id, accounts)
        }
        VaultInstruction::Deposit { amount } => {
            msg!("Instruction: Deposit");
            process_deposit(program_id, accounts, amount)
        }
        VaultInstruction::Withdraw { amount } => {
            msg!("Instruction: Withdraw");
            process_withdraw(program_id, accounts, amount)
        }
        VaultInstruction::Close => {
            msg!("Instruction: Close");
            process_close(program_id, accounts)
        }
    }
}

// === Initialize =====================================================================

fn process_initialize(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    // Классический способ разобрать аккаунты: итератор + next_account_info.
    // (В остальных инструкциях — другой способ: структура + TryFrom, см. ниже.)
    let accounts_iter = &mut accounts.iter();
    let owner = next_account_info(accounts_iter)?;
    let vault = next_account_info(accounts_iter)?;
    let system_program_account = next_account_info(accounts_iter)?;

    require!(owner.is_signer, ProgramError::MissingRequiredSignature);
    require!(
        system_program::check_id(system_program_account.key),
        ProgramError::IncorrectProgramId
    );

    // Адрес копилки не берём на веру — пересчитываем из сидов.
    let (expected_vault, bump) = find_vault_address(owner.key, program_id);
    require!(
        *vault.key == expected_vault,
        VaultError::InvalidVaultAddress
    );

    // Столько лампортов нужно, чтобы аккаунт на Vault::LEN байт мог существовать.
    let space = Vault::LEN;
    let lamports = Rent::get()?.minimum_balance(space);

    // CPI в System Program: «создай аккаунт `vault`, выдели `space` байт и сделай
    // владельцем нашу программу». Новый аккаунт обязан подписать create_account;
    // у PDA ключа нет, поэтому «подписываем» сидами через invoke_signed.
    //
    // Нюанс: если кто-то заранее переведёт SOL на адрес PDA, create_account
    // упадёт с «already in use». Anchor в `init` обходит это через transfer + allocate + assign.
    let signer_seeds: &[&[u8]] = &[VAULT_SEED, owner.key.as_ref(), &[bump]];
    invoke_signed(
        &system_instruction::create_account(
            owner.key,
            vault.key,
            lamports,
            space as u64,
            program_id,
        ),
        // clone() дешёвый: копируются ссылки и Rc, а не данные аккаунта.
        &[owner.clone(), vault.clone(), system_program_account.clone()],
        &[signer_seeds],
    )?;

    // После CPI аккаунт принадлежит нам и в нём Vault::LEN нулевых байт — записываем состояние.
    let state = Vault {
        owner: *owner.key,
        bump,
        total_deposited: 0,
        total_withdrawn: 0,
    };
    state.save(vault)?;

    msg!("Vault {} created, bump {}", vault.key, bump);
    Ok(())
}

// === Deposit =========================================================================

/// Аккаунты инструкции Deposit — «типизированный» разбор среза.
///
/// Так устроен `#[derive(Accounts)]` в Anchor и типичный код на Pinocchio.
/// Два времени жизни: `'a` — ссылки на элементы среза, `'info` — память аккаунтов.
/// Один общий `'a` здесь не подойдёт: `AccountInfo<'info>` инвариантен по `'info`
/// (внутри `RefCell`), и компилятор не сможет «укоротить» время жизни.
struct DepositAccounts<'a, 'info> {
    depositor: &'a AccountInfo<'info>,
    vault: &'a AccountInfo<'info>,
    system_program: &'a AccountInfo<'info>,
}

impl<'a, 'info> TryFrom<&'a [AccountInfo<'info>]> for DepositAccounts<'a, 'info> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo<'info>]) -> Result<Self, Self::Error> {
        // Срез-паттерн + let-else: первые три аккаунта или ошибка. `..` — «остальные».
        let [depositor, vault, system_program_account, ..] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };
        // Проверки «формы» — как типы `Signer<'info>` и `Program<'info, System>` в Anchor.
        require!(depositor.is_signer, ProgramError::MissingRequiredSignature);
        require!(
            system_program::check_id(system_program_account.key),
            ProgramError::IncorrectProgramId
        );
        Ok(Self {
            depositor,
            vault,
            system_program: system_program_account,
        })
    }
}

fn process_deposit(program_id: &Pubkey, accounts: &[AccountInfo], amount: u64) -> ProgramResult {
    let accs = DepositAccounts::try_from(accounts)?;
    require!(amount > 0, VaultError::ZeroAmount);

    // load(): владелец аккаунта = наша программа + верный дискриминатор.
    // assert_address(): это именно PDA копилки, а не другой аккаунт той же программы.
    let mut state = Vault::load(accs.vault, program_id)?;
    state.assert_address(accs.vault.key, program_id)?;

    // Кошелёк вносящего принадлежит System Program — списать с него может только она.
    // Поэтому CPI; подпись depositor-а (is_signer) «пробрасывается» в вызов.
    invoke(
        &system_instruction::transfer(accs.depositor.key, accs.vault.key, amount),
        &[
            accs.depositor.clone(),
            accs.vault.clone(),
            accs.system_program.clone(),
        ],
    )?;

    // Деньги в u64-лампортах, арифметика только checked_*: переполнение → ошибка, не мусор.
    state.total_deposited = state
        .total_deposited
        .checked_add(amount)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    state.save(accs.vault)?;

    msg!(
        "Deposited {} lamports, total deposited {}",
        amount,
        state.total_deposited
    );
    Ok(())
}

// === Withdraw и Close ===============================================================

/// Аккаунты Withdraw и Close: владелец и его копилка.
struct OwnerAccounts<'a, 'info> {
    owner: &'a AccountInfo<'info>,
    vault: &'a AccountInfo<'info>,
}

impl<'a, 'info> TryFrom<&'a [AccountInfo<'info>]> for OwnerAccounts<'a, 'info> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo<'info>]) -> Result<Self, Self::Error> {
        let [owner, vault, ..] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };
        require!(owner.is_signer, ProgramError::MissingRequiredSignature);
        Ok(Self { owner, vault })
    }
}

/// Загрузить копилку и убедиться, что подписант — её владелец.
fn load_owned_vault(accs: &OwnerAccounts, program_id: &Pubkey) -> Result<Vault, ProgramError> {
    let state = Vault::load(accs.vault, program_id)?;
    state.assert_address(accs.vault.key, program_id)?;
    // Главная проверка безопасности (в Anchor — `has_one = owner`). Подпись есть у
    // любого, кто прислал транзакцию; важно, что подписал именно владелец копилки.
    require!(state.owner == *accs.owner.key, VaultError::Unauthorized);
    Ok(state)
}

fn process_withdraw(program_id: &Pubkey, accounts: &[AccountInfo], amount: u64) -> ProgramResult {
    let accs = OwnerAccounts::try_from(accounts)?;
    require!(amount > 0, VaultError::ZeroAmount);
    let mut state = load_owned_vault(&accs, program_id)?;

    // Снять можно всё, кроме rent-резерва: копилка должна остаться rent-exempt.
    let reserve = Rent::get()?.minimum_balance(accs.vault.data_len());
    let available = accs.vault.lamports().saturating_sub(reserve);
    require!(amount <= available, VaultError::InsufficientFunds);

    // Копилка принадлежит НАШЕЙ программе, поэтому списываем напрямую, без CPI.
    // (System Program тут не поможет: её transfer требует, чтобы у `from` не было данных.)
    // `**` снимает два слоя: RefMut<&mut u64> → &mut u64 → u64.
    let owner_balance = accs
        .owner
        .lamports()
        .checked_add(amount)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    **accs.vault.try_borrow_mut_lamports()? -= amount; // безопасно: выше проверили amount ≤ available
    **accs.owner.try_borrow_mut_lamports()? = owner_balance;

    state.total_withdrawn = state
        .total_withdrawn
        .checked_add(amount)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    state.save(accs.vault)?;

    msg!(
        "Withdrew {} lamports, {} still available",
        amount,
        available - amount
    );
    Ok(())
}

fn process_close(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let accs = OwnerAccounts::try_from(accounts)?;
    load_owned_vault(&accs, program_id)?;

    // Закрыть аккаунт = забрать все лампорты. Аккаунт с нулевым балансом рантайм
    // удалит в конце транзакции, а rent-депозит вернётся владельцу.
    let all_lamports = accs.vault.lamports();
    let owner_balance = accs
        .owner
        .lamports()
        .checked_add(all_lamports)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    **accs.owner.try_borrow_mut_lamports()? = owner_balance;
    **accs.vault.try_borrow_mut_lamports()? = 0;

    // Затираем данные: даже если в этой же транзакции кто-то снова пополнит адрес,
    // валидной копилки там уже не будет (защита от «воскрешения» аккаунта).
    accs.vault.try_borrow_mut_data()?.fill(0);

    msg!("Vault closed, {} lamports returned to owner", all_lamports);
    Ok(())
}
