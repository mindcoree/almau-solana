//! Тесты программы в LiteSVM — лёгкой виртуальной машине Solana прямо в `cargo test`.
//! Грузится тот же `counter.so`, что уходит в Devnet, поэтому сначала `cargo build-sbf`.
//!
//! Anchor собран на solana-address 1.x, а LiteSVM — на 2.x: это один и тот же 32-байтный
//! адрес, но разные типы Rust, поэтому на границе перекладываем байты (`addr`, `key`).
use anchor_lang::{AccountDeserialize, Discriminator, InstructionData, ToAccountMetas};
use counter::{accounts, instruction, Counter};
use litesvm::LiteSVM;
use solana_keypair::Keypair;
use solana_message::{AccountMeta, Address, Instruction};
use solana_signer::Signer;
use solana_transaction::Transaction;

type AnchorPubkey = anchor_lang::prelude::Pubkey;

fn addr(key: &AnchorPubkey) -> Address {
    Address::new_from_array(key.to_bytes())
}

fn key(address: &Address) -> AnchorPubkey {
    AnchorPubkey::new_from_array(address.to_bytes())
}

fn counter_pda(authority: &Address) -> AnchorPubkey {
    AnchorPubkey::find_program_address(&[b"counter", key(authority).as_ref()], &counter::ID).0
}

/// Инструкция из типов, которые Anchor сгенерировал по программе: данные — `instruction::*`,
/// аккаунты — `accounts::*`. Ровно так же клиент на TypeScript собирает её по IDL.
fn ix(data: impl InstructionData, metas: impl ToAccountMetas) -> Instruction {
    Instruction {
        program_id: addr(&counter::ID),
        accounts: metas
            .to_account_metas(None)
            .into_iter()
            .map(|m| AccountMeta { pubkey: addr(&m.pubkey), is_signer: m.is_signer, is_writable: m.is_writable })
            .collect(),
        data: data.data(),
    }
}

fn initialize(authority: &Address) -> Instruction {
    ix(
        instruction::Initialize {},
        accounts::Initialize {
            authority: key(authority),
            counter: counter_pda(authority),
            system_program: anchor_lang::system_program::ID,
        },
    )
}

fn update(data: impl InstructionData, authority: &Address, counter: AnchorPubkey) -> Instruction {
    ix(data, accounts::Update { authority: key(authority), counter })
}

struct Env {
    svm: LiteSVM,
    user: Keypair,
}

impl Env {
    fn new() -> Self {
        let mut svm = LiteSVM::new();
        let so = concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/deploy/counter.so");
        svm.add_program_from_file(addr(&counter::ID), so).expect("сначала cargo build-sbf");
        let user = Keypair::new();
        svm.airdrop(&user.pubkey(), 1_000_000_000).unwrap();
        Self { svm, user }
    }

    fn send(&mut self, ix: Instruction, signer: &Keypair) -> Result<Vec<String>, String> {
        // Новый blockhash — иначе два одинаковых increment подряд дадут одну и ту же подпись.
        self.svm.expire_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&signer.pubkey()),
            &[signer],
            self.svm.latest_blockhash(),
        );
        self.svm
            .send_transaction(tx)
            .map(|meta| meta.logs)
            .map_err(|failed| format!("{:?}\n{}", failed.err, failed.meta.logs.join("\n")))
    }

    fn counter(&self, authority: &Address) -> Counter {
        let account = self.svm.get_account(&addr(&counter_pda(authority))).expect("счётчика нет");
        assert_eq!(key(&account.owner), counter::ID, "владелец аккаунта — программа");
        Counter::try_deserialize(&mut account.data.as_slice()).unwrap()
    }
}

#[test]
fn initialize_creates_counter_at_zero() {
    let mut env = Env::new();
    let user = env.user.insecure_clone();
    let logs = env.send(initialize(&user.pubkey()), &user).unwrap();
    assert!(logs.iter().any(|l| l.contains("Instruction: Initialize")), "{logs:#?}");

    let counter = env.counter(&user.pubkey());
    assert_eq!(counter.count, 0);
    assert_eq!(counter.authority, key(&user.pubkey()));

    // 8 байт дискриминатора + 32 authority + 8 count + 1 bump
    let raw = env.svm.get_account(&addr(&counter_pda(&user.pubkey()))).unwrap().data;
    assert_eq!(raw.len(), 49);
    assert_eq!(&raw[..8], Counter::DISCRIMINATOR);
}

#[test]
fn increment_and_decrement() {
    let mut env = Env::new();
    let user = env.user.insecure_clone();
    let pda = counter_pda(&user.pubkey());
    env.send(initialize(&user.pubkey()), &user).unwrap();

    for _ in 0..3 {
        env.send(update(instruction::Increment {}, &user.pubkey(), pda), &user).unwrap();
    }
    assert_eq!(env.counter(&user.pubkey()).count, 3);

    let logs = env.send(update(instruction::Decrement {}, &user.pubkey(), pda), &user).unwrap();
    assert!(logs.iter().any(|l| l.contains("count = 2")), "{logs:#?}");
    assert_eq!(env.counter(&user.pubkey()).count, 2);
}

#[test]
fn cannot_go_below_zero() {
    let mut env = Env::new();
    let user = env.user.insecure_clone();
    env.send(initialize(&user.pubkey()), &user).unwrap();

    let err = env
        .send(update(instruction::Decrement {}, &user.pubkey(), counter_pda(&user.pubkey())), &user)
        .unwrap_err();
    // Ошибки программы в Anchor нумеруются с 6000: Overflow = 6000, Underflow = 6001.
    assert!(err.contains("Custom(6001)") && err.contains("Underflow"), "{err}");
    assert_eq!(env.counter(&user.pubkey()).count, 0);
}

#[test]
fn stranger_cannot_touch_my_counter() {
    let mut env = Env::new();
    let owner = env.user.insecure_clone();
    let my_counter = counter_pda(&owner.pubkey());
    env.send(initialize(&owner.pubkey()), &owner).unwrap();

    let stranger = Keypair::new();
    env.svm.airdrop(&stranger.pubkey(), 1_000_000_000).unwrap();
    let err = env.send(update(instruction::Increment {}, &stranger.pubkey(), my_counter), &stranger).unwrap_err();
    // Сиды ["counter", stranger] дают другой адрес — Anchor отклоняет: ConstraintSeeds = 2006.
    assert!(err.contains("Custom(2006)"), "{err}");
    assert_eq!(env.counter(&owner.pubkey()).count, 0);
}

#[test]
fn cannot_initialize_twice() {
    let mut env = Env::new();
    let user = env.user.insecure_clone();
    env.send(initialize(&user.pubkey()), &user).unwrap();
    let err = env.send(initialize(&user.pubkey()), &user).unwrap_err();
    assert!(err.contains("already in use"), "{err}");
}
