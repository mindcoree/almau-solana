use anchor_lang::solana_program::{program_option::COption, program_pack::Pack};
use anchor_lang::{AccountDeserialize, AccountSerialize, InstructionData, ToAccountMetas};
use anchor_spl::token_interface::spl_token_2022::{
    extension::{
        transfer_fee::{TransferFeeAmount, TransferFeeConfig},
        BaseStateWithExtensions, BaseStateWithExtensionsMut, ExtensionType, StateWithExtensions,
        StateWithExtensionsMut,
    },
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

    fn token_account(&mut self, mint: Pubkey, owner: Pubkey, amount: u64) -> Pubkey {
        let mint_account = self.svm.get_account(&addr(&mint)).unwrap();
        let with_fee = StateWithExtensions::<Mint>::unpack(&mint_account.data)
            .unwrap()
            .get_extension::<TransferFeeConfig>()
            .is_ok();
        let len = if with_fee {
            ExtensionType::try_calculate_account_len::<TokenAccount>(&[
                ExtensionType::TransferFeeAmount,
            ])
            .unwrap()
        } else {
            TokenAccount::LEN
        };
        let mut data = vec![0; len];
        let state = TokenAccount {
            mint,
            owner,
            amount,
            state: anchor_spl::token_interface::spl_token_2022::state::AccountState::Initialized,
            ..TokenAccount::default()
        };
        if with_fee {
            let mut account =
                StateWithExtensionsMut::<TokenAccount>::unpack_uninitialized(&mut data).unwrap();
            account.init_extension::<TransferFeeAmount>(true).unwrap();
            account.base = state;
            account.pack_base();
            account.init_account_type().unwrap();
        } else {
            TokenAccount::pack(state, &mut data).unwrap();
        }
        let address = key(&Keypair::new().pubkey());
        self.svm
            .set_account(
                addr(&address),
                Account {
                    lamports: self.svm.minimum_balance_for_rent_exemption(len),
                    data,
                    owner: addr(&self.token_program),
                    executable: false,
                    rent_epoch: 0,
                },
            )
            .unwrap();
        address
    }

    fn user(&mut self, signer: &Keypair, amount: u64) -> (Pubkey, Pubkey) {
        self.svm.airdrop(&signer.pubkey(), 1_000_000_000).unwrap();
        self.send(
            self.open(&signer.pubkey(), self.position(&signer.pubkey())),
            signer,
        )
        .unwrap();
        let source = self.token_account(self.asset_mint, key(&signer.pubkey()), amount);
        let shares = self.token_account(self.share_mint, key(&signer.pubkey()), 0);
        (source, shares)
    }

    fn deposit(&self, user: &Address, source: Pubkey, shares: Pubkey, amount: u64) -> Instruction {
        vault_instruction(
            vault_ix::Deposit { amount },
            accounts::VaultUser {
                user: key(user),
                vault: self.vault,
                position: self.position(user),
                asset_mint: self.asset_mint,
                share_mint: self.share_mint,
                token_vault: self.token_vault,
                user_token_account: source,
                user_share_account: shares,
                token_program: self.token_program,
                market: pda(&[b"market", self.vault.as_ref()]),
            },
        )
    }

    fn balance(&self, address: Pubkey) -> u64 {
        let account = self.svm.get_account(&addr(&address)).unwrap();
        StateWithExtensions::<TokenAccount>::unpack(&account.data)
            .unwrap()
            .base
            .amount
    }

    fn withdraw(
        &self,
        user: &Address,
        destination: Pubkey,
        shares: Pubkey,
        amount: u64,
    ) -> Instruction {
        let mut ix = self.deposit(user, destination, shares, amount);
        ix.data = vault_ix::Withdraw { shares: amount }.data();
        ix
    }

    fn supply(&self) -> u64 {
        let account = self.svm.get_account(&addr(&self.share_mint)).unwrap();
        StateWithExtensions::<Mint>::unpack(&account.data)
            .unwrap()
            .base
            .supply
    }

    fn config(&self) -> VaultConfig {
        let account = self.svm.get_account(&addr(&self.vault)).unwrap();
        VaultConfig::try_deserialize(&mut account.data.as_slice()).unwrap()
    }

    fn deposited(&self, user: &Address) -> u64 {
        let account = self.svm.get_account(&addr(&self.position(user))).unwrap();
        UserPosition::try_deserialize(&mut account.data.as_slice())
            .unwrap()
            .deposited_amount
    }

    fn snapshot(&self, addresses: &[Pubkey]) -> Vec<Vec<u8>> {
        addresses
            .iter()
            .map(|address| self.svm.get_account(&addr(address)).unwrap().data)
            .collect()
    }
}

struct ClmmEnv {
    env: Env,
    fixture: serde_json::Value,
    market: Pubkey,
    quote_vault: Pubkey,
}

impl ClmmEnv {
    fn new() -> Self {
        let directory = concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/fixtures/");
        let fixture: serde_json::Value = serde_json::from_slice(
            &std::fs::read(format!("{directory}clmm.json"))
                .expect("сначала npm run swap:fixture в vault/client"),
        )
        .unwrap();
        let mut env = Env::new(anchor_spl::token::ID);
        for account in fixture["accounts"].as_object().unwrap().values() {
            let address: Pubkey = account["address"].as_str().unwrap().parse().unwrap();
            let owner: Pubkey = account["owner"].as_str().unwrap().parse().unwrap();
            env.svm
                .set_account(
                    addr(&address),
                    Account {
                        lamports: account["lamports"].as_u64().unwrap(),
                        owner: addr(&owner),
                        executable: account["executable"].as_bool().unwrap(),
                        rent_epoch: 0,
                        data: account["data"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|byte| byte.as_u64().unwrap() as u8)
                            .collect(),
                    },
                )
                .unwrap();
        }
        env.svm
            .add_program_from_file(
                addr(&vault::raydium_clmm::ID),
                format!("{directory}raydium-clmm.so"),
            )
            .unwrap();
        let mut clock = env
            .svm
            .get_sysvar::<anchor_lang::solana_program::clock::Clock>();
        clock.unix_timestamp = fixture["blockTimestamp"].as_i64().unwrap();
        env.svm.set_sysvar(&clock);
        env.asset_mint = fixture["accounts"]["assetMint"]["address"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        env.vault = pda(&[
            b"vault",
            env.admin.pubkey().as_ref(),
            env.asset_mint.as_ref(),
        ]);
        env.share_mint = pda(&[b"share_mint", env.vault.as_ref()]);
        env.token_vault = pda(&[b"token_vault", env.vault.as_ref()]);
        let market = pda(&[b"market", env.vault.as_ref()]);
        let quote_vault = pda(&[b"quote_vault", env.vault.as_ref()]);
        env.initialize();
        Self {
            env,
            fixture,
            market,
            quote_vault,
        }
    }

    fn key(&self, name: &str) -> Pubkey {
        self.fixture["accounts"][name]["address"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap()
    }

    fn initialize_market(&self, admin: &Address, quote_mint: Pubkey) -> Instruction {
        vault_instruction(
            vault_ix::InitializeMarket {},
            accounts::InitializeMarket {
                admin: key(admin),
                vault: self.env.vault,
                market: self.market,
                asset_mint: self.env.asset_mint,
                quote_mint,
                share_mint: self.env.share_mint,
                quote_vault: self.quote_vault,
                pool_state: self.key("poolState"),
                clmm_program: vault::raydium_clmm::ID,
                asset_token_program: self.env.token_program,
                quote_token_program: self.env.token_program,
                system_program: anchor_lang::system_program::ID,
            },
        )
    }

    fn enable(&mut self) {
        let admin = self.env.admin.insecure_clone();
        self.env
            .send(
                self.initialize_market(&admin.pubkey(), self.key("quoteMint")),
                &admin,
            )
            .unwrap();
    }

    fn fund(&mut self) {
        let admin = self.env.admin.insecure_clone();
        let source = self
            .env
            .token_account(self.env.asset_mint, key(&admin.pubkey()), 100_000_000);
        let transfer = instruction::transfer_checked(
            &self.env.token_program,
            &source,
            &self.env.asset_mint,
            &self.env.token_vault,
            &key(&admin.pubkey()),
            &[],
            100_000_000,
            6,
        )
        .unwrap();
        self.env.send(convert(transfer), &admin).unwrap();
    }

    fn swap(&self, admin: &Address, amount_in: u64, minimum_out: u64) -> Instruction {
        let mut ix = vault_instruction(
            vault_ix::Swap {
                amount_in,
                minimum_out,
            },
            accounts::Swap {
                admin: key(admin),
                vault: self.env.vault,
                market: self.market,
                asset_mint: self.env.asset_mint,
                quote_mint: self.key("quoteMint"),
                token_vault: self.env.token_vault,
                quote_vault: self.quote_vault,
                pool_state: self.key("poolState"),
                amm_config: self.key("ammConfig"),
                input_vault: self.key("inputVault"),
                output_vault: self.key("outputVault"),
                observation_state: self.key("observation"),
                clmm_program: vault::raydium_clmm::ID,
                token_program: anchor_spl::token::ID,
                token_program_2022: anchor_spl::token_2022::ID,
                memo_program: "MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr"
                    .parse()
                    .unwrap(),
            },
        );
        for address in self.fixture["quote"]["remainingAccounts"]
            .as_array()
            .unwrap()
        {
            let pubkey: Pubkey = address.as_str().unwrap().parse().unwrap();
            ix.accounts.push(AccountMeta::new(addr(&pubkey), false));
        }
        ix
    }

    fn watched(&self) -> Vec<Pubkey> {
        let mut accounts = vec![
            self.env.vault,
            self.env.share_mint,
            self.env.token_vault,
            self.quote_vault,
            self.market,
        ];
        accounts.extend(
            self.fixture["accounts"]
                .as_object()
                .unwrap()
                .values()
                .map(|account| {
                    account["address"]
                        .as_str()
                        .unwrap()
                        .parse::<Pubkey>()
                        .unwrap()
                }),
        );
        accounts
    }
}

#[test]
fn initializes_second_storage_with_vault_authority_and_no_account_migration() {
    let mut c = ClmmEnv::new();
    c.enable();
    let account = c.env.svm.get_account(&addr(&c.market)).unwrap();
    assert_eq!(account.data.len(), 137);
    let market = vault::MarketConfig::try_deserialize(&mut account.data.as_slice()).unwrap();
    assert_eq!(market.vault, c.env.vault);
    assert_eq!(market.quote_mint, c.key("quoteMint"));
    assert_eq!(market.quote_vault, c.quote_vault);
    assert_eq!(market.pool_state, c.key("poolState"));
    let storage = c.env.svm.get_account(&addr(&c.quote_vault)).unwrap();
    let storage = TokenAccount::unpack(&storage.data).unwrap();
    assert_eq!(storage.owner, c.env.vault);
    assert_eq!(storage.mint, c.key("quoteMint"));
    assert_eq!(storage.amount, 0);
    assert_eq!(
        c.env
            .svm
            .get_account(&addr(&c.env.vault))
            .unwrap()
            .data
            .len(),
        145
    );
}

#[test]
fn real_raydium_cpi_uses_vault_signature_and_matches_sdk_quote() {
    let mut c = ClmmEnv::new();
    c.enable();
    c.fund();
    let admin = c.env.admin.insecure_clone();
    let expected: u64 = c.fixture["quote"]["expectedOut"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let minimum: u64 = c.fixture["quote"]["minimumOut"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    c.env
        .send(c.swap(&admin.pubkey(), 10_000_000, minimum), &admin)
        .unwrap();
    assert_eq!(c.env.balance(c.env.token_vault), 90_000_000);
    assert_eq!(c.env.balance(c.quote_vault), expected);
    assert_eq!(c.env.config().total_deposited, 90_000_000);
    assert_eq!(c.env.supply(), 0);
    let account = c.env.svm.get_account(&addr(&c.key("poolState"))).unwrap();
    let pool =
        vault::raydium_clmm::accounts::PoolState::try_deserialize(&mut account.data.as_slice())
            .unwrap();
    let sqrt_price = pool.sqrt_price_x64;
    assert_eq!(
        sqrt_price.to_string(),
        c.fixture["quote"]["sqrtPriceAfterX64"].as_str().unwrap()
    );
}

#[test]
fn raydium_slippage_failure_rolls_back_pool_vaults_ticks_and_counters() {
    let mut c = ClmmEnv::new();
    c.enable();
    c.fund();
    let admin = c.env.admin.insecure_clone();
    let watched = c.watched();
    let before = c.env.snapshot(&watched);
    let error = c
        .env
        .send(c.swap(&admin.pubkey(), 10_000_000, u64::MAX), &admin)
        .unwrap_err();
    assert!(error.contains("TooLittleOutputReceived"), "{error}");
    assert!(error.contains("Instruction: SwapV2"), "{error}");
    assert_eq!(c.env.snapshot(&watched), before);
}

#[test]
fn only_admin_can_enable_a_market_or_swap() {
    let mut c = ClmmEnv::new();
    let stranger = Keypair::new();
    c.env
        .svm
        .airdrop(&stranger.pubkey(), 1_000_000_000)
        .unwrap();
    let error = c
        .env
        .send(
            c.initialize_market(&stranger.pubkey(), c.key("quoteMint")),
            &stranger,
        )
        .unwrap_err();
    assert!(error.contains("ConstraintHasOne"), "{error}");
    assert!(c.env.svm.get_account(&addr(&c.market)).is_none());
    assert!(c.env.svm.get_account(&addr(&c.quote_vault)).is_none());
    c.enable();
    c.fund();
    let watched = c.watched();
    let before = c.env.snapshot(&watched);
    let error = c
        .env
        .send(c.swap(&stranger.pubkey(), 10_000_000, 1), &stranger)
        .unwrap_err();
    assert!(error.contains("ConstraintHasOne"), "{error}");
    assert_eq!(c.env.snapshot(&watched), before);
}

#[test]
fn market_must_match_the_actual_pool_mints() {
    let mut c = ClmmEnv::new();
    let admin = c.env.admin.insecure_clone();
    let wrong_mint = key(&Keypair::new().pubkey());
    let original = c.env.svm.get_account(&addr(&c.key("quoteMint"))).unwrap();
    c.env.svm.set_account(addr(&wrong_mint), original).unwrap();
    let error = c
        .env
        .send(c.initialize_market(&admin.pubkey(), wrong_mint), &admin)
        .unwrap_err();
    assert!(error.contains("InvalidPool"), "{error}");
    assert!(c.env.svm.get_account(&addr(&c.market)).is_none());
    assert!(c.env.svm.get_account(&addr(&c.quote_vault)).is_none());
}

#[test]
fn outstanding_shares_prevent_switching_to_trading_mode() {
    let mut c = ClmmEnv::new();
    let admin = c.env.admin.insecure_clone();
    let (source, shares) = c.env.user(&admin, 20_000_000);
    c.env
        .send(
            c.env.deposit(&admin.pubkey(), source, shares, 10_000_000),
            &admin,
        )
        .unwrap();
    let error = c
        .env
        .send(
            c.initialize_market(&admin.pubkey(), c.key("quoteMint")),
            &admin,
        )
        .unwrap_err();
    assert!(error.contains("OutstandingShares"), "{error}");
    assert!(c.env.svm.get_account(&addr(&c.market)).is_none());
    assert_eq!(c.env.supply(), 10_000_000);
    assert_eq!(c.env.balance(shares), 10_000_000);
}

#[test]
fn trading_mode_blocks_single_asset_shares_and_cannot_be_hidden_by_a_fake_marker() {
    let mut c = ClmmEnv::new();
    c.enable();
    let admin = c.env.admin.insecure_clone();
    let (source, shares) = c.env.user(&admin, 20_000_000);
    let watched = [
        c.env.vault,
        c.env.token_vault,
        c.env.share_mint,
        c.market,
        source,
        shares,
    ];
    let before = c.env.snapshot(&watched);
    for ix in [
        c.env.deposit(&admin.pubkey(), source, shares, 10_000_000),
        c.env.withdraw(&admin.pubkey(), source, shares, 1),
    ] {
        let error = c.env.send(ix, &admin).unwrap_err();
        assert!(error.contains("TradingVault"), "{error}");
        assert_eq!(c.env.snapshot(&watched), before);
    }
    let mut fake_marker = c.env.deposit(&admin.pubkey(), source, shares, 10_000_000);
    fake_marker.accounts.last_mut().unwrap().pubkey = Keypair::new().pubkey();
    let error = c.env.send(fake_marker, &admin).unwrap_err();
    assert!(error.contains("ConstraintSeeds"), "{error}");
    assert_eq!(c.env.snapshot(&watched), before);
}

#[test]
fn invalid_swap_amounts_and_substituted_accounts_leave_assets_unchanged() {
    let mut c = ClmmEnv::new();
    c.enable();
    c.fund();
    let admin = c.env.admin.insecure_clone();
    let watched = c.watched();
    let before = c.env.snapshot(&watched);
    for (amount, minimum, expected) in [
        (0, 1, "ZeroAmount"),
        (1, 0, "ZeroMinimumOut"),
        (100_000_001, 1, "InsufficientAssets"),
    ] {
        let error = c
            .env
            .send(c.swap(&admin.pubkey(), amount, minimum), &admin)
            .unwrap_err();
        assert!(error.contains(expected), "{error}");
        assert_eq!(c.env.snapshot(&watched), before);
    }
    let fake_vault = c
        .env
        .token_account(c.env.asset_mint, c.key("poolState"), 100_000_000);
    let mut ix = c.swap(&admin.pubkey(), 10_000_000, 1);
    ix.accounts
        .iter_mut()
        .find(|meta| meta.pubkey == addr(&c.key("inputVault")))
        .unwrap()
        .pubkey = addr(&fake_vault);
    let error = c.env.send(ix, &admin).unwrap_err();
    assert!(error.contains("InvalidPool"), "{error}");
    assert_eq!(c.env.snapshot(&watched), before);
    let mut ix = c.swap(&admin.pubkey(), 10_000_000, 1);
    ix.accounts
        .iter_mut()
        .find(|meta| meta.pubkey == addr(&vault::raydium_clmm::ID))
        .unwrap()
        .pubkey = addr(&anchor_spl::token::ID);
    let error = c.env.send(ix, &admin).unwrap_err();
    assert!(error.contains("InvalidProgramId"), "{error}");
    assert_eq!(c.env.snapshot(&watched), before);
}

#[test]
fn partial_and_full_withdrawals_share_profit_round_down_and_allow_a_new_cycle() {
    for token_program in [anchor_spl::token::ID, anchor_spl::token_2022::ID] {
        let mut env = Env::new(token_program);
        env.initialize();
        let first = env.admin.insecure_clone();
        let second = Keypair::new();
        let (tokens_a, shares_a) = env.user(&first, 12);
        let (tokens_b, shares_b) = env.user(&second, 5);
        env.send(env.deposit(&first.pubkey(), tokens_a, shares_a, 10), &first)
            .unwrap();
        env.send(
            env.deposit(&second.pubkey(), tokens_b, shares_b, 5),
            &second,
        )
        .unwrap();
        env.send(
            convert(
                instruction::transfer_checked(
                    &env.token_program,
                    &tokens_a,
                    &env.asset_mint,
                    &env.token_vault,
                    &key(&first.pubkey()),
                    &[],
                    2,
                    6,
                )
                .unwrap(),
            ),
            &first,
        )
        .unwrap();
        // 4 * 17 / 15 = 4; затем 6 * 13 / 11 = 7; последнему достаётся остаток 6.
        for (user, tokens, shares, amount, payout, remaining) in [
            (&first, tokens_a, shares_a, 4, 4, 13),
            (&first, tokens_a, shares_a, 6, 7, 6),
            (&second, tokens_b, shares_b, 5, 6, 0),
        ] {
            let before = env.balance(tokens);
            env.send(env.withdraw(&user.pubkey(), tokens, shares, amount), user)
                .unwrap();
            assert_eq!(env.balance(tokens) - before, payout);
            assert_eq!(env.balance(env.token_vault), remaining);
            assert_eq!(env.config().total_deposited, remaining);
        }
        assert_eq!(env.supply(), 0);
        assert_eq!(env.balance(shares_a), 0);
        assert_eq!(env.balance(shares_b), 0);
        assert_eq!(env.deposited(&first.pubkey()), 10);
        assert_eq!(env.deposited(&second.pubkey()), 5);
        env.send(env.deposit(&first.pubkey(), tokens_a, shares_a, 3), &first)
            .unwrap();
        assert_eq!(env.supply(), 3);
        assert_eq!(env.balance(env.token_vault), 3);
    }
}

#[test]
fn received_shares_can_be_redeemed_without_a_personal_deposit() {
    for token_program in [anchor_spl::token::ID, anchor_spl::token_2022::ID] {
        let mut env = Env::new(token_program);
        env.initialize();
        let owner = env.admin.insecure_clone();
        let recipient = Keypair::new();
        let (tokens_a, shares_a) = env.user(&owner, 100);
        let (tokens_b, shares_b) = env.user(&recipient, 0);
        env.send(
            env.deposit(&owner.pubkey(), tokens_a, shares_a, 100),
            &owner,
        )
        .unwrap();
        env.send(
            convert(
                instruction::transfer_checked(
                    &env.token_program,
                    &shares_a,
                    &env.share_mint,
                    &shares_b,
                    &key(&owner.pubkey()),
                    &[],
                    40,
                    6,
                )
                .unwrap(),
            ),
            &owner,
        )
        .unwrap();
        env.send(
            env.withdraw(&recipient.pubkey(), tokens_b, shares_b, 40),
            &recipient,
        )
        .unwrap();
        assert_eq!(env.balance(tokens_b), 40);
        assert_eq!(env.balance(shares_b), 0);
        assert_eq!(env.deposited(&recipient.pubkey()), 0);
        assert_eq!(env.balance(shares_a), 60);
        assert_eq!(env.supply(), 60);
        assert_eq!(env.balance(env.token_vault), 60);
        assert_eq!(env.config().total_deposited, 60);
    }
}

#[test]
fn tmr_withdrawals_exclude_withheld_fees_and_roll_back_a_zero_net_payout() {
    let mut env = Env::new(anchor_spl::token_2022::ID);
    let data = include_bytes!("fixtures/tmr-mint.bin").to_vec();
    let mut mint = env.svm.get_account(&addr(&env.asset_mint)).unwrap();
    mint.lamports = env.svm.minimum_balance_for_rent_exemption(data.len());
    mint.data = data;
    env.svm.set_account(addr(&env.asset_mint), mint).unwrap();
    env.initialize();
    let user = env.admin.insecure_clone();
    let (tokens, shares) = env.user(&user, 200_000_000);
    env.send(
        env.deposit(&user.pubkey(), tokens, shares, 100_000_000),
        &user,
    )
    .unwrap();
    env.send(
        env.withdraw(&user.pubkey(), tokens, shares, 10_000_000),
        &user,
    )
    .unwrap();
    assert_eq!(env.balance(tokens), 109_900_000);
    assert_eq!(env.balance(shares), 89_000_000);
    assert_eq!(env.supply(), 89_000_000);
    assert_eq!(env.config().total_deposited, 89_000_000);
    let accounts = [
        env.vault,
        env.position(&user.pubkey()),
        env.token_vault,
        env.share_mint,
        tokens,
        shares,
    ];
    let before = env.snapshot(&accounts);
    let error = env
        .send(env.withdraw(&user.pubkey(), tokens, shares, 1), &user)
        .unwrap_err();
    assert!(error.contains("NothingReceived"), "{error}");
    assert_eq!(env.snapshot(&accounts), before);
    env.send(
        env.withdraw(&user.pubkey(), tokens, shares, 89_000_000),
        &user,
    )
    .unwrap();
    assert_eq!(env.balance(tokens), 198_010_000);
    assert_eq!(env.balance(shares), 0);
    assert_eq!(env.supply(), 0);
    assert_eq!(env.balance(env.token_vault), 0);
    assert_eq!(env.config().total_deposited, 0);
    for (address, expected) in [(env.token_vault, 1_000_000), (tokens, 990_000)] {
        let account = env.svm.get_account(&addr(&address)).unwrap();
        let state = StateWithExtensions::<TokenAccount>::unpack(&account.data).unwrap();
        assert_eq!(
            u64::from(
                state
                    .get_extension::<TransferFeeAmount>()
                    .unwrap()
                    .withheld_amount
            ),
            expected
        );
    }
}

#[test]
fn zero_excess_and_dust_withdrawals_do_not_change_balances() {
    let mut env = Env::new(anchor_spl::token::ID);
    env.initialize();
    let user = env.admin.insecure_clone();
    let (tokens, shares) = env.user(&user, 100);
    env.send(env.deposit(&user.pubkey(), tokens, shares, 100), &user)
        .unwrap();
    let accounts = [
        env.vault,
        env.position(&user.pubkey()),
        env.token_vault,
        env.share_mint,
        tokens,
        shares,
    ];
    let before = env.snapshot(&accounts);
    for (amount, expected) in [(0, "ZeroAmount"), (101, "InsufficientShares")] {
        let error = env
            .send(env.withdraw(&user.pubkey(), tokens, shares, amount), &user)
            .unwrap_err();
        assert!(error.contains(expected), "{error}");
        assert_eq!(env.snapshot(&accounts), before);
    }
    // Имитируем убыток: цена одной share меньше минимальной единицы актива.
    let mut storage = env.svm.get_account(&addr(&env.token_vault)).unwrap();
    let mut state = TokenAccount::unpack(&storage.data).unwrap();
    state.amount = 1;
    TokenAccount::pack(state, &mut storage.data).unwrap();
    env.svm
        .set_account(addr(&env.token_vault), storage)
        .unwrap();
    let before = env.snapshot(&accounts);
    let error = env
        .send(env.withdraw(&user.pubkey(), tokens, shares, 1), &user)
        .unwrap_err();
    assert!(error.contains("WithdrawTooSmall"), "{error}");
    assert_eq!(env.snapshot(&accounts), before);
    env.send(env.withdraw(&user.pubkey(), tokens, shares, 100), &user)
        .unwrap();
    assert_eq!(env.balance(tokens), 1);
    assert_eq!(env.supply(), 0);
}

#[test]
fn withdrawals_reject_foreign_accounts_wrong_mints_and_missing_user_signature() {
    let mut env = Env::new(anchor_spl::token::ID);
    env.initialize();
    let owner = env.admin.insecure_clone();
    let stranger = Keypair::new();
    let (tokens_a, shares_a) = env.user(&owner, 100);
    let (tokens_b, shares_b) = env.user(&stranger, 0);
    let wrong_tokens = env.token_account(env.share_mint, key(&owner.pubkey()), 0);
    let wrong_shares = env.token_account(env.asset_mint, key(&owner.pubkey()), 0);
    env.send(
        env.deposit(&owner.pubkey(), tokens_a, shares_a, 100),
        &owner,
    )
    .unwrap();
    let mut wrong_position = env.withdraw(&stranger.pubkey(), tokens_b, shares_b, 10);
    wrong_position
        .accounts
        .iter_mut()
        .find(|meta| meta.pubkey == addr(&env.position(&stranger.pubkey())))
        .unwrap()
        .pubkey = addr(&env.position(&owner.pubkey()));
    let mut unsigned = env.withdraw(&owner.pubkey(), tokens_a, shares_a, 10);
    unsigned
        .accounts
        .iter_mut()
        .find(|meta| meta.pubkey == owner.pubkey())
        .unwrap()
        .is_signer = false;
    let accounts = [
        env.vault,
        env.position(&owner.pubkey()),
        env.position(&stranger.pubkey()),
        env.token_vault,
        env.share_mint,
        tokens_a,
        shares_a,
        tokens_b,
        shares_b,
        wrong_tokens,
        wrong_shares,
    ];
    let before = env.snapshot(&accounts);
    for (ix, signer, expected) in [
        (
            env.withdraw(&stranger.pubkey(), tokens_b, shares_a, 10),
            &stranger,
            "ConstraintTokenOwner",
        ),
        (
            env.withdraw(&owner.pubkey(), tokens_b, shares_a, 10),
            &owner,
            "ConstraintTokenOwner",
        ),
        (
            env.withdraw(&owner.pubkey(), wrong_tokens, shares_a, 10),
            &owner,
            "ConstraintTokenMint",
        ),
        (
            env.withdraw(&owner.pubkey(), tokens_a, wrong_shares, 10),
            &owner,
            "ConstraintTokenMint",
        ),
        (wrong_position, &stranger, "ConstraintSeeds"),
        (unsigned, &stranger, "AccountNotSigner"),
    ] {
        let error = env.send(ix, signer).unwrap_err();
        assert!(error.contains(expected), "{error}");
        assert_eq!(env.snapshot(&accounts), before);
    }
}

#[test]
fn a_failed_asset_transfer_rolls_back_the_preceding_share_burn() {
    for token_program in [anchor_spl::token::ID, anchor_spl::token_2022::ID] {
        let mut env = Env::new(token_program);
        env.initialize();
        let user = env.admin.insecure_clone();
        let (tokens, shares) = env.user(&user, 100);
        env.send(env.deposit(&user.pubkey(), tokens, shares, 100), &user)
            .unwrap();
        let mut account = env.svm.get_account(&addr(&tokens)).unwrap();
        let mut state = TokenAccount::unpack(&account.data).unwrap();
        state.state = anchor_spl::token_interface::spl_token_2022::state::AccountState::Frozen;
        TokenAccount::pack(state, &mut account.data).unwrap();
        env.svm.set_account(addr(&tokens), account).unwrap();
        let accounts = [
            env.vault,
            env.position(&user.pubkey()),
            env.token_vault,
            env.share_mint,
            tokens,
            shares,
        ];
        let before = env.snapshot(&accounts);
        let error = env
            .send(env.withdraw(&user.pubkey(), tokens, shares, 50), &user)
            .unwrap_err();
        assert!(error.contains("Account is frozen"), "{error}");
        let burn_success = error
            .find(&format!("Program {} success", token_program))
            .unwrap();
        assert!(
            burn_success < error.find("Account is frozen").unwrap(),
            "{error}"
        );
        assert_eq!(env.snapshot(&accounts), before);
    }
}

#[test]
fn withdrawal_multiplication_handles_u64_max_without_overflow() {
    let mut env = Env::new(anchor_spl::token::ID);
    env.initialize();
    let user = env.admin.insecure_clone();
    let (tokens, shares) = env.user(&user, u64::MAX);
    env.send(env.deposit(&user.pubkey(), tokens, shares, u64::MAX), &user)
        .unwrap();
    env.send(
        env.withdraw(&user.pubkey(), tokens, shares, u64::MAX - 1),
        &user,
    )
    .unwrap();
    assert_eq!(env.balance(tokens), u64::MAX - 1);
    assert_eq!(env.supply(), 1);
    assert_eq!(env.balance(env.token_vault), 1);
    env.send(env.withdraw(&user.pubkey(), tokens, shares, 1), &user)
        .unwrap();
    assert_eq!(env.balance(tokens), u64::MAX);
    assert_eq!(env.supply(), 0);
    assert_eq!(env.config().total_deposited, 0);
}

#[test]
fn deposits_update_shares_and_positions_for_both_token_programs() {
    for token_program in [anchor_spl::token::ID, anchor_spl::token_2022::ID] {
        let mut env = Env::new(token_program);
        env.initialize();
        let users = [env.admin.insecure_clone(), Keypair::new(), Keypair::new()];
        let mut total = 0;
        for (user, amount) in users.iter().zip([100_000_000, 50_000_000, 25_000_000]) {
            let (source, shares) = env.user(user, 1_000_000_000);
            env.send(env.deposit(&user.pubkey(), source, shares, amount), user)
                .unwrap();
            total += amount;
            assert_eq!(env.balance(source), 1_000_000_000 - amount);
            assert_eq!(env.balance(shares), amount);
            assert_eq!(env.deposited(&user.pubkey()), amount);
            assert_eq!(env.config().total_deposited, total);
            assert_eq!(env.balance(env.token_vault), total);
        }
        let mint = env.svm.get_account(&addr(&env.share_mint)).unwrap();
        assert_eq!(Mint::unpack(&mint.data).unwrap().supply, total);
    }
}

#[test]
fn late_deposit_uses_actual_assets_and_does_not_take_earlier_profit() {
    let mut env = Env::new(anchor_spl::token::ID);
    env.initialize();
    let admin = env.admin.insecure_clone();
    let (source_a, shares_a) = env.user(&admin, 2_000_000_000);
    env.send(
        env.deposit(&admin.pubkey(), source_a, shares_a, 1_000_000_000),
        &admin,
    )
    .unwrap();
    let profit = instruction::transfer_checked(
        &env.token_program,
        &source_a,
        &env.asset_mint,
        &env.token_vault,
        &key(&admin.pubkey()),
        &[],
        100_000_000,
        6,
    )
    .unwrap();
    env.send(convert(profit), &admin).unwrap();
    let user = Keypair::new();
    let (source_b, shares_b) = env.user(&user, 110_000_000);
    env.send(
        env.deposit(&user.pubkey(), source_b, shares_b, 110_000_000),
        &user,
    )
    .unwrap();
    assert_eq!(env.balance(shares_a), 1_000_000_000);
    assert_eq!(env.balance(shares_b), 100_000_000);
    assert_eq!(env.balance(env.token_vault), 1_210_000_000);
    assert_eq!(env.config().total_deposited, 1_210_000_000);
    assert_eq!(env.deposited(&user.pubkey()), 110_000_000);
}

#[test]
fn tmr_deposit_credits_only_received_tokens_and_rolls_back_a_fully_withheld_deposit() {
    let mut env = Env::new(anchor_spl::token_2022::ID);
    let data = include_bytes!("fixtures/tmr-mint.bin").to_vec();
    let mut mint = env.svm.get_account(&addr(&env.asset_mint)).unwrap();
    mint.lamports = env.svm.minimum_balance_for_rent_exemption(data.len());
    mint.data = data;
    env.svm.set_account(addr(&env.asset_mint), mint).unwrap();
    env.initialize();
    let user = env.admin.insecure_clone();
    let (source, shares) = env.user(&user, 200_000_000);
    env.send(
        env.deposit(&user.pubkey(), source, shares, 100_000_000),
        &user,
    )
    .unwrap();
    assert_eq!(env.balance(source), 100_000_000);
    assert_eq!(env.balance(shares), 99_000_000);
    assert_eq!(env.balance(env.token_vault), 99_000_000);
    assert_eq!(env.config().total_deposited, 99_000_000);
    assert_eq!(env.deposited(&user.pubkey()), 99_000_000);
    let storage = env.svm.get_account(&addr(&env.token_vault)).unwrap();
    let storage = StateWithExtensions::<TokenAccount>::unpack(&storage.data).unwrap();
    assert_eq!(
        u64::from(
            storage
                .get_extension::<TransferFeeAmount>()
                .unwrap()
                .withheld_amount
        ),
        1_000_000
    );
    let accounts = [
        env.vault,
        env.position(&user.pubkey()),
        env.token_vault,
        env.share_mint,
        source,
        shares,
    ];
    let before = env.snapshot(&accounts);
    let error = env
        .send(env.deposit(&user.pubkey(), source, shares, 1), &user)
        .unwrap_err();
    assert!(error.contains("NothingReceived"), "{error}");
    assert_eq!(env.snapshot(&accounts), before);
}

#[test]
fn zero_and_insufficient_deposits_leave_all_balances_and_counters_unchanged() {
    let mut env = Env::new(anchor_spl::token::ID);
    env.initialize();
    let user = env.admin.insecure_clone();
    let (source, shares) = env.user(&user, 100);
    let accounts = [
        env.vault,
        env.position(&user.pubkey()),
        env.token_vault,
        env.share_mint,
        source,
        shares,
    ];
    let before = env.snapshot(&accounts);
    for (amount, expected) in [(0, "ZeroAmount"), (101, "Custom(1)")] {
        let error = env
            .send(env.deposit(&user.pubkey(), source, shares, amount), &user)
            .unwrap_err();
        assert!(error.contains(expected), "{error}");
        assert_eq!(env.snapshot(&accounts), before);
    }
}

#[test]
fn users_cannot_deposit_from_someone_elses_account_or_into_their_shares_or_position() {
    let mut env = Env::new(anchor_spl::token::ID);
    env.initialize();
    let owner = env.admin.insecure_clone();
    let stranger = Keypair::new();
    let (owner_source, owner_shares) = env.user(&owner, 100);
    let (source, shares) = env.user(&stranger, 100);
    let accounts = [
        env.vault,
        env.token_vault,
        env.share_mint,
        source,
        shares,
        owner_source,
        owner_shares,
        env.position(&owner.pubkey()),
        env.position(&stranger.pubkey()),
    ];
    let before = env.snapshot(&accounts);
    let mut wrong_position = env.deposit(&stranger.pubkey(), source, shares, 10);
    wrong_position
        .accounts
        .iter_mut()
        .find(|meta| meta.pubkey == addr(&env.position(&stranger.pubkey())))
        .unwrap()
        .pubkey = addr(&env.position(&owner.pubkey()));
    for (ix, expected) in [
        (
            env.deposit(&stranger.pubkey(), owner_source, shares, 10),
            "ConstraintTokenOwner",
        ),
        (
            env.deposit(&stranger.pubkey(), source, owner_shares, 10),
            "ConstraintTokenOwner",
        ),
        (wrong_position, "ConstraintSeeds"),
    ] {
        let error = env.send(ix, &stranger).unwrap_err();
        assert!(error.contains(expected), "{error}");
        assert_eq!(env.snapshot(&accounts), before);
    }
}

#[test]
fn zero_shares_and_position_overflow_roll_back_the_token_transfer() {
    let mut env = Env::new(anchor_spl::token::ID);
    env.initialize();
    let owner = env.admin.insecure_clone();
    let (owner_source, owner_shares) = env.user(&owner, 1001);
    env.send(
        env.deposit(&owner.pubkey(), owner_source, owner_shares, 1),
        &owner,
    )
    .unwrap();
    let profit = instruction::transfer_checked(
        &env.token_program,
        &owner_source,
        &env.asset_mint,
        &env.token_vault,
        &key(&owner.pubkey()),
        &[],
        1000,
        6,
    )
    .unwrap();
    env.send(convert(profit), &owner).unwrap();
    let user = Keypair::new();
    let (source, shares) = env.user(&user, 2000);
    let accounts = [
        env.vault,
        env.position(&user.pubkey()),
        env.token_vault,
        env.share_mint,
        source,
        shares,
    ];
    let before = env.snapshot(&accounts);
    let error = env
        .send(env.deposit(&user.pubkey(), source, shares, 1), &user)
        .unwrap_err();
    assert!(error.contains("SharesTooSmall"), "{error}");
    assert_eq!(env.snapshot(&accounts), before);

    let position_address = addr(&env.position(&user.pubkey()));
    let mut account = env.svm.get_account(&position_address).unwrap();
    let mut position = UserPosition::try_deserialize(&mut account.data.as_slice()).unwrap();
    position.deposited_amount = u64::MAX;
    position
        .try_serialize(&mut account.data.as_mut_slice())
        .unwrap();
    env.svm.set_account(position_address, account).unwrap();
    let before = env.snapshot(&accounts);
    let error = env
        .send(env.deposit(&user.pubkey(), source, shares, 1001), &user)
        .unwrap_err();
    assert!(error.contains("MathOverflow"), "{error}");
    assert_eq!(env.snapshot(&accounts), before);
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
