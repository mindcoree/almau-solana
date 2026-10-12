//! Логика программы — перенесена из piggy без изменений, поменялись только `use`.
//!
//! Главное правило on-chain кода: **не доверять ничему, что пришло в инструкции**.

use borsh::BorshDeserialize;
use solana_account_info::{AccountInfo, next_account_info};
use solana_cpi::{invoke, invoke_signed};
use solana_msg::msg;
use solana_program_error::{ProgramError, ProgramResult};
use solana_pubkey::Pubkey;
use solana_rent::Rent;
// `Rent::get()` — метод трейта `Sysvar`: в сети он читает sysvar через syscall.
use solana_system_interface::{instruction as system_instruction, program as system_program};
use solana_sysvar::Sysvar;

use crate::error::VaultError;
use crate::instruction::VaultInstruction;
use crate::require;
use crate::state::{AccountState, Vault};
use crate::{VAULT_SEED, find_vault_address};

/// Точка входа: рантайм передаёт адрес программы, аккаунты и байты инструкции.
pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    let instruction = VaultInstruction::try_from_slice(instruction_data)
        .map_err(|_| ProgramError::InvalidInstructionData)?;

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
    require!(*vault.key == expected_vault, VaultError::InvalidVaultAddress);

    let space = Vault::LEN;
    let lamports = Rent::get()?.minimum_balance(space);

    // CPI в System Program с подписью PDA через сиды.
    let signer_seeds: &[&[u8]] = &[VAULT_SEED, owner.key.as_ref(), &[bump]];
    invoke_signed(
        &system_instruction::create_account(
            owner.key,
            vault.key,
            lamports,
            space as u64,
            program_id,
        ),
        &[owner.clone(), vault.clone(), system_program_account.clone()],
        &[signer_seeds],
    )?;

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

struct DepositAccounts<'a, 'info> {
    depositor: &'a AccountInfo<'info>,
    vault: &'a AccountInfo<'info>,
    system_program: &'a AccountInfo<'info>,
}

impl<'a, 'info> TryFrom<&'a [AccountInfo<'info>]> for DepositAccounts<'a, 'info> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo<'info>]) -> Result<Self, Self::Error> {
        let [depositor, vault, system_program_account, ..] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };
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

    let mut state = Vault::load(accs.vault, program_id)?;
    state.assert_address(accs.vault.key, program_id)?;

    invoke(
        &system_instruction::transfer(accs.depositor.key, accs.vault.key, amount),
        &[
            accs.depositor.clone(),
            accs.vault.clone(),
            accs.system_program.clone(),
        ],
    )?;

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

/// Загрузить копилку и убедиться, что подписант — её владелец (`has_one = owner` в Anchor).
fn load_owned_vault(accs: &OwnerAccounts, program_id: &Pubkey) -> Result<Vault, ProgramError> {
    let state = Vault::load(accs.vault, program_id)?;
    state.assert_address(accs.vault.key, program_id)?;
    require!(state.owner == *accs.owner.key, VaultError::Unauthorized);
    Ok(state)
}

fn process_withdraw(program_id: &Pubkey, accounts: &[AccountInfo], amount: u64) -> ProgramResult {
    let accs = OwnerAccounts::try_from(accounts)?;
    require!(amount > 0, VaultError::ZeroAmount);
    let mut state = load_owned_vault(&accs, program_id)?;

    let reserve = Rent::get()?.minimum_balance(accs.vault.data_len());
    let available = accs.vault.lamports().saturating_sub(reserve);
    require!(amount <= available, VaultError::InsufficientFunds);

    // Копилка принадлежит нашей программе — списываем напрямую, без CPI.
    let owner_balance = accs
        .owner
        .lamports()
        .checked_add(amount)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    **accs.vault.try_borrow_mut_lamports()? -= amount;
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

    let all_lamports = accs.vault.lamports();
    let owner_balance = accs
        .owner
        .lamports()
        .checked_add(all_lamports)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    **accs.owner.try_borrow_mut_lamports()? = owner_balance;
    **accs.vault.try_borrow_mut_lamports()? = 0;
    accs.vault.try_borrow_mut_data()?.fill(0);

    msg!("Vault closed, {} lamports returned to owner", all_lamports);
    Ok(())
}
