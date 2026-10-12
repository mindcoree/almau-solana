//! # piggy — копилка на Rust, устроенная по модели Solana
//!
//! Проект повторяет настоящий стек Solana в миниатюре, чтобы на нём можно было
//! потренировать конструкции Rust, которые встречаются в Solana-разработке:
//!
//! | модуль               | аналог в Solana                                                      |
//! |----------------------|----------------------------------------------------------------------|
//! | [`sdk`]              | `solana-program`: `Pubkey`, `AccountInfo`, `msg!`, `invoke_signed`…  |
//! | [`system_program`]   | встроенная System Program `11111111111111111111111111111111`         |
//! | [`vault`]            | **наша on-chain программа** (в реальном проекте — отдельный crate)   |
//! | [`runtime`]          | валидатор: подписи, исполнение, CPI, правила владения, rent          |
//! | [`client`]           | `solana-sdk`: ключи, транзакции, SOL ↔ лампорты                      |
//!
//! Порядок чтения: `vault/` → `sdk/` → `runtime.rs` → `main.rs`.

// В проекте нет ни строчки unsafe — и компилятор за этим следит.
#![forbid(unsafe_code)]

pub mod client;
pub mod runtime;
pub mod sdk;
pub mod system_program;
pub mod vault;
