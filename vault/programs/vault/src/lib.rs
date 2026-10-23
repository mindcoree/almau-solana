//! Недели 8–10: токеновый vault с депозитом, shares и выводом через CPI.
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    self, Burn, Mint, MintTo, TokenAccount, TokenInterface, TransferChecked,
};

declare_id!("J6x4mnjWyfsAYVVd1zNSfJwf9FG1k1LNwzjkoodkKmnN");

#[program]
pub mod vault {
    use super::*;

    /// Создать каркас один раз для пары (администратор, принимаемый mint).
    pub fn create_vault(ctx: Context<CreateVault>) -> Result<()> {
        let vault = &mut ctx.accounts.vault;
        vault.admin = ctx.accounts.admin.key();
        vault.asset_mint = ctx.accounts.asset_mint.key();
        vault.share_mint = ctx.accounts.share_mint.key();
        vault.token_vault = ctx.accounts.token_vault.key();
        vault.total_deposited = 0;
        vault.bump = ctx.bumps.vault;
        msg!("Vault {} created", vault.key());
        Ok(())
    }

    /// Пользователь подписывает создание своей позиции; средства не переводятся.
    pub fn open_position(ctx: Context<OpenPosition>) -> Result<()> {
        let position = &mut ctx.accounts.position;
        position.vault = ctx.accounts.vault.key();
        position.owner = ctx.accounts.user.key();
        position.deposited_amount = 0;
        position.bump = ctx.bumps.position;
        msg!("Position {} opened for {}", position.key(), position.owner);
        Ok(())
    }

    /// Зачислить фактически полученные токены и выпустить долю по цене до депозита.
    pub fn deposit(ctx: Context<VaultUser>, amount: u64) -> Result<()> {
        require!(amount > 0, VaultError::ZeroAmount);
        let assets_before = ctx.accounts.token_vault.amount;
        let supply = ctx.accounts.share_mint.supply;

        token_interface::transfer_checked(
            CpiContext::new(
                ctx.accounts.token_program.key(),
                TransferChecked {
                    from: ctx.accounts.user_token_account.to_account_info(),
                    mint: ctx.accounts.asset_mint.to_account_info(),
                    to: ctx.accounts.token_vault.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(),
                },
            ),
            amount,
            ctx.accounts.asset_mint.decimals,
        )?;
        ctx.accounts.token_vault.reload()?;
        // Token-2022 удерживает свою комиссию: shares обеспечиваются доступным балансом.
        let received = ctx
            .accounts
            .token_vault
            .amount
            .checked_sub(assets_before)
            .ok_or(VaultError::MathOverflow)?;
        require!(received > 0, VaultError::NothingReceived);
        let shares = if supply == 0 {
            received
        } else {
            require!(assets_before > 0, VaultError::InvalidVaultBalance);
            (received as u128)
                .checked_mul(supply as u128)
                .and_then(|value| value.checked_div(assets_before as u128))
                .and_then(|value| u64::try_from(value).ok())
                .ok_or(VaultError::MathOverflow)?
        };
        require!(shares > 0, VaultError::SharesTooSmall);
        let deposited = ctx
            .accounts
            .position
            .deposited_amount
            .checked_add(received)
            .ok_or(VaultError::MathOverflow)?;

        let admin = ctx.accounts.vault.admin;
        let asset_mint = ctx.accounts.vault.asset_mint;
        let bump = [ctx.accounts.vault.bump];
        let seeds: &[&[u8]] = &[b"vault", admin.as_ref(), asset_mint.as_ref(), &bump];
        token_interface::mint_to(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.key(),
                MintTo {
                    mint: ctx.accounts.share_mint.to_account_info(),
                    to: ctx.accounts.user_share_account.to_account_info(),
                    authority: ctx.accounts.vault.to_account_info(),
                },
                &[seeds],
            ),
            shares,
        )?;
        ctx.accounts.vault.total_deposited = ctx.accounts.token_vault.amount;
        ctx.accounts.position.deposited_amount = deposited;
        msg!(
            "Deposit: sent={}, received={}, shares={}",
            amount,
            received,
            shares
        );
        Ok(())
    }

    /// Сжечь shares пользователя и вернуть их долю доступных активов vault.
    pub fn withdraw(ctx: Context<VaultUser>, shares: u64) -> Result<()> {
        require!(shares > 0, VaultError::ZeroAmount);
        require!(
            shares <= ctx.accounts.user_share_account.amount,
            VaultError::InsufficientShares
        );
        let supply = ctx.accounts.share_mint.supply;
        require!(supply > 0, VaultError::InvalidVaultBalance);
        let assets = (shares as u128)
            .checked_mul(ctx.accounts.token_vault.amount as u128)
            .and_then(|value| value.checked_div(supply as u128))
            .and_then(|value| u64::try_from(value).ok())
            .ok_or(VaultError::MathOverflow)?;
        require!(assets > 0, VaultError::WithdrawTooSmall);
        let user_balance_before = ctx.accounts.user_token_account.amount;

        token_interface::burn(
            CpiContext::new(
                ctx.accounts.token_program.key(),
                Burn {
                    mint: ctx.accounts.share_mint.to_account_info(),
                    from: ctx.accounts.user_share_account.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(),
                },
            ),
            shares,
        )?;

        let admin = ctx.accounts.vault.admin;
        let asset_mint = ctx.accounts.vault.asset_mint;
        let bump = [ctx.accounts.vault.bump];
        let seeds: &[&[u8]] = &[b"vault", admin.as_ref(), asset_mint.as_ref(), &bump];
        token_interface::transfer_checked(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.key(),
                TransferChecked {
                    from: ctx.accounts.token_vault.to_account_info(),
                    mint: ctx.accounts.asset_mint.to_account_info(),
                    to: ctx.accounts.user_token_account.to_account_info(),
                    authority: ctx.accounts.vault.to_account_info(),
                },
                &[seeds],
            ),
            assets,
            ctx.accounts.asset_mint.decimals,
        )?;
        ctx.accounts.token_vault.reload()?;
        ctx.accounts.user_token_account.reload()?;
        let received = ctx
            .accounts
            .user_token_account
            .amount
            .checked_sub(user_balance_before)
            .ok_or(VaultError::MathOverflow)?;
        // Полностью удержанная комиссия не должна сжигать долю без выплаты.
        require!(received > 0, VaultError::NothingReceived);
        ctx.accounts.vault.total_deposited = ctx.accounts.token_vault.amount;
        msg!(
            "Withdraw: shares={}, assets={}, received={}",
            shares,
            assets,
            received
        );
        Ok(())
    }
}

#[derive(Accounts)]
pub struct CreateVault<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(mint::token_program = token_program)]
    pub asset_mint: InterfaceAccount<'info, Mint>,

    #[account(
        init,
        payer = admin,
        space = 8 + VaultConfig::INIT_SPACE,
        seeds = [b"vault", admin.key().as_ref(), asset_mint.key().as_ref()],
        bump,
    )]
    pub vault: Account<'info, VaultConfig>,

    #[account(
        init,
        payer = admin,
        seeds = [b"share_mint", vault.key().as_ref()],
        bump,
        mint::decimals = asset_mint.decimals,
        mint::authority = vault,
        mint::token_program = token_program,
    )]
    pub share_mint: InterfaceAccount<'info, Mint>,

    #[account(
        init,
        payer = admin,
        seeds = [b"token_vault", vault.key().as_ref()],
        bump,
        token::mint = asset_mint,
        token::authority = vault,
        token::token_program = token_program,
    )]
    pub token_vault: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct OpenPosition<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        seeds = [b"vault", vault.admin.as_ref(), vault.asset_mint.as_ref()],
        bump = vault.bump,
    )]
    pub vault: Account<'info, VaultConfig>,

    #[account(
        init,
        payer = user,
        space = 8 + UserPosition::INIT_SPACE,
        seeds = [b"position", vault.key().as_ref(), user.key().as_ref()],
        bump,
    )]
    pub position: Account<'info, UserPosition>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct VaultUser<'info> {
    pub user: Signer<'info>,

    #[account(
        mut,
        seeds = [b"vault", vault.admin.as_ref(), vault.asset_mint.as_ref()],
        bump = vault.bump,
        has_one = asset_mint,
        has_one = share_mint,
        has_one = token_vault,
    )]
    pub vault: Account<'info, VaultConfig>,

    #[account(
        mut,
        seeds = [b"position", vault.key().as_ref(), user.key().as_ref()],
        bump = position.bump,
        has_one = vault,
        constraint = position.owner == user.key() @ VaultError::Unauthorized,
    )]
    pub position: Account<'info, UserPosition>,

    #[account(mint::token_program = token_program)]
    pub asset_mint: InterfaceAccount<'info, Mint>,

    #[account(
        mut,
        seeds = [b"share_mint", vault.key().as_ref()],
        bump,
        mint::authority = vault,
        mint::token_program = token_program,
    )]
    pub share_mint: InterfaceAccount<'info, Mint>,

    #[account(
        mut,
        seeds = [b"token_vault", vault.key().as_ref()],
        bump,
        token::mint = asset_mint,
        token::authority = vault,
        token::token_program = token_program,
    )]
    pub token_vault: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        token::mint = asset_mint,
        token::authority = user,
        token::token_program = token_program,
    )]
    pub user_token_account: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        token::mint = share_mint,
        token::authority = user,
        token::token_program = token_program,
    )]
    pub user_share_account: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Interface<'info, TokenInterface>,
}

#[account]
#[derive(InitSpace)]
pub struct VaultConfig {
    pub admin: Pubkey,
    pub asset_mint: Pubkey,
    pub share_mint: Pubkey,
    pub token_vault: Pubkey,
    pub total_deposited: u64,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct UserPosition {
    pub vault: Pubkey,
    pub owner: Pubkey,
    pub deposited_amount: u64,
    pub bump: u8,
}

#[error_code]
pub enum VaultError {
    #[msg("Amount must be greater than zero")]
    ZeroAmount,
    #[msg("Transfer received no spendable tokens")]
    NothingReceived,
    #[msg("Deposit is too small to receive a share")]
    SharesTooSmall,
    #[msg("Arithmetic overflow")]
    MathOverflow,
    #[msg("Outstanding shares require a nonzero vault balance")]
    InvalidVaultBalance,
    #[msg("Position must belong to the signer")]
    Unauthorized,
    #[msg("Not enough shares in the user's token account")]
    InsufficientShares,
    #[msg("Withdrawal is too small to receive an asset")]
    WithdrawTooSmall,
}
