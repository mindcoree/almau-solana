use crate::{
    raydium_clmm::{
        self,
        accounts::{PersonalPositionState, PoolState},
        program::RaydiumClmm,
    },
    MarketConfig, VaultConfig, VaultError, VaultUser,
};
use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::Token,
    token_2022::Token2022,
    token_interface::{
        self, spl_token_2022::instruction::AuthorityType, Mint, SetAuthority, TokenAccount,
    },
};

pub fn require_single_asset(accounts: &VaultUser) -> Result<()> {
    require!(
        accounts.position_state.data_is_empty(),
        VaultError::TradingVault
    );
    if accounts.market.data_is_empty() {
        return Ok(());
    }
    require_keys_eq!(*accounts.market.owner, crate::ID, VaultError::InvalidPool);
    let market = MarketConfig::try_deserialize(&mut accounts.market.try_borrow_data()?.as_ref())?;
    require_keys_eq!(market.vault, accounts.vault.key(), VaultError::InvalidPool);
    require_keys_eq!(
        market.quote_vault,
        accounts.quote_vault.key(),
        VaultError::InvalidPool
    );
    require!(
        *accounts.quote_vault.owner == anchor_spl::token::ID
            || *accounts.quote_vault.owner == anchor_spl::token_2022::ID,
        VaultError::InvalidPool
    );
    let quote =
        TokenAccount::try_deserialize(&mut accounts.quote_vault.try_borrow_data()?.as_ref())?;
    require_keys_eq!(quote.owner, accounts.vault.key(), VaultError::InvalidPool);
    require_keys_eq!(quote.mint, market.quote_mint, VaultError::InvalidPool);
    require!(quote.amount == 0, VaultError::TradingVault);
    Ok(())
}

pub fn tick_array_start(tick: i32, spacing: u16) -> i32 {
    let width = i32::from(spacing) * 60;
    tick.div_euclid(width) * width
}

fn set_owner<'info>(
    account: &InterfaceAccount<'info, TokenAccount>,
    authority: AccountInfo<'info>,
    owner: Pubkey,
    seeds: &[&[&[u8]]],
) -> Result<()> {
    token_interface::set_authority(
        CpiContext::new_with_signer(
            *account.to_account_info().owner,
            SetAuthority {
                current_authority: authority,
                account_or_mint: account.to_account_info(),
            },
            seeds,
        ),
        AuthorityType::AccountOwner,
        Some(owner),
    )
}

pub fn open_clmm<'info>(
    ctx: Context<'info, OpenClmmPosition<'info>>,
    tick_lower: i32,
    tick_upper: i32,
    liquidity: u128,
    maximum_a: u64,
    maximum_b: u64,
    rent_budget: u64,
) -> Result<()> {
    require!(liquidity > 0 && rent_budget > 0, VaultError::ZeroLiquidity);
    require!(maximum_a > 0 || maximum_b > 0, VaultError::ZeroAmount);
    let spacing = ctx.accounts.pool_state.load()?.tick_spacing;
    require!(
        tick_lower < tick_upper
            && tick_lower % i32::from(spacing) == 0
            && tick_upper % i32::from(spacing) == 0,
        VaultError::InvalidTicks
    );
    require!(
        ctx.accounts.token_vault.amount >= maximum_a
            && ctx.accounts.quote_vault.amount >= maximum_b,
        VaultError::InsufficientAssets
    );
    let before_a = ctx.accounts.token_vault.amount;
    let before_b = ctx.accounts.quote_vault.amount;
    let admin = ctx.accounts.vault.admin;
    let asset = ctx.accounts.vault.asset_mint;
    let vault_bump = [ctx.accounts.vault.bump];
    let vault_seeds: &[&[u8]] = &[b"vault", admin.as_ref(), asset.as_ref(), &vault_bump];
    let vault_key = ctx.accounts.vault.key();
    let payer_bump = [ctx.bumps.liquidity_payer];
    let payer_seeds: &[&[u8]] = &[b"liquidity_payer", vault_key.as_ref(), &payer_bump];
    // Raydium совмещает оплату rent и authority токенов; payer должен быть System-owned без данных.
    anchor_lang::system_program::transfer(
        CpiContext::new(
            ctx.accounts.system_program.key(),
            anchor_lang::system_program::Transfer {
                from: ctx.accounts.admin.to_account_info(),
                to: ctx.accounts.liquidity_payer.to_account_info(),
            },
        ),
        rent_budget,
    )?;
    for account in [&ctx.accounts.token_vault, &ctx.accounts.quote_vault] {
        set_owner(
            account,
            ctx.accounts.vault.to_account_info(),
            ctx.accounts.liquidity_payer.key(),
            &[vault_seeds],
        )?;
    }
    raydium_clmm::cpi::open_position_with_token22_nft(
        CpiContext::new_with_signer(
            ctx.accounts.clmm_program.key(),
            raydium_clmm::cpi::accounts::OpenPositionWithToken22Nft {
                payer: ctx.accounts.liquidity_payer.to_account_info(),
                position_nft_owner: ctx.accounts.vault.to_account_info(),
                position_nft_mint: ctx.accounts.nft_mint.to_account_info(),
                position_nft_account: ctx.accounts.nft_account.to_account_info(),
                pool_state: ctx.accounts.pool_state.to_account_info(),
                protocol_position: ctx.accounts.protocol_position.to_account_info(),
                tick_array_lower: ctx.accounts.tick_array_lower.to_account_info(),
                tick_array_upper: ctx.accounts.tick_array_upper.to_account_info(),
                personal_position: ctx.accounts.personal_position.to_account_info(),
                token_account_0: ctx.accounts.token_vault.to_account_info(),
                token_account_1: ctx.accounts.quote_vault.to_account_info(),
                token_vault_0: ctx.accounts.pool_vault_a.to_account_info(),
                token_vault_1: ctx.accounts.pool_vault_b.to_account_info(),
                rent: ctx.accounts.rent.to_account_info(),
                system_program: ctx.accounts.system_program.to_account_info(),
                token_program: ctx.accounts.token_program.to_account_info(),
                associated_token_program: ctx.accounts.associated_token_program.to_account_info(),
                token_program_2022: ctx.accounts.token_program_2022.to_account_info(),
                vault_0_mint: ctx.accounts.asset_mint.to_account_info(),
                vault_1_mint: ctx.accounts.quote_mint.to_account_info(),
            },
            &[payer_seeds],
        )
        .with_remaining_accounts(ctx.remaining_accounts.to_vec()),
        tick_lower,
        tick_upper,
        tick_array_start(tick_lower, spacing),
        tick_array_start(tick_upper, spacing),
        liquidity,
        maximum_a,
        maximum_b,
        true,
        None,
    )?;
    for account in [&ctx.accounts.token_vault, &ctx.accounts.quote_vault] {
        set_owner(
            account,
            ctx.accounts.liquidity_payer.to_account_info(),
            vault_key,
            &[payer_seeds],
        )?;
    }
    let leftover = ctx.accounts.liquidity_payer.lamports();
    anchor_lang::system_program::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.system_program.key(),
            anchor_lang::system_program::Transfer {
                from: ctx.accounts.liquidity_payer.to_account_info(),
                to: ctx.accounts.admin.to_account_info(),
            },
            &[payer_seeds],
        ),
        leftover,
    )?;
    ctx.accounts.token_vault.reload()?;
    ctx.accounts.quote_vault.reload()?;
    let spent_a = before_a
        .checked_sub(ctx.accounts.token_vault.amount)
        .ok_or(VaultError::MathOverflow)?;
    let spent_b = before_b
        .checked_sub(ctx.accounts.quote_vault.amount)
        .ok_or(VaultError::MathOverflow)?;
    require!(
        spent_a <= maximum_a && spent_b <= maximum_b,
        VaultError::SlippageExceeded
    );
    require_keys_eq!(
        ctx.accounts.token_vault.owner,
        vault_key,
        VaultError::PositionMismatch
    );
    require_keys_eq!(
        ctx.accounts.quote_vault.owner,
        vault_key,
        VaultError::PositionMismatch
    );
    let personal = PersonalPositionState::try_deserialize(
        &mut ctx.accounts.personal_position.try_borrow_data()?.as_ref(),
    )?;
    require!(
        personal.liquidity == liquidity
            && personal.pool_id == ctx.accounts.pool_state.key()
            && personal.nft_mint == ctx.accounts.nft_mint.key(),
        VaultError::PositionMismatch
    );
    let position = &mut ctx.accounts.position_state;
    position.vault = vault_key;
    position.nft_mint = ctx.accounts.nft_mint.key();
    position.personal_position = ctx.accounts.personal_position.key();
    position.bump = ctx.bumps.position_state;
    ctx.accounts.vault.total_deposited = ctx.accounts.token_vault.amount;
    msg!(
        "OpenClmm: liquidity={}, spent_a={}, spent_b={}",
        liquidity,
        spent_a,
        spent_b
    );
    Ok(())
}

fn decrease<'info>(
    accounts: &ManageClmmPosition<'info>,
    remaining: Vec<AccountInfo<'info>>,
    liquidity: u128,
    minimum_a: u64,
    minimum_b: u64,
) -> Result<()> {
    let admin = accounts.vault.admin;
    let mint = accounts.vault.asset_mint;
    let bump = [accounts.vault.bump];
    let seeds: &[&[u8]] = &[b"vault", admin.as_ref(), mint.as_ref(), &bump];
    raydium_clmm::cpi::decrease_liquidity_v2(
        CpiContext::new_with_signer(
            accounts.clmm_program.key(),
            raydium_clmm::cpi::accounts::DecreaseLiquidityV2 {
                nft_owner: accounts.vault.to_account_info(),
                nft_account: accounts.nft_account.to_account_info(),
                personal_position: accounts.personal_position.to_account_info(),
                pool_state: accounts.pool_state.to_account_info(),
                protocol_position: accounts.protocol_position.to_account_info(),
                token_vault_0: accounts.pool_vault_a.to_account_info(),
                token_vault_1: accounts.pool_vault_b.to_account_info(),
                tick_array_lower: accounts.tick_array_lower.to_account_info(),
                tick_array_upper: accounts.tick_array_upper.to_account_info(),
                recipient_token_account_0: accounts.token_vault.to_account_info(),
                recipient_token_account_1: accounts.quote_vault.to_account_info(),
                token_program: accounts.token_program.to_account_info(),
                token_program_2022: accounts.token_program_2022.to_account_info(),
                memo_program: accounts.memo_program.to_account_info(),
                vault_0_mint: accounts.asset_mint.to_account_info(),
                vault_1_mint: accounts.quote_mint.to_account_info(),
            },
            &[seeds],
        )
        .with_remaining_accounts(remaining),
        liquidity,
        minimum_a,
        minimum_b,
    )
}

pub fn collect_clmm<'info>(ctx: Context<'info, ManageClmmPosition<'info>>) -> Result<()> {
    let a = ctx.accounts.token_vault.amount;
    let b = ctx.accounts.quote_vault.amount;
    decrease(&ctx.accounts, ctx.remaining_accounts.to_vec(), 0, 0, 0)?;
    ctx.accounts.token_vault.reload()?;
    ctx.accounts.quote_vault.reload()?;
    ctx.accounts.vault.total_deposited = ctx.accounts.token_vault.amount;
    msg!(
        "CollectFees: a={}, b={}",
        ctx.accounts
            .token_vault
            .amount
            .checked_sub(a)
            .ok_or(VaultError::MathOverflow)?,
        ctx.accounts
            .quote_vault
            .amount
            .checked_sub(b)
            .ok_or(VaultError::MathOverflow)?
    );
    Ok(())
}

pub fn close_clmm<'info>(
    ctx: Context<'info, ManageClmmPosition<'info>>,
    minimum_a: u64,
    minimum_b: u64,
) -> Result<()> {
    let liquidity = ctx.accounts.personal_position.liquidity;
    let before_a = ctx.accounts.token_vault.amount;
    let before_b = ctx.accounts.quote_vault.amount;
    decrease(
        &ctx.accounts,
        ctx.remaining_accounts.to_vec(),
        liquidity,
        minimum_a,
        minimum_b,
    )?;
    let admin = ctx.accounts.vault.admin;
    let mint = ctx.accounts.vault.asset_mint;
    let bump = [ctx.accounts.vault.bump];
    let seeds: &[&[u8]] = &[b"vault", admin.as_ref(), mint.as_ref(), &bump];
    let lamports_before = ctx.accounts.vault.get_lamports();
    raydium_clmm::cpi::close_position(
        CpiContext::new_with_signer(
            ctx.accounts.clmm_program.key(),
            raydium_clmm::cpi::accounts::ClosePosition {
                nft_owner: ctx.accounts.vault.to_account_info(),
                position_nft_mint: ctx.accounts.nft_mint.to_account_info(),
                position_nft_account: ctx.accounts.nft_account.to_account_info(),
                personal_position: ctx.accounts.personal_position.to_account_info(),
                system_program: ctx.accounts.system_program.to_account_info(),
                token_program: ctx.accounts.token_program_2022.to_account_info(),
            },
            &[seeds],
        )
        .with_remaining_accounts(vec![ctx.accounts.pool_state.to_account_info()]),
    )?;
    let refund = ctx
        .accounts
        .vault
        .get_lamports()
        .checked_sub(lamports_before)
        .ok_or(VaultError::MathOverflow)?;
    ctx.accounts.vault.sub_lamports(refund)?;
    ctx.accounts.admin.add_lamports(refund)?;
    ctx.accounts
        .position_state
        .close(ctx.accounts.admin.to_account_info())?;
    ctx.accounts.token_vault.reload()?;
    ctx.accounts.quote_vault.reload()?;
    ctx.accounts.vault.total_deposited = ctx.accounts.token_vault.amount;
    msg!(
        "CloseClmm: liquidity={}, returned_a={}, returned_b={}",
        liquidity,
        ctx.accounts
            .token_vault
            .amount
            .checked_sub(before_a)
            .ok_or(VaultError::MathOverflow)?,
        ctx.accounts
            .quote_vault
            .amount
            .checked_sub(before_b)
            .ok_or(VaultError::MathOverflow)?
    );
    Ok(())
}

#[derive(Accounts)]
#[instruction(tick_lower: i32, tick_upper: i32)]
pub struct OpenClmmPosition<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(mut, seeds = [b"vault", vault.admin.as_ref(), vault.asset_mint.as_ref()], bump = vault.bump,
        has_one = admin, has_one = asset_mint, has_one = token_vault)]
    pub vault: Account<'info, VaultConfig>,
    #[account(seeds = [b"market", vault.key().as_ref()], bump = market.bump, has_one = vault,
        has_one = quote_mint, has_one = quote_vault, has_one = pool_state)]
    pub market: Account<'info, MarketConfig>,
    #[account(init, payer = admin, space = 8 + VaultPosition::INIT_SPACE, seeds = [b"vault_position", vault.key().as_ref()], bump)]
    pub position_state: Account<'info, VaultPosition>,
    #[account(mut, seeds = [b"liquidity_payer", vault.key().as_ref()], bump)]
    pub liquidity_payer: SystemAccount<'info>,
    pub asset_mint: InterfaceAccount<'info, Mint>,
    pub quote_mint: InterfaceAccount<'info, Mint>,
    #[account(mut, token::mint = asset_mint, token::authority = vault, address = vault.token_vault)]
    pub token_vault: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, token::mint = quote_mint, token::authority = vault)]
    pub quote_vault: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, constraint = pool_state.load()?.token_mint_0 == asset_mint.key() @ VaultError::InvalidPool,
        constraint = pool_state.load()?.token_mint_1 == quote_mint.key() @ VaultError::InvalidPool)]
    pub pool_state: AccountLoader<'info, PoolState>,
    #[account(mut)]
    pub nft_mint: Signer<'info>,
    /// CHECK: Raydium создаёт ATA для NFT с owner = vault и проверяет его адрес.
    #[account(mut)]
    pub nft_account: UncheckedAccount<'info>,
    /// CHECK: создаётся Raydium; связь с NFT проверяется после CPI.
    #[account(mut)]
    pub personal_position: UncheckedAccount<'info>,
    /// CHECK: deprecated Raydium ABI placeholder, не используется программой Raydium.
    pub protocol_position: UncheckedAccount<'info>,
    #[account(mut, address = pool_state.load()?.token_vault_0, token::mint = asset_mint, token::authority = pool_state)]
    pub pool_vault_a: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, address = pool_state.load()?.token_vault_1, token::mint = quote_mint, token::authority = pool_state)]
    pub pool_vault_b: InterfaceAccount<'info, TokenAccount>,
    /// CHECK: seeds и размер проверяет Raydium при создании/загрузке массива.
    #[account(mut)]
    pub tick_array_lower: UncheckedAccount<'info>,
    /// CHECK: seeds и размер проверяет Raydium при создании/загрузке массива.
    #[account(mut)]
    pub tick_array_upper: UncheckedAccount<'info>,
    pub clmm_program: Program<'info, RaydiumClmm>,
    pub token_program: Program<'info, Token>,
    pub token_program_2022: Program<'info, Token2022>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub rent: Sysvar<'info, Rent>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ManageClmmPosition<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(mut, seeds = [b"vault", vault.admin.as_ref(), vault.asset_mint.as_ref()], bump = vault.bump,
        has_one = admin, has_one = asset_mint, has_one = token_vault)]
    pub vault: Account<'info, VaultConfig>,
    #[account(seeds = [b"market", vault.key().as_ref()], bump = market.bump, has_one = vault,
        has_one = quote_mint, has_one = quote_vault, has_one = pool_state)]
    pub market: Account<'info, MarketConfig>,
    #[account(mut, seeds = [b"vault_position", vault.key().as_ref()], bump = position_state.bump,
        has_one = vault, has_one = nft_mint, has_one = personal_position)]
    pub position_state: Account<'info, VaultPosition>,
    pub asset_mint: InterfaceAccount<'info, Mint>,
    pub quote_mint: InterfaceAccount<'info, Mint>,
    #[account(mut, token::mint = asset_mint, token::authority = vault, address = vault.token_vault)]
    pub token_vault: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, token::mint = quote_mint, token::authority = vault)]
    pub quote_vault: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, constraint = pool_state.load()?.token_mint_0 == asset_mint.key() @ VaultError::InvalidPool,
        constraint = pool_state.load()?.token_mint_1 == quote_mint.key() @ VaultError::InvalidPool)]
    pub pool_state: AccountLoader<'info, PoolState>,
    #[account(mut, mint::token_program = token_program_2022)]
    pub nft_mint: InterfaceAccount<'info, Mint>,
    #[account(mut, token::mint = nft_mint, token::authority = vault, token::token_program = token_program_2022,
        constraint = nft_account.amount == 1 @ VaultError::PositionMismatch)]
    pub nft_account: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, constraint = personal_position.pool_id == pool_state.key() @ VaultError::PositionMismatch,
        constraint = personal_position.nft_mint == nft_mint.key() @ VaultError::PositionMismatch)]
    pub personal_position: Box<Account<'info, PersonalPositionState>>,
    /// CHECK: deprecated Raydium ABI placeholder.
    pub protocol_position: UncheckedAccount<'info>,
    #[account(mut, address = pool_state.load()?.token_vault_0, token::mint = asset_mint, token::authority = pool_state)]
    pub pool_vault_a: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, address = pool_state.load()?.token_vault_1, token::mint = quote_mint, token::authority = pool_state)]
    pub pool_vault_b: InterfaceAccount<'info, TokenAccount>,
    /// CHECK: Raydium сверяет tick arrays с сохранёнными границами personal_position.
    #[account(mut)]
    pub tick_array_lower: UncheckedAccount<'info>,
    /// CHECK: Raydium сверяет tick arrays с сохранёнными границами personal_position.
    #[account(mut)]
    pub tick_array_upper: UncheckedAccount<'info>,
    pub clmm_program: Program<'info, RaydiumClmm>,
    pub token_program: Program<'info, Token>,
    pub token_program_2022: Program<'info, Token2022>,
    /// CHECK: canonical SPL Memo program.
    #[account(address = pubkey!("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr"), executable)]
    pub memo_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[account]
#[derive(InitSpace)]
pub struct VaultPosition {
    pub vault: Pubkey,
    pub nft_mint: Pubkey,
    pub personal_position: Pubkey,
    pub bump: u8,
}
