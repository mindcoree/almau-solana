//! Интеграционные тесты: сценарии целиком — клиент собирает транзакции, рантайм
//! исполняет. Стиль как у тестов на LiteSVM: `airdrop` → `send_transaction` → проверки.

use piggy::client::{Keypair, LAMPORTS_PER_SOL, Transaction};
use piggy::runtime::{
    InstructionError, LAMPORTS_PER_SIGNATURE, Ledger, TransactionError, TransactionResult,
};
use piggy::sdk::account_info::AccountInfo;
use piggy::sdk::instruction::{AccountMeta, Instruction};
use piggy::sdk::program::{invoke, invoke_signed};
use piggy::sdk::program_error::{ProgramError, ProgramResult};
use piggy::sdk::pubkey::Pubkey;
use piggy::system_program::instruction as system_instruction;
use piggy::system_program::{self, SystemError};
use piggy::vault::error::VaultError;
use piggy::vault::instruction::{self as vault_ix, VaultInstruction};
use piggy::vault::state::{AccountState, Vault};
use piggy::vault::{self, VAULT_SEED, find_vault_address};

/// Комиссия транзакции с одной подписью.
const FEE: u64 = LAMPORTS_PER_SIGNATURE;
const SOL: u64 = LAMPORTS_PER_SOL;

/// «Сеть» и два пользователя по 10 SOL.
fn setup() -> (Ledger, Keypair, Keypair) {
    let mut ledger = Ledger::new();
    let (alice, bob) = (Keypair::new(), Keypair::new());
    ledger.airdrop(&alice.pubkey(), 10 * SOL).unwrap();
    ledger.airdrop(&bob.pubkey(), 10 * SOL).unwrap();
    (ledger, alice, bob)
}

fn send(ledger: &mut Ledger, instructions: &[Instruction], signer: &Keypair) -> TransactionResult {
    let tx = Transaction::new_signed_with_payer(instructions, Some(&signer.pubkey()), &[signer]);
    ledger.send_transaction(&tx)
}

fn vault_of(owner: &Keypair) -> Pubkey {
    find_vault_address(&owner.pubkey(), &vault::ID).0
}

fn vault_state(ledger: &Ledger, owner: &Keypair) -> Vault {
    let account = ledger
        .get_account(&vault_of(owner))
        .expect("копилка существует");
    Vault::try_from_bytes(&account.data).unwrap()
}

/// Создать копилку и сразу положить в неё `lamports` — одной транзакцией.
fn create_vault(ledger: &mut Ledger, owner: &Keypair, lamports: u64) {
    let key = owner.pubkey();
    let instructions = [
        vault_ix::initialize(&key),
        vault_ix::deposit(&key, &key, lamports),
    ];
    send(ledger, &instructions, owner).unwrap();
}

/// Ошибка инструкции из результата упавшей транзакции.
fn instruction_error(result: TransactionResult) -> InstructionError {
    match result.expect_err("транзакция должна упасть").err {
        TransactionError::InstructionError(_, error) => error,
        other => panic!("ожидалась ошибка инструкции, получено: {other}"),
    }
}

/// Так ошибка программы выглядит на уровне рантайма.
fn program_error(error: impl Into<ProgramError>) -> InstructionError {
    InstructionError::from(error.into())
}

#[test]
fn full_lifecycle_costs_only_fees() {
    let (mut ledger, alice, _) = setup();
    let owner = alice.pubkey();
    let vault = vault_of(&alice);
    let rent = ledger.minimum_balance_for_rent_exemption(Vault::LEN);
    assert_eq!(rent, 1_287_600);

    // Initialize: PDA создан, принадлежит программе и держит rent-минимум.
    let meta = send(&mut ledger, &[vault_ix::initialize(&owner)], &alice).unwrap();
    let account = ledger.get_account(&vault).unwrap();
    assert_eq!(account.owner, vault::ID);
    assert_eq!(account.lamports, rent);
    assert_eq!(account.data.len(), Vault::LEN);
    let state = vault_state(&ledger, &alice);
    assert_eq!(state.owner, owner);
    assert_eq!(state.bump, find_vault_address(&owner, &vault::ID).1);
    // В логах видно CPI в System Program — второй уровень стека вызовов.
    let cpi_line = format!("Program {} invoke [2]", system_program::ID);
    assert!(meta.logs.contains(&cpi_line));

    send(
        &mut ledger,
        &[vault_ix::deposit(&owner, &owner, 2 * SOL)],
        &alice,
    )
    .unwrap();
    assert_eq!(ledger.get_balance(&vault), rent + 2 * SOL);

    send(
        &mut ledger,
        &[vault_ix::withdraw(&owner, &owner, SOL / 2)],
        &alice,
    )
    .unwrap();
    let state = vault_state(&ledger, &alice);
    assert_eq!(state.total_deposited, 2 * SOL);
    assert_eq!(state.total_withdrawn, SOL / 2);

    // Close: аккаунт удалён, rent-депозит вернулся — Алиса потеряла только 4 комиссии.
    send(&mut ledger, &[vault_ix::close(&owner, &owner)], &alice).unwrap();
    assert!(ledger.get_account(&vault).is_none());
    assert_eq!(ledger.get_balance(&owner), 10 * SOL - 4 * FEE);

    // Адрес снова свободен — копилку можно создать заново.
    send(&mut ledger, &[vault_ix::initialize(&owner)], &alice).unwrap();
}

#[test]
fn anyone_can_deposit_but_only_owner_can_withdraw() {
    let (mut ledger, alice, bob) = setup();
    create_vault(&mut ledger, &alice, SOL);

    // Боб пополняет копилку Алисы — можно.
    let deposit = vault_ix::deposit(&bob.pubkey(), &alice.pubkey(), SOL);
    send(&mut ledger, &[deposit], &bob).unwrap();
    assert_eq!(vault_state(&ledger, &alice).total_deposited, 2 * SOL);

    // Боб снимает из копилки Алисы: подпись у него есть, но он не владелец.
    let bob_before = ledger.get_balance(&bob.pubkey());
    let vault_before = vault_state(&ledger, &alice);
    let withdraw = vault_ix::withdraw(&bob.pubkey(), &alice.pubkey(), 1);
    let result = send(&mut ledger, &[withdraw], &bob);
    assert_eq!(
        instruction_error(result),
        program_error(VaultError::Unauthorized)
    );

    // Копилка не изменилась, а комиссию Боб заплатил и за упавшую транзакцию.
    assert_eq!(vault_state(&ledger, &alice), vault_before);
    assert_eq!(ledger.get_balance(&bob.pubkey()), bob_before - FEE);
}

#[test]
fn rent_reserve_cannot_be_withdrawn() {
    let (mut ledger, alice, _) = setup();
    create_vault(&mut ledger, &alice, SOL);
    let owner = alice.pubkey();

    let too_much = vault_ix::withdraw(&owner, &owner, SOL + 1);
    let result = send(&mut ledger, &[too_much], &alice);
    assert_eq!(
        instruction_error(result),
        program_error(VaultError::InsufficientFunds)
    );

    send(
        &mut ledger,
        &[vault_ix::withdraw(&owner, &owner, SOL)],
        &alice,
    )
    .unwrap();
    assert_eq!(
        ledger.get_balance(&vault_of(&alice)),
        ledger.minimum_balance_for_rent_exemption(Vault::LEN)
    );
}

#[test]
fn zero_amount_is_rejected() {
    let (mut ledger, alice, _) = setup();
    create_vault(&mut ledger, &alice, SOL);
    let owner = alice.pubkey();
    let result = send(&mut ledger, &[vault_ix::deposit(&owner, &owner, 0)], &alice);
    assert_eq!(
        instruction_error(result),
        program_error(VaultError::ZeroAmount)
    );
}

#[test]
fn second_initialize_fails_inside_system_program() {
    let (mut ledger, alice, _) = setup();
    create_vault(&mut ledger, &alice, SOL);

    let result = send(
        &mut ledger,
        &[vault_ix::initialize(&alice.pubkey())],
        &alice,
    );
    let logs = &result.as_ref().unwrap_err().meta.logs;
    assert!(logs.iter().any(|line| line.contains("already in use")));
    // Ошибку вернула System Program (код 0), наша программа лишь пробросила её через `?`.
    assert_eq!(
        instruction_error(result),
        program_error(SystemError::AccountAlreadyInUse)
    );
}

#[test]
fn multi_instruction_transaction_is_atomic() {
    let (mut ledger, alice, _) = setup();
    let owner = alice.pubkey();

    // Вторая инструкция просит больше, чем есть у Алисы, — падает вся транзакция.
    let instructions = [
        vault_ix::initialize(&owner),
        vault_ix::deposit(&owner, &owner, 100 * SOL),
    ];
    let failed = send(&mut ledger, &instructions, &alice).unwrap_err();
    assert_eq!(
        failed.err,
        TransactionError::InstructionError(
            1,
            program_error(SystemError::ResultWithNegativeLamports)
        )
    );
    // Первая инструкция тоже откатилась: копилки нет, списана только комиссия.
    assert!(ledger.get_account(&vault_of(&alice)).is_none());
    assert_eq!(ledger.get_balance(&owner), 10 * SOL - FEE);
}

#[test]
fn fee_cannot_leave_payer_below_rent_minimum() {
    let mut ledger = Ledger::new();
    let carol = Keypair::new();
    let minimum = ledger.minimum_balance_for_rent_exemption(0);
    ledger.airdrop(&carol.pubkey(), minimum).unwrap();

    // Любая транзакция стоит комиссию, а после неё у Кэрол осталось бы меньше минимума.
    let transfer = system_instruction::transfer(&carol.pubkey(), &carol.pubkey(), 1);
    let failed = send(&mut ledger, &[transfer], &carol).unwrap_err();
    assert_eq!(
        failed.err,
        TransactionError::InsufficientFundsForRent(carol.pubkey())
    );
    // Такая транзакция отклоняется целиком — комиссию не списали.
    assert_eq!(ledger.get_balance(&carol.pubkey()), minimum);
}

#[test]
fn signatures_are_checked_by_the_runtime() {
    let (mut ledger, alice, bob) = setup();
    create_vault(&mut ledger, &alice, 2 * SOL);
    let forged = [vault_ix::withdraw(&alice.pubkey(), &alice.pubkey(), SOL)];

    // Боб помечает Алису подписантом, но подписывает только сам.
    let tx = Transaction::new_signed_with_payer(&forged, Some(&bob.pubkey()), &[&bob]);
    assert_eq!(
        ledger.send_transaction(&tx).unwrap_err().err,
        TransactionError::MissingSignature(alice.pubkey())
    );

    // Боб кладёт свою подпись под именем Алисы — ed25519 это не пропустит.
    let mut tx = Transaction::new_signed_with_payer(&forged, Some(&alice.pubkey()), &[&bob]);
    tx.signatures[0].0 = alice.pubkey();
    assert_eq!(
        ledger.send_transaction(&tx).unwrap_err().err,
        TransactionError::SignatureFailure
    );
}

#[test]
fn program_checks_signer_flag_and_account_owner() {
    let (mut ledger, alice, bob) = setup();
    create_vault(&mut ledger, &alice, SOL);

    // Вносящий не помечен подписантом — программа не пустит,
    // иначе кто угодно «вносил» бы чужие деньги.
    let mut deposit = vault_ix::deposit(&alice.pubkey(), &alice.pubkey(), 1);
    deposit.accounts[0].is_signer = false;
    let result = send(&mut ledger, &[deposit], &bob);
    assert_eq!(
        instruction_error(result),
        program_error(ProgramError::MissingRequiredSignature)
    );

    // Вместо копилки подсунули обычный кошелёк: его владелец — не наша программа.
    let fake_vault = Instruction::new_with_borsh(
        vault::ID,
        &VaultInstruction::Withdraw { amount: 1 },
        vec![
            AccountMeta::new(bob.pubkey(), true),
            AccountMeta::new(alice.pubkey(), false),
        ],
    );
    let result = send(&mut ledger, &[fake_vault], &bob);
    assert_eq!(
        instruction_error(result),
        program_error(ProgramError::IncorrectProgramId)
    );
}

// --- Программа-«взломщик»: правила держит рантайм, а не честность программ ------------

const THIEF_ID: Pubkey = Pubkey::from_str_const("Thief11111111111111111111111111111111111111");

fn thief(program_id: &Pubkey, accounts: &[AccountInfo], data: &[u8]) -> ProgramResult {
    let [victim, receiver, vault_owner, ..] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
    match data.first() {
        // 0: списать лампорты с чужого кошелька напрямую.
        Some(0) => {
            **victim.try_borrow_mut_lamports()? -= 1_000;
            **receiver.try_borrow_mut_lamports()? += 1_000;
        }
        // 1: вписать себя владельцем в данные чужой копилки.
        Some(1) => {
            victim.try_borrow_mut_data()?[8..40].copy_from_slice(receiver.key.as_ref());
        }
        // 2: «подписать» копилку её же сидами. Но рантайм считает PDA от program id
        //    ВЫЗЫВАЮЩЕЙ программы — получится другой адрес, и подпись не засчитается.
        Some(2) => {
            let seeds: &[&[u8]] = &[VAULT_SEED, vault_owner.key.as_ref()];
            let (_, bump) = Pubkey::find_program_address(seeds, program_id);
            let seeds: &[&[u8]] = &[VAULT_SEED, vault_owner.key.as_ref(), &[bump]];
            invoke_signed(
                &system_instruction::transfer(victim.key, receiver.key, 1_000),
                &[victim.clone(), receiver.clone()],
                &[seeds],
            )?;
        }
        // 3: держать заимствование данных во время CPI.
        Some(3) => {
            let _data = victim.try_borrow_mut_data()?;
            invoke(
                &system_instruction::transfer(receiver.key, victim.key, 1),
                &[receiver.clone(), victim.clone()],
            )?;
        }
        // Остальное: паника.
        _ => panic!("boom"),
    }
    Ok(())
}

fn thief_instruction(mode: u8, victim: Pubkey, receiver: Pubkey, owner: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        THIEF_ID,
        &[mode],
        vec![
            AccountMeta::new(victim, false),
            AccountMeta::new(receiver, true),
            AccountMeta::new_readonly(owner, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
    )
}

#[test]
fn runtime_stops_a_malicious_program() {
    let (mut ledger, alice, bob) = setup();
    ledger.add_program(THIEF_ID, thief);
    create_vault(&mut ledger, &alice, SOL);
    let (owner, vault) = (alice.pubkey(), vault_of(&alice));
    let owner_before = ledger.get_balance(&owner);
    let vault_before = vault_state(&ledger, &alice);

    let mut attempt = |mode, victim| {
        let instruction = thief_instruction(mode, victim, bob.pubkey(), owner);
        instruction_error(send(&mut ledger, &[instruction], &bob))
    };
    assert_eq!(
        attempt(0, owner),
        InstructionError::ExternalAccountLamportSpend
    );
    assert_eq!(
        attempt(1, vault),
        InstructionError::ExternalAccountDataModified
    );
    assert_eq!(attempt(2, vault), InstructionError::PrivilegeEscalation);
    assert_eq!(
        attempt(3, owner),
        program_error(ProgramError::AccountBorrowFailed)
    );
    assert_eq!(attempt(9, owner), InstructionError::ProgramFailedToComplete);

    // Ни одна попытка ничего не изменила.
    assert_eq!(ledger.get_balance(&owner), owner_before);
    assert_eq!(vault_state(&ledger, &alice), vault_before);
}

#[test]
fn ledger_survives_json_roundtrip() {
    let (mut ledger, alice, _) = setup();
    create_vault(&mut ledger, &alice, 3 * SOL);
    let path = std::env::temp_dir().join(format!("piggy-{}.json", std::process::id()));

    ledger.save(&path).unwrap();
    let restored = Ledger::load_or_new(&path).unwrap();
    std::fs::remove_file(&path).unwrap();

    let vault = vault_of(&alice);
    assert_eq!(restored.get_account(&vault), ledger.get_account(&vault));
    assert_eq!(
        restored.get_balance(&alice.pubkey()),
        ledger.get_balance(&alice.pubkey())
    );
}
