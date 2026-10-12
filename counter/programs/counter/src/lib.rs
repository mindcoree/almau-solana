//! Counter — первая программа на Anchor (неделя 5).
//!
//! У каждого пользователя свой счётчик: аккаунт-PDA с сидами `["counter", authority]`.
//! Создать счётчик, прибавить и отнять единицу может только его владелец (authority).
//!
//! Всё, что на неделе 4 в копилке писалось руками, здесь делает Anchor:
//! точка входа и разбор инструкции — `#[program]`, проверки аккаунтов — `#[derive(Accounts)]`,
//! создание аккаунта через CPI в System Program — `init`, дискриминатор и Borsh — `#[account]`.
use anchor_lang::prelude::*;

declare_id!("HN2SRai9T3z7Rsw2avdz7Ns5tewqHctg9JeTeiSU6jTW");

#[program]
pub mod counter {
    use super::*;

    /// Создать счётчик со значением 0.
    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        let counter = &mut ctx.accounts.counter;
        counter.authority = ctx.accounts.authority.key();
        counter.count = 0;
        counter.bump = ctx.bumps.counter;
        msg!("Counter {} created", counter.key());
        Ok(())
    }

    /// Прибавить единицу.
    pub fn increment(ctx: Context<Update>) -> Result<()> {
        let counter = &mut ctx.accounts.counter;
        counter.count = counter.count.checked_add(1).ok_or(CounterError::Overflow)?;
        msg!("count = {}", counter.count);
        Ok(())
    }

    /// Отнять единицу. Ниже нуля — ошибка `Underflow`.
    pub fn decrement(ctx: Context<Update>) -> Result<()> {
        let counter = &mut ctx.accounts.counter;
        counter.count = counter.count.checked_sub(1).ok_or(CounterError::Underflow)?;
        msg!("count = {}", counter.count);
        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    /// Владелец счётчика: подписывает транзакцию и платит rent.
    #[account(mut)]
    pub authority: Signer<'info>,

    /// `init` — Anchor сам создаст аккаунт через CPI в System Program,
    /// подпишет за PDA сидами и запишет дискриминатор.
    #[account(
        init,
        payer = authority,
        space = 8 + Counter::INIT_SPACE,
        seeds = [b"counter", authority.key().as_ref()],
        bump,
    )]
    pub counter: Account<'info, Counter>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Update<'info> {
    pub authority: Signer<'info>,

    /// Anchor проверит: владелец аккаунта — эта программа, дискриминатор — Counter,
    /// адрес — PDA от подписанта, и `counter.authority == authority`.
    #[account(
        mut,
        seeds = [b"counter", authority.key().as_ref()],
        bump = counter.bump,
        has_one = authority,
    )]
    pub counter: Account<'info, Counter>,
}

#[account]
#[derive(InitSpace)]
pub struct Counter {
    pub authority: Pubkey,
    pub count: u64,
    pub bump: u8,
}

#[error_code]
pub enum CounterError {
    #[msg("Счётчик переполнен")]
    Overflow,
    #[msg("Счётчик не может быть меньше нуля")]
    Underflow,
}
