use anchor_lang::solana_program::{program_option::COption, program_pack::Pack};
use anchor_lang::{AccountDeserialize, InstructionData, ToAccountMetas};
use anchor_spl::token_interface::spl_token_2022::{
    extension::{transfer_fee::TransferFeeAmount, BaseStateWithExtensions, StateWithExtensions},
    instruction,
    state::{Account as TokenAccount, Mint},
};
use litesvm::LiteSVM;
use solana_account::Account;
use solana_keypair::Keypair;
use solana_message::{AccountMeta, Address, Instruction};
use solana_signer::Signer;
use solana_transaction::Transaction;
use vault::{accounts, instruction as vault_ix, UserPosition, VaultConfig};

type Pubkey = anchor_lang::prelude::Pubkey;

fn key(address: &Address) -> Pubkey {
    Pubkey::new_from_array(address.to_bytes())
}

fn addr(pubkey: &Pubkey) -> Address {
    Address::new_from_array(pubkey.to_bytes())
}

fn pda(seeds: &[&[u8]]) -> Pubkey {
    Pubkey::find_program_address(seeds, &vault::ID).0
}

fn convert(ix: anchor_lang::solana_program::instruction::Instruction) -> Instruction {
    Instruction {
        program_id: addr(&ix.program_id),
        accounts: ix
            .accounts
            .into_iter()
            .map(|m| AccountMeta {
                pubkey: addr(&m.pubkey),
                is_signer: m.is_signer,
                is_writable: m.is_writable,
            })
            .collect(),
        data: ix.data,
    }
}

fn vault_instruction(data: impl InstructionData, metas: impl ToAccountMetas) -> Instruction {
    convert(anchor_lang::solana_program::instruction::Instruction {
        program_id: vault::ID,
        accounts: metas.to_account_metas(None),
        data: data.data(),
    })
}

struct Env {
    svm: LiteSVM,
    admin: Keypair,
    asset_mint: Pubkey,
    token_program: Pubkey,
    vault: Pubkey,
    share_mint: Pubkey,
    token_vault: Pubkey,
}

impl Env {
    fn new(token_program: Pubkey) -> Self {
        let mut svm = LiteSVM::new();
        let so = concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/deploy/vault.so");
        svm.add_program_from_file(addr(&vault::ID), so)
            .expect("сначала cargo build-sbf");
        let admin = Keypair::new();
        svm.airdrop(&admin.pubkey(), 10_000_000_000).unwrap();
        let asset_mint = key(&Keypair::new().pubkey());
        let mut data = vec![0; Mint::LEN];
        Mint::pack(
            Mint {
                mint_authority: COption::Some(key(&admin.pubkey())),
                supply: 0,
                decimals: 6,
                is_initialized: true,
                freeze_authority: COption::None,
            },
            &mut data,
        )
        .unwrap();
        svm.set_account(
            addr(&asset_mint),
            Account {
                lamports: svm.minimum_balance_for_rent_exemption(data.len()),
                data,
                owner: addr(&token_program),
                executable: false,
                rent_epoch: 0,
            },
        )
        .unwrap();
        let vault = pda(&[b"vault", admin.pubkey().as_ref(), asset_mint.as_ref()]);
        let share_mint = pda(&[b"share_mint", vault.as_ref()]);
        let token_vault = pda(&[b"token_vault", vault.as_ref()]);
        Self {
            svm,
            admin,
            asset_mint,
            token_program,
            vault,
            share_mint,
            token_vault,
        }
    }

    fn create(&self) -> Instruction {
        vault_instruction(
            vault_ix::CreateVault {},
            accounts::CreateVault {
                admin: key(&self.admin.pubkey()),
                asset_mint: self.asset_mint,
                vault: self.vault,
                share_mint: self.share_mint,
                token_vault: self.token_vault,
                token_program: self.token_program,
                system_program: anchor_lang::system_program::ID,
            },
        )
    }

    fn open(&self, user: &Address, position: Pubkey) -> Instruction {
        vault_instruction(
            vault_ix::OpenPosition {},
            accounts::OpenPosition {
                user: key(user),
                vault: self.vault,
                position,
                system_program: anchor_lang::system_program::ID,
            },
        )
    }

    fn position(&self, user: &Address) -> Pubkey {
        pda(&[b"position", self.vault.as_ref(), user.as_ref()])
    }

    fn send(&mut self, ix: Instruction, signer: &Keypair) -> Result<(), String> {
        self.svm.expire_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&signer.pubkey()),
            &[signer],
            self.svm.latest_blockhash(),
        );
        self.svm
            .send_transaction(tx)
            .map(|_| ())
            .map_err(|failed| format!("{:?}\n{}", failed.err, failed.meta.logs.join("\n")))
    }

    fn initialize(&mut self) {
        let admin = self.admin.insecure_clone();
        self.send(self.create(), &admin).unwrap();
    }
}

#[test]
fn creates_empty_vault_with_program_authorities_for_both_token_programs() {
    for token_program in [anchor_spl::token::ID, anchor_spl::token_2022::ID] {
        let mut env = Env::new(token_program);
        env.initialize();
        let config_account = env.svm.get_account(&addr(&env.vault)).unwrap();
        assert_eq!(config_account.owner, addr(&vault::ID));
        assert_eq!(config_account.data.len(), 145);
        let config = VaultConfig::try_deserialize(&mut config_account.data.as_slice()).unwrap();
        assert_eq!(config.admin, key(&env.admin.pubkey()));
        assert_eq!(config.asset_mint, env.asset_mint);
        assert_eq!(config.share_mint, env.share_mint);
        assert_eq!(config.token_vault, env.token_vault);
        assert_eq!(config.total_deposited, 0);

        let mint_account = env.svm.get_account(&addr(&env.share_mint)).unwrap();
        assert_eq!(mint_account.owner, addr(&token_program));
        let mint = Mint::unpack(&mint_account.data).unwrap();
        assert_eq!(mint.mint_authority, COption::Some(env.vault));
        assert_eq!(mint.freeze_authority, COption::None);
        assert_eq!(mint.supply, 0);
        assert_eq!(mint.decimals, 6);
        let storage_account = env.svm.get_account(&addr(&env.token_vault)).unwrap();
        assert_eq!(storage_account.owner, addr(&token_program));
        let storage = TokenAccount::unpack(&storage_account.data).unwrap();
        assert_eq!(storage.owner, env.vault);
        assert_eq!(storage.mint, env.asset_mint);
        assert_eq!(storage.amount, 0);
        assert_eq!(storage.delegate, COption::None);
        assert_eq!(storage.close_authority, COption::None);
    }
}

#[test]
fn tmr_mint_extensions_create_compatible_empty_storage() {
    let mut env = Env::new(anchor_spl::token_2022::ID);
    let data = include_bytes!("fixtures/tmr-mint.bin").to_vec();
    env.svm
        .set_account(
            addr(&env.asset_mint),
            Account {
                lamports: env.svm.minimum_balance_for_rent_exemption(data.len()),
                data,
                owner: addr(&env.token_program),
                executable: false,
                rent_epoch: 0,
            },
        )
        .unwrap();
    env.initialize();
    let storage_account = env.svm.get_account(&addr(&env.token_vault)).unwrap();
    let storage = StateWithExtensions::<TokenAccount>::unpack(&storage_account.data).unwrap();
    assert_eq!(storage.base.mint, env.asset_mint);
    assert_eq!(storage.base.owner, env.vault);
    assert_eq!(storage.base.amount, 0);
    let fee = storage.get_extension::<TransferFeeAmount>().unwrap();
    assert_eq!(u64::from(fee.withheld_amount), 0);
    let share_account = env.svm.get_account(&addr(&env.share_mint)).unwrap();
    let share = StateWithExtensions::<Mint>::unpack(&share_account.data).unwrap();
    assert_eq!(share.base.mint_authority, COption::Some(env.vault));
    assert_eq!(share.base.freeze_authority, COption::None);
    assert_eq!(share.base.supply, 0);
    assert!(share.get_extension_types().unwrap().is_empty());
}

#[test]
fn each_user_opens_a_deterministic_empty_position() {
    let mut env = Env::new(anchor_spl::token_2022::ID);
    env.initialize();
    let users = [env.admin.insecure_clone(), Keypair::new()];
    for user in &users {
        env.svm.airdrop(&user.pubkey(), 1_000_000_000).unwrap();
        let position = env.position(&user.pubkey());
        env.send(env.open(&user.pubkey(), position), user).unwrap();
        let account = env.svm.get_account(&addr(&position)).unwrap();
        assert_eq!(account.owner, addr(&vault::ID));
        assert_eq!(account.data.len(), 81);
        let state = UserPosition::try_deserialize(&mut account.data.as_slice()).unwrap();
        assert_eq!(state.owner, key(&user.pubkey()));
        assert_eq!(state.vault, env.vault);
        assert_eq!(state.deposited_amount, 0);
    }
    assert_ne!(
        env.position(&users[0].pubkey()),
        env.position(&users[1].pubkey())
    );
}

#[test]
fn duplicate_vault_and_position_are_rejected() {
    let mut env = Env::new(anchor_spl::token::ID);
    env.initialize();
    let admin = env.admin.insecure_clone();
    assert!(env
        .send(env.create(), &admin)
        .unwrap_err()
        .contains("already in use"));
    let position = env.position(&admin.pubkey());
    env.send(env.open(&admin.pubkey(), position), &admin)
        .unwrap();
    assert!(env
        .send(env.open(&admin.pubkey(), position), &admin)
        .unwrap_err()
        .contains("already in use"));
    let state = env.svm.get_account(&addr(&position)).unwrap();
    assert_eq!(
        UserPosition::try_deserialize(&mut state.data.as_slice())
            .unwrap()
            .deposited_amount,
        0
    );
}

#[test]
fn a_user_cannot_open_someone_elses_position() {
    let mut env = Env::new(anchor_spl::token::ID);
    env.initialize();
    let stranger = Keypair::new();
    env.svm.airdrop(&stranger.pubkey(), 1_000_000_000).unwrap();
    let victim_position = env.position(&env.admin.pubkey());
    let error = env
        .send(env.open(&stranger.pubkey(), victim_position), &stranger)
        .unwrap_err();
    assert!(error.contains("ConstraintSeeds"), "{error}");
    assert!(env.svm.get_account(&addr(&victim_position)).is_none());
}

#[test]
fn asset_mint_must_belong_to_the_selected_token_program() {
    let mut env = Env::new(anchor_spl::token_2022::ID);
    env.token_program = anchor_spl::token::ID;
    let admin = env.admin.insecure_clone();
    let error = env.send(env.create(), &admin).unwrap_err();
    assert!(error.contains("IncorrectProgramId"), "{error}");
    assert!(env.svm.get_account(&addr(&env.vault)).is_none());
    assert!(env.svm.get_account(&addr(&env.share_mint)).is_none());
    assert!(env.svm.get_account(&addr(&env.token_vault)).is_none());
}

#[test]
fn the_admin_cannot_mint_shares_or_change_their_authority() {
    let mut env = Env::new(anchor_spl::token::ID);
    env.initialize();
    let admin = env.admin.insecure_clone();
    let destination = key(&Keypair::new().pubkey());
    let mut data = vec![0; TokenAccount::LEN];
    TokenAccount::pack(
        TokenAccount {
            mint: env.share_mint,
            owner: key(&admin.pubkey()),
            amount: 0,
            delegate: COption::None,
            state: anchor_spl::token_interface::spl_token_2022::state::AccountState::Initialized,
            is_native: COption::None,
            delegated_amount: 0,
            close_authority: COption::None,
        },
        &mut data,
    )
    .unwrap();
    env.svm
        .set_account(
            addr(&destination),
            Account {
                lamports: env.svm.minimum_balance_for_rent_exemption(data.len()),
                data,
                owner: addr(&env.token_program),
                executable: false,
                rent_epoch: 0,
            },
        )
        .unwrap();
    let mint_to = instruction::mint_to(
        &env.token_program,
        &env.share_mint,
        &destination,
        &key(&admin.pubkey()),
        &[],
        1,
    )
    .unwrap();
    let error = env.send(convert(mint_to), &admin).unwrap_err();
    assert!(
        error.contains("OwnerMismatch") || error.contains("Custom(4)"),
        "{error}"
    );
    let set_authority = instruction::set_authority(
        &env.token_program,
        &env.share_mint,
        Some(&key(&admin.pubkey())),
        instruction::AuthorityType::MintTokens,
        &key(&admin.pubkey()),
        &[],
    )
    .unwrap();
    assert!(env.send(convert(set_authority), &admin).is_err());
    let mint_account = env.svm.get_account(&addr(&env.share_mint)).unwrap();
    assert_eq!(
        Mint::unpack(&mint_account.data).unwrap().mint_authority,
        COption::Some(env.vault)
    );
}

#[test]
fn the_admin_cannot_take_control_of_or_close_the_token_storage() {
    let mut env = Env::new(anchor_spl::token::ID);
    env.initialize();
    let admin = env.admin.insecure_clone();
    let set_authority = instruction::set_authority(
        &env.token_program,
        &env.token_vault,
        Some(&key(&admin.pubkey())),
        instruction::AuthorityType::AccountOwner,
        &key(&admin.pubkey()),
        &[],
    )
    .unwrap();
    assert!(env.send(convert(set_authority), &admin).is_err());
    let close = instruction::close_account(
        &env.token_program,
        &env.token_vault,
        &key(&admin.pubkey()),
        &key(&admin.pubkey()),
        &[],
    )
    .unwrap();
    assert!(env.send(convert(close), &admin).is_err());
    assert!(env.svm.get_account(&addr(&env.token_vault)).is_some());
}
