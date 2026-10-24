use crate::{
    raydium_clmm::{
        self,
        accounts::{AmmConfig, ObservationState, PoolState},
        program::RaydiumClmm,
    },
    VaultConfig, VaultError,
};
use anchor_lang::prelude::*;
use anchor_spl::{
    token::Token,
    token_2022::Token2022,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

pub fn initialize(ctx: Context<InitializeMarket>) -> Result<()> {
    require!(
        ctx.accounts.share_mint.supply == 0,
        VaultError::OutstandingShares
    );
    let pool = ctx.accounts.pool_state.load()?;
    let asset = ctx.accounts.asset_mint.key();
    let quote = ctx.accounts.quote_mint.key();
    require!(
        (pool.token_mint_0 == asset && pool.token_mint_1 == quote)
            || (pool.token_mint_1 == asset && pool.token_mint_0 == quote),
        VaultError::InvalidPool
    );
    let market = &mut ctx.accounts.market;
    market.vault = ctx.accounts.vault.key();
    market.quote_mint = quote;
    market.quote_vault = ctx.accounts.quote_vault.key();
    market.pool_state = ctx.accounts.pool_state.key();
    market.bump = ctx.bumps.market;
    Ok(())
}

pub fn execute<'info>(
    ctx: Context<'info, Swap<'info>>,
    amount_in: u64,
    minimum_out: u64,
) -> Result<()> {
    require!(amount_in > 0, VaultError::ZeroAmount);
    require!(minimum_out > 0, VaultError::ZeroMinimumOut);
    let input_before = ctx.accounts.token_vault.amount;
    let output_before = ctx.accounts.quote_vault.amount;
    require!(input_before >= amount_in, VaultError::InsufficientAssets);
    {
        let pool = ctx.accounts.pool_state.load()?;
        let zero_for_one = pool.token_mint_0 == ctx.accounts.asset_mint.key();
        let (input_mint, output_mint, input_vault, output_vault) = if zero_for_one {
            (
                pool.token_mint_0,
                pool.token_mint_1,
                pool.token_vault_0,
                pool.token_vault_1,
            )
        } else {
            (
                pool.token_mint_1,
                pool.token_mint_0,
                pool.token_vault_1,
                pool.token_vault_0,
            )
        };
        require_keys_eq!(
            input_mint,
            ctx.accounts.asset_mint.key(),
            VaultError::InvalidPool
        );
        require_keys_eq!(
            output_mint,
            ctx.accounts.quote_mint.key(),
            VaultError::InvalidPool
        );
        require_keys_eq!(
            input_vault,
            ctx.accounts.input_vault.key(),
            VaultError::InvalidPool
        );
        require_keys_eq!(
            output_vault,
            ctx.accounts.output_vault.key(),
            VaultError::InvalidPool
        );
    }
    let admin = ctx.accounts.vault.admin;
    let mint = ctx.accounts.vault.asset_mint;
    let bump = [ctx.accounts.vault.bump];
    let seeds: &[&[u8]] = &[b"vault", admin.as_ref(), mint.as_ref(), &bump];
    raydium_clmm::cpi::swap_v2(
        CpiContext::new_with_signer(
            ctx.accounts.clmm_program.key(),
            raydium_clmm::cpi::accounts::SwapV2 {
                payer: ctx.accounts.vault.to_account_info(),
                amm_config: ctx.accounts.amm_config.to_account_info(),
                pool_state: ctx.accounts.pool_state.to_account_info(),
                input_token_account: ctx.accounts.token_vault.to_account_info(),
                output_token_account: ctx.accounts.quote_vault.to_account_info(),
                input_vault: ctx.accounts.input_vault.to_account_info(),
                output_vault: ctx.accounts.output_vault.to_account_info(),
                observation_state: ctx.accounts.observation_state.to_account_info(),
                token_program: ctx.accounts.token_program.to_account_info(),
                token_program_2022: ctx.accounts.token_program_2022.to_account_info(),
                memo_program: ctx.accounts.memo_program.to_account_info(),
                input_vault_mint: ctx.accounts.asset_mint.to_account_info(),
                output_vault_mint: ctx.accounts.quote_mint.to_account_info(),
            },
            &[seeds],
        )
        .with_remaining_accounts(ctx.remaining_accounts.to_vec()),
        amount_in,
        minimum_out,
        0,
        true,
    )?;
    ctx.accounts.token_vault.reload()?;
    ctx.accounts.quote_vault.reload()?;
    let spent = input_before
        .checked_sub(ctx.accounts.token_vault.amount)
        .ok_or(VaultError::MathOverflow)?;
    let received = ctx
        .accounts
        .quote_vault
        .amount
        .checked_sub(output_before)
        .ok_or(VaultError::MathOverflow)?;
    require_eq!(spent, amount_in, VaultError::PartialSwap);
    require!(received >= minimum_out, VaultError::SlippageExceeded);
    ctx.accounts.vault.total_deposited = ctx.accounts.token_vault.amount;
    msg!(
        "Swap: amount_in={}, amount_out={}, minimum_out={}",
        spent,
        received,
        minimum_out
    );
    Ok(())
}

#[derive(Accounts)]
pub struct InitializeMarket<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(
        seeds = [b"vault", vault.admin.as_ref(), vault.asset_mint.as_ref()], bump = vault.bump,
        has_one = admin, has_one = asset_mint, has_one = share_mint,
    )]
    pub vault: Account<'info, VaultConfig>,
    #[account(init, payer = admin, space = 8 + MarketConfig::INIT_SPACE, seeds = [b"market", vault.key().as_ref()], bump)]
    pub market: Account<'info, MarketConfig>,
    #[account(mint::token_program = asset_token_program)]
    pub asset_mint: InterfaceAccount<'info, Mint>,
    #[account(mint::token_program = quote_token_program, constraint = quote_mint.key() != asset_mint.key() @ VaultError::InvalidPool)]
    pub quote_mint: InterfaceAccount<'info, Mint>,
    #[account(mint::authority = vault, mint::token_program = asset_token_program)]
    pub share_mint: InterfaceAccount<'info, Mint>,
    #[account(init, payer = admin, seeds = [b"quote_vault", vault.key().as_ref()], bump,
        token::mint = quote_mint, token::authority = vault, token::token_program = quote_token_program)]
    pub quote_vault: InterfaceAccount<'info, TokenAccount>,
    pub pool_state: AccountLoader<'info, PoolState>,
    pub clmm_program: Program<'info, RaydiumClmm>,
    pub asset_token_program: Interface<'info, TokenInterface>,
    pub quote_token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Swap<'info> {
    pub admin: Signer<'info>,
    #[account(mut, seeds = [b"vault", vault.admin.as_ref(), vault.asset_mint.as_ref()], bump = vault.bump,
        has_one = admin, has_one = asset_mint, has_one = token_vault)]
    pub vault: Account<'info, VaultConfig>,
    #[account(seeds = [b"market", vault.key().as_ref()], bump = market.bump,
        has_one = vault, has_one = quote_mint, has_one = quote_vault, has_one = pool_state)]
    pub market: Account<'info, MarketConfig>,
    pub asset_mint: InterfaceAccount<'info, Mint>,
    pub quote_mint: InterfaceAccount<'info, Mint>,
    #[account(mut, seeds = [b"token_vault", vault.key().as_ref()], bump, token::mint = asset_mint, token::authority = vault)]
    pub token_vault: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, seeds = [b"quote_vault", vault.key().as_ref()], bump, token::mint = quote_mint, token::authority = vault)]
    pub quote_vault: InterfaceAccount<'info, TokenAccount>,
    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,
    #[account(address = pool_state.load()?.amm_config)]
    pub amm_config: Box<Account<'info, AmmConfig>>,
    #[account(mut, token::mint = asset_mint, token::authority = pool_state)]
    pub input_vault: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, token::mint = quote_mint, token::authority = pool_state)]
    pub output_vault: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, address = pool_state.load()?.observation_key)]
    pub observation_state: AccountLoader<'info, ObservationState>,
    pub clmm_program: Program<'info, RaydiumClmm>,
    pub token_program: Program<'info, Token>,
    pub token_program_2022: Program<'info, Token2022>,
    /// CHECK: canonical SPL Memo program, forwarded to Raydium.
    #[account(address = pubkey!("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr"), executable)]
    pub memo_program: UncheckedAccount<'info>,
}

#[account]
#[derive(InitSpace)]
pub struct MarketConfig {
    pub vault: Pubkey,
    pub quote_mint: Pubkey,
    pub quote_vault: Pubkey,
    pub pool_state: Pubkey,
    pub bump: u8,
}
