//! Profile — децентрализованный on-chain профиль на PDA (неделя 6).
//!
//! Данные разложены по двум аккаунтам, оба — PDA этой программы:
//!
//!   ["profile", owner]     Profile        — сам профиль: имя, описание, ссылка, даты.
//!                                           Один на кошелёк: адрес вычисляется из owner.
//!   ["username", username] UsernameRecord — «запись в реестре имён»: кому принадлежит имя.
//!                                           Второй раз такое имя не создать — аккаунт
//!                                           по этому адресу уже существует.
//!
//! Так профиль находится и по кошельку, и по имени без всякой базы данных:
//! адрес вычисляет любой клиент из сидов.
use anchor_lang::prelude::*;

declare_id!("EuErHgT1GbzogqFpXhCdce5M9U1bAxZc8QwmcvkVLSuP");

/// Имя — это сид PDA, а сид не длиннее 32 байт.
pub const MAX_USERNAME: usize = 32;
pub const MIN_USERNAME: usize = 3;
pub const MAX_BIO: usize = 200;
pub const MAX_URL: usize = 100;

#[program]
pub mod profile {
    use super::*;

    /// Создать профиль и занять имя.
    pub fn create_profile(ctx: Context<CreateProfile>, username: String, bio: String, url: String) -> Result<()> {
        check_texts(&bio, &url)?;
        let now = Clock::get()?.unix_timestamp;

        let profile = &mut ctx.accounts.profile;
        profile.owner = ctx.accounts.owner.key();
        profile.username = username;
        profile.bio = bio;
        profile.url = url;
        profile.created_at = now;
        profile.updated_at = now;
        profile.bump = ctx.bumps.profile;

        let record = &mut ctx.accounts.username_record;
        record.owner = profile.owner;
        record.bump = ctx.bumps.username_record;

        msg!("Profile @{} created for {}", profile.username, profile.owner);
        Ok(())
    }

    /// Изменить описание и ссылку. Имя не меняется: оно — адрес записи в реестре.
    pub fn update_profile(ctx: Context<UpdateProfile>, bio: String, url: String) -> Result<()> {
        check_texts(&bio, &url)?;
        let profile = &mut ctx.accounts.profile;
        profile.bio = bio;
        profile.url = url;
        profile.updated_at = Clock::get()?.unix_timestamp;
        msg!("Profile @{} updated", profile.username);
        Ok(())
    }

    /// Удалить профиль: оба аккаунта закрываются, rent возвращается владельцу, имя освобождается.
    pub fn close_profile(ctx: Context<CloseProfile>) -> Result<()> {
        msg!("Profile @{} closed", ctx.accounts.profile.username);
        Ok(())
    }
}

/// Имя: 3–32 символа, строчные латинские буквы, цифры и `_`.
/// Так у одного имени одно написание — «Timur» и «timur» не станут двумя разными PDA.
pub fn is_valid_username(username: &str) -> bool {
    (MIN_USERNAME..=MAX_USERNAME).contains(&username.len())
        && username.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
}

fn check_texts(bio: &str, url: &str) -> Result<()> {
    require!(bio.len() <= MAX_BIO, ProfileError::BioTooLong);
    require!(url.len() <= MAX_URL, ProfileError::UrlTooLong);
    Ok(())
}

#[derive(Accounts)]
#[instruction(username: String)]
pub struct CreateProfile<'info> {
    /// Имя проверяется здесь, на первом аккаунте, — до того, как Anchor
    /// вычислит из него адрес PDA и создаст аккаунты.
    #[account(mut, constraint = is_valid_username(&username) @ ProfileError::InvalidUsername)]
    pub owner: Signer<'info>,

    #[account(
        init,
        payer = owner,
        space = 8 + Profile::INIT_SPACE,
        seeds = [b"profile", owner.key().as_ref()],
        bump,
    )]
    pub profile: Account<'info, Profile>,

    /// Если имя уже занято, этот аккаунт существует, и `init` упадёт — так и
    /// обеспечивается уникальность имён.
    #[account(
        init,
        payer = owner,
        space = 8 + UsernameRecord::INIT_SPACE,
        seeds = [b"username", username.as_bytes()],
        bump,
    )]
    pub username_record: Account<'info, UsernameRecord>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct UpdateProfile<'info> {
    pub owner: Signer<'info>,

    #[account(
        mut,
        seeds = [b"profile", owner.key().as_ref()],
        bump = profile.bump,
        has_one = owner,
    )]
    pub profile: Account<'info, Profile>,
}

#[derive(Accounts)]
pub struct CloseProfile<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    /// `close = owner` — Anchor обнулит аккаунт и переведёт его lamports владельцу.
    #[account(
        mut,
        close = owner,
        seeds = [b"profile", owner.key().as_ref()],
        bump = profile.bump,
        has_one = owner,
    )]
    pub profile: Account<'info, Profile>,

    /// Адрес записи вычисляется из имени, которое хранится в профиле.
    #[account(
        mut,
        close = owner,
        seeds = [b"username", profile.username.as_bytes()],
        bump = username_record.bump,
        has_one = owner,
    )]
    pub username_record: Account<'info, UsernameRecord>,
}

#[account]
#[derive(InitSpace)]
pub struct Profile {
    pub owner: Pubkey,
    #[max_len(32)]
    pub username: String,
    #[max_len(200)]
    pub bio: String,
    #[max_len(100)]
    pub url: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct UsernameRecord {
    pub owner: Pubkey,
    pub bump: u8,
}

#[error_code]
pub enum ProfileError {
    #[msg("Имя: 3–32 символа, строчные латинские буквы, цифры и _")]
    InvalidUsername,
    #[msg("Описание длиннее 200 байт")]
    BioTooLong,
    #[msg("Ссылка длиннее 100 байт")]
    UrlTooLong,
}
