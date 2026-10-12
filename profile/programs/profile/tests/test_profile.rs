//! Тесты программы в LiteSVM на том же `profile.so`, что уходит в Devnet (сначала `cargo build-sbf`).
//!
//! Anchor собран на solana-address 1.x, LiteSVM — на 2.x: это те же 32 байта, но разные
//! типы Rust, поэтому на границе адреса перекладываются через `to_bytes()` (`addr`, `key`).
use anchor_lang::{AccountDeserialize, InstructionData, ToAccountMetas};
use litesvm::LiteSVM;
use profile::{accounts, instruction, Profile, UsernameRecord};
use solana_keypair::Keypair;
use solana_message::{AccountMeta, Address, Instruction};
use solana_signer::Signer;
use solana_transaction::Transaction;

type AnchorPubkey = anchor_lang::prelude::Pubkey;
const SOL: u64 = 1_000_000_000;

fn addr(key: &AnchorPubkey) -> Address {
    Address::new_from_array(key.to_bytes())
}

fn key(address: &Address) -> AnchorPubkey {
    AnchorPubkey::new_from_array(address.to_bytes())
}

fn profile_pda(owner: &Address) -> AnchorPubkey {
    AnchorPubkey::find_program_address(&[b"profile", key(owner).as_ref()], &profile::ID).0
}

fn username_pda(username: &str) -> AnchorPubkey {
    AnchorPubkey::find_program_address(&[b"username", username.as_bytes()], &profile::ID).0
}

fn ix(data: impl InstructionData, metas: impl ToAccountMetas) -> Instruction {
    Instruction {
        program_id: addr(&profile::ID),
        accounts: metas
            .to_account_metas(None)
            .into_iter()
            .map(|m| AccountMeta { pubkey: addr(&m.pubkey), is_signer: m.is_signer, is_writable: m.is_writable })
            .collect(),
        data: data.data(),
    }
}

fn create(owner: &Address, username: &str, bio: &str, url: &str) -> Instruction {
    ix(
        instruction::CreateProfile { username: username.into(), bio: bio.into(), url: url.into() },
        accounts::CreateProfile {
            owner: key(owner),
            profile: profile_pda(owner),
            username_record: username_pda(username),
            system_program: anchor_lang::system_program::ID,
        },
    )
}

fn update(owner: &Address, profile: AnchorPubkey, bio: &str, url: &str) -> Instruction {
    ix(
        instruction::UpdateProfile { bio: bio.into(), url: url.into() },
        accounts::UpdateProfile { owner: key(owner), profile },
    )
}

fn close(owner: &Address, username: &str) -> Instruction {
    ix(
        instruction::CloseProfile {},
        accounts::CloseProfile {
            owner: key(owner),
            profile: profile_pda(owner),
            username_record: username_pda(username),
        },
    )
}

struct Env {
    svm: LiteSVM,
}

impl Env {
    fn new() -> Self {
        let mut svm = LiteSVM::new();
        let so = concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/deploy/profile.so");
        svm.add_program_from_file(addr(&profile::ID), so).expect("сначала cargo build-sbf");
        Self { svm }
    }

    fn user(&mut self) -> Keypair {
        let user = Keypair::new();
        self.svm.airdrop(&user.pubkey(), SOL).unwrap();
        user
    }

    fn send(&mut self, ix: Instruction, signer: &Keypair) -> Result<Vec<String>, String> {
        self.svm.expire_blockhash();
        let tx = Transaction::new_signed_with_payer(&[ix], Some(&signer.pubkey()), &[signer], self.svm.latest_blockhash());
        self.svm
            .send_transaction(tx)
            .map(|meta| meta.logs)
            .map_err(|failed| format!("{:?}\n{}", failed.err, failed.meta.logs.join("\n")))
    }

    fn read<T: AccountDeserialize>(&self, address: AnchorPubkey) -> Option<T> {
        let account = self.svm.get_account(&addr(&address))?;
        if account.lamports == 0 {
            return None;
        }
        assert_eq!(key(&account.owner), profile::ID, "владелец аккаунта — программа");
        Some(T::try_deserialize(&mut account.data.as_slice()).unwrap())
    }
}

#[test]
fn create_profile_and_find_it_by_wallet_and_by_name() {
    let mut env = Env::new();
    let timur = env.user();
    let logs = env.send(create(&timur.pubkey(), "timur", "Изучаю Solana", "https://solana.com"), &timur).unwrap();
    assert!(logs.iter().any(|l| l.contains("Profile @timur created")), "{logs:#?}");

    // по кошельку
    let profile: Profile = env.read(profile_pda(&timur.pubkey())).unwrap();
    assert_eq!(profile.owner, key(&timur.pubkey()));
    assert_eq!(profile.username, "timur");
    assert_eq!(profile.bio, "Изучаю Solana");
    assert_eq!(profile.url, "https://solana.com");
    assert_eq!(profile.created_at, profile.updated_at);

    // по имени: запись реестра → владелец → его профиль
    let record: UsernameRecord = env.read(username_pda("timur")).unwrap();
    assert_eq!(record.owner, key(&timur.pubkey()));
    assert_eq!(profile_pda(&addr(&record.owner)), profile_pda(&timur.pubkey()));

    // размер профиля — максимальный, посчитан InitSpace: 8 + 32 + (4+32) + (4+200) + (4+100) + 8 + 8 + 1
    let raw = env.svm.get_account(&addr(&profile_pda(&timur.pubkey()))).unwrap().data;
    assert_eq!(raw.len(), 401);
}

#[test]
fn owner_updates_bio_and_url() {
    let mut env = Env::new();
    let timur = env.user();
    env.send(create(&timur.pubkey(), "timur", "старое", ""), &timur).unwrap();
    env.send(update(&timur.pubkey(), profile_pda(&timur.pubkey()), "новое описание", "https://solana.com/docs"), &timur)
        .unwrap();

    let profile: Profile = env.read(profile_pda(&timur.pubkey())).unwrap();
    assert_eq!(profile.bio, "новое описание");
    assert_eq!(profile.url, "https://solana.com/docs");
    assert_eq!(profile.username, "timur", "имя не меняется");
    assert!(profile.updated_at >= profile.created_at);
}

#[test]
fn username_is_unique() {
    let mut env = Env::new();
    let timur = env.user();
    let other = env.user();
    env.send(create(&timur.pubkey(), "timur", "", ""), &timur).unwrap();

    let err = env.send(create(&other.pubkey(), "timur", "я тоже timur", ""), &other).unwrap_err();
    assert!(err.contains("already in use"), "{err}");
    assert!(env.read::<Profile>(profile_pda(&other.pubkey())).is_none(), "у второго профиль не создан");
}

#[test]
fn one_profile_per_wallet() {
    let mut env = Env::new();
    let timur = env.user();
    env.send(create(&timur.pubkey(), "timur", "", ""), &timur).unwrap();
    let err = env.send(create(&timur.pubkey(), "timur_2", "", ""), &timur).unwrap_err();
    assert!(err.contains("already in use"), "{err}");
}

#[test]
fn bad_input_is_rejected() {
    let mut env = Env::new();
    let user = env.user();
    // Ошибки программы в Anchor нумеруются с 6000: InvalidUsername, BioTooLong, UrlTooLong.
    for name in ["Timur", "ti", "timur medikhanov", "тимур"] {
        let err = env.send(create(&user.pubkey(), name, "", ""), &user).unwrap_err();
        assert!(err.contains("Custom(6000)"), "{name}: {err}");
    }
    let err = env.send(create(&user.pubkey(), "timur", &"a".repeat(201), ""), &user).unwrap_err();
    assert!(err.contains("Custom(6001)"), "{err}");
    let err = env.send(create(&user.pubkey(), "timur", "", &"a".repeat(101)), &user).unwrap_err();
    assert!(err.contains("Custom(6002)"), "{err}");

    // ровно на границе — можно
    env.send(create(&user.pubkey(), &"a".repeat(32), &"b".repeat(200), &"c".repeat(100)), &user).unwrap();
}

#[test]
fn stranger_cannot_edit_my_profile() {
    let mut env = Env::new();
    let timur = env.user();
    let stranger = env.user();
    env.send(create(&timur.pubkey(), "timur", "моё", ""), &timur).unwrap();

    // Сиды ["profile", stranger] дают другой адрес — Anchor отклоняет: ConstraintSeeds = 2006.
    let err = env
        .send(update(&stranger.pubkey(), profile_pda(&timur.pubkey()), "взломано", ""), &stranger)
        .unwrap_err();
    assert!(err.contains("Custom(2006)"), "{err}");
    assert_eq!(env.read::<Profile>(profile_pda(&timur.pubkey())).unwrap().bio, "моё");
}

#[test]
fn close_returns_rent_and_frees_the_name() {
    let mut env = Env::new();
    let timur = env.user();
    env.send(create(&timur.pubkey(), "timur", "", ""), &timur).unwrap();
    let before = env.svm.get_balance(&timur.pubkey()).unwrap();

    env.send(close(&timur.pubkey(), "timur"), &timur).unwrap();
    assert!(env.read::<Profile>(profile_pda(&timur.pubkey())).is_none());
    assert!(env.read::<UsernameRecord>(username_pda("timur")).is_none());
    // вернулся rent обоих аккаунтов (минус комиссия 5000 лампортов)
    assert!(env.svm.get_balance(&timur.pubkey()).unwrap() > before + 3_000_000);

    // имя свободно — его может занять другой
    let other = env.user();
    env.send(create(&other.pubkey(), "timur", "", ""), &other).unwrap();
    let record: UsernameRecord = env.read(username_pda("timur")).unwrap();
    assert_eq!(record.owner, key(&other.pubkey()));
}
