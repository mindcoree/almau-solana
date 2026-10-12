# piggy — копилка на Rust по модели Solana

**Автор:** Медиханов Тимур

Подробное руководство к локальному проекту из корня репозитория.
Все команды Cargo ниже выполняются в `almau-solana/`.
Общий список работ и отчётов находится в [README репозитория](../README.md).

`piggy` — учебная CLI-программа: «копилка» (vault), в которую кто угодно может
положить SOL, а снять — только владелец. Внутри всё устроено как в Solana: адреса PDA,
`AccountInfo` с `Rc<RefCell<…>>`, Borsh, CPI в System Program, коды ошибок, подписи
ed25519, rent. Но работает локально: не нужны ни Solana CLI, ни валидатор — только Rust.

Код программы (`src/vault/`) написан так же, как настоящая нативная Solana-программа,
и переносится на настоящий SDK почти одной заменой `use`. Остальное — мини-SDK,
System Program и «валидатор» — нужно, чтобы программа ожила и было видно, как всё
работает изнутри.

```text
$ piggy
piggy — копилка SOL на Rust по модели Solana: кошельки, PDA, CPI, Borsh

Usage: piggy [OPTIONS] <COMMAND>

Commands:
  keygen    Создать кошелёк (файл ключа в формате solana-keygen)
  wallets   Кошельки, балансы и копилки
  airdrop   Начислить SOL «из воздуха» (как `solana airdrop` в devnet)
  pda       Показать, как вычисляется адрес копилки (PDA), — без транзакции
  init      Создать копилку
  deposit   Положить SOL в копилку (свою или чужую)
  withdraw  Снять SOL из копилки (разрешено только владельцу)
  close     Закрыть копилку и вернуть владельцу всё, включая rent-резерв
  show      Копилка: баланс, rent-резерв и разбор данных по байтам
  account   Сырой аккаунт по адресу или имени кошелька (как `solana account`)
  help      Print this message or the help of the given subcommand(s)

Options:
      --home <HOME>  Папка с «блокчейном» (ledger.json) и кошельками [env: PIGGY_HOME=] [default: ~/.config/piggy]
  -h, --help         Print help
  -V, --version      Print version
```

## Быстрый старт

### 1. Установить Rust (один раз)

```bash
# macOS / Linux
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Windows: скачайте и запустите rustup-init.exe с https://rustup.rs
# (установщик предложит поставить Visual Studio Build Tools — соглашайтесь)

rustup update                     # нужна версия 1.85 или новее
```

После установки откройте новое окно терминала — так подхватится путь `~/.cargo/bin`.

### 2. Установить piggy (один раз)

```bash
git clone https://github.com/mindcoree/almau-solana.git
cd almau-solana
cargo install --locked --path .   # собирает и кладёт команду piggy в ~/.cargo/bin
```

Всё: теперь `piggy` — обычная команда терминала, из любой папки.

```bash
piggy                             # справка со списком команд
piggy keygen alice                # первая команда — создать кошелёк
```

Кошельки и «блокчейн» лежат в `~/.config/piggy` — как у Solana CLI в `~/.config/solana`.
Другую папку можно задать через `--home <папка>` или переменную `PIGGY_HOME`.

### Команды

| Команда | Что делает |
|---|---|
| `piggy keygen <имя>` | создать кошелёк (ключ ed25519 в формате solana-keygen) |
| `piggy wallets` | все кошельки, балансы и копилки |
| `piggy airdrop <имя> <SOL>` | начислить SOL «из воздуха», как в devnet |
| `piggy pda <имя>` | показать, как вычисляется адрес копилки и bump (без транзакции) |
| `piggy init <имя> [--deposit <SOL>]` | создать копилку; можно сразу положить SOL той же транзакцией |
| `piggy deposit <имя> <SOL> [--to <кому>]` | положить SOL в свою копилку или в чужую |
| `piggy withdraw <имя> <SOL> [--from <чья>]` | снять SOL; с `--from` можно попробовать снять из чужой копилки — программа откажет |
| `piggy close <имя>` | закрыть копилку и забрать всё, включая rent-резерв |
| `piggy show <имя>` | копилка: баланс, резерв и разбор данных по байтам |
| `piggy account <имя или адрес>` | «сырой» аккаунт, как `solana account` |

- Суммы — в SOL через точку: `1`, `2.5`, `0.25`; не больше 9 знаков после точки.
- Вместо имени владельца (`pda`, `show`, `account`, `--to`, `--from`) можно указать
  адрес в base58.
- Справка по команде: `piggy withdraw --help`.

### Весь сценарий по шагам

```bash
piggy keygen alice                 # кошелёк = ключ ed25519 в формате solana-keygen
piggy keygen bob
piggy airdrop alice 10             # SOL «из воздуха», как в devnet
piggy airdrop bob 5
piggy pda alice                    # как ищется адрес копилки и bump
piggy init alice                   # создать копилку: CPI + invoke_signed
piggy deposit alice 2.5
piggy deposit bob 1 --to alice     # пополнить чужую копилку — можно
piggy withdraw bob 1 --from alice  # снять из чужой — VaultError::Unauthorized
piggy withdraw alice 0.5
piggy withdraw alice 100           # больше, чем есть, — InsufficientFunds
piggy show alice                   # баланс, rent-резерв, байты Borsh по полям
piggy account alice                # «сырой» аккаунт, как `solana account`
piggy close alice                  # закрыть и вернуть всё, включая резерв
piggy wallets
```

### Обновить, сбросить, удалить

```bash
cargo install --path .             # после правок в коде — пересобрать команду piggy
cargo run -q -- wallets            # или запускать прямо из папки проекта, без установки
rm -rf ~/.config/piggy             # начать с чистого листа: удалить кошельки и «блокчейн»
cargo uninstall piggy              # удалить команду piggy
```

В Windows PowerShell папка данных удаляется так:
`Remove-Item -Recurse -Force $HOME\.config\piggy`.

Как это выглядит (адреса у вас будут другие — ключи случайные):

```text
$ piggy pda alice
PDA = sha256(сиды ‖ bump ‖ program_id ‖ "ProgramDerivedAddress")
  сиды:       ["vault", 8F4ERs2oxgjX6rChCPBaQMynWmFkvtuJhZ36Jonai5ST]
  program_id: Vau1tkczJmsqv3E7sVPgDnNZu3W2HjmSidktauy74DJ
Перебираем bump от 255 вниз, пока адрес не окажется вне кривой ed25519:
  bump 255: точка на кривой — у неё мог бы быть приватный ключ, пропускаем
  bump 254: точка на кривой — у неё мог бы быть приватный ключ, пропускаем
  bump 253: точка на кривой — у неё мог бы быть приватный ключ, пропускаем
  bump 252: F9xurmNPyiaMG5dpoB2Nb6bwxSfwHCpdxBxHgU5boS6H — вне кривой ✓

$ piggy init alice
Транзакция 5F6TsVhu…xJWrDaaT
  Program Vau1tkczJmsqv3E7sVPgDnNZu3W2HjmSidktauy74DJ invoke [1]
  Program log: Instruction: Initialize
  Program 11111111111111111111111111111111 invoke [2]
  Program 11111111111111111111111111111111 success
  Program log: Vault F9xurmNPyiaMG5dpoB2Nb6bwxSfwHCpdxBxHgU5boS6H created, bump 252
  Program Vau1tkczJmsqv3E7sVPgDnNZu3W2HjmSidktauy74DJ success
  комиссия: 5000 лампортов
✓ успешно

$ piggy withdraw bob 1 --from alice
Транзакция 4VzEHCNY…5bGjWnS5
  Program Vau1tkczJmsqv3E7sVPgDnNZu3W2HjmSidktauy74DJ invoke [1]
  Program log: Instruction: Withdraw
  Program Vau1tkczJmsqv3E7sVPgDnNZu3W2HjmSidktauy74DJ failed: custom program error: 0x1770
  комиссия: 5000 лампортов
  → VaultError::Unauthorized (6000): снимать и закрывать копилку может только её владелец
Ошибка: транзакция отклонена: Error processing Instruction 0: custom program error: 0x1770

$ piggy show alice
  …
Данные аккаунта — 57 байт (Borsh):
  [0..8]    d3 08 e8 2b 02 98 75 77          дискриминатор = sha256("account:Vault")[..8]
  [8..40]   6b 9b 97 fd 8d ff … (32 байта)   owner = 8F4ERs2oxgjX6rChCPBaQMynWmFkvtuJhZ36Jonai5ST
  [40..41]  fc                               bump = 252
  [41..49]  00 c3 9d d0 00 00 00 00          total_deposited = 3500000000 (u64 little-endian)
  [49..57]  00 65 cd 1d 00 00 00 00          total_withdrawn = 500000000
```

## Что где лежит

```text
src/
  vault/             наша on-chain программа — читать первой
    instruction.rs     enum инструкций (Borsh) и конструкторы для клиента
    processor.rs       логика и все проверки безопасности
    state.rs           данные аккаунта, дискриминатор, трейт AccountState
    error.rs           коды ошибок 6000+ и их расшифровка
  sdk/               мини-solana_program: Pubkey и PDA, AccountInfo, CPI, msg!, Rent
  system_program.rs  System Program: create_account и transfer, ручной разбор байтов
  runtime.rs         «валидатор»: подписи, исполнение, CPI, правила владения, rent
  client.rs          ключи, транзакции, SOL ↔ лампорты
  main.rs            CLI на clap
tests/vault_flow.rs  сценарии и атаки: подделка подписи, чужая копилка, программа-взломщик
```

## Конструкции Rust и где их смотреть

| Конструкция | Зачем в Solana | Где |
|---|---|---|
| `struct` и `enum` с данными, `#[derive]` | состояние аккаунтов и инструкции | `vault/state.rs`, `vault/instruction.rs` |
| Borsh: `BorshSerialize` / `BorshDeserialize` | основной формат данных on-chain | `state.rs` (`save`/`load`), `instruction.rs` |
| Байты вручную: `to_le_bytes`, `split_first_chunk`, `try_into` | нативные и Pinocchio-программы | `system_program.rs` |
| `match`, `let … else`, срез-паттерны `[a, b, ..]` | разбор инструкций и аккаунтов | `processor.rs` |
| `Result`, `?`, `From` / `TryFrom`, `thiserror` | ошибки программы и коды для клиента | `vault/error.rs`, `sdk/program_error.rs` |
| `anyhow` | ошибки в CLI и клиентском коде | `main.rs`, `client.rs` |
| Времена жизни `'a` / `'info`, инвариантность | `AccountInfo<'a>` и структуры аккаунтов | `sdk/account_info.rs`, `DepositAccounts` в `processor.rs` |
| `Rc<RefCell<&'a mut T>>`, `try_borrow_mut_*`, `**` | общий изменяемый доступ к аккаунтам | `sdk/account_info.rs`, `process_withdraw` |
| Трейты: ассоциированные константы, методы по умолчанию, супертрейты | «типизированные» аккаунты, как `#[account]` в Anchor | `AccountState` в `state.rs` |
| Обобщённые функции с ограничениями трейтов | `Instruction::new_with_borsh<T: BorshSerialize>` | `sdk/instruction.rs` |
| Итераторы, `collect::<Result<Vec<_>, _>>()` | разбор аккаунтов, проверки | `next_account_info`, `runtime.rs` |
| `macro_rules!` | `msg!`, `require!`, `declare_id!`, `entrypoint!` | `sdk/` |
| `const fn`, вычисления при компиляции | адрес программы в `declare_id!` | `Pubkey::from_str_const` |
| `checked_add`, целые лампорты, `overflow-checks` | деньги без переполнений и без `f64` | `processor.rs`, `client.rs`, `Cargo.toml` |
| Указатели на функции `fn(…) -> …` | точка входа программы | `sdk/entrypoint.rs`, `runtime.rs` |
| `thread_local!`, `catch_unwind` | как рантайм держит контекст и ловит панику программы | `runtime.rs` |
| `#[cfg(test)]`, интеграционные и doc-тесты | тесты программ | `tests/`, модули `tests` |

Почему у структур аккаунтов два времени жизни, а не одно: `AccountInfo<'a>` хранит
`RefCell`, поэтому он инвариантен по `'a`. С одним общим временем жизни компилятор
ответит `lifetime may not live long enough … the struct AccountInfo<'a> is invariant
over the parameter 'a` — это одна из самых частых ошибок новичков в Solana.

## Идеи Solana, которые здесь работают

- **Аккаунты и владельцы.** Менять данные и списывать лампорты может только
  программа-владелец; пополнять можно любой writable-аккаунт; отдать аккаунт другой
  программе можно только с обнулёнными данными. Это проверяет рантайм, а не честность
  программы: в тестах программа-«взломщик» пробует обойти правила и получает те же
  ошибки, что выдал бы настоящий валидатор. Права в CPI рантайм тоже сверяет со своими
  записями, а не с флагами `AccountInfo`, которые программа может поменять у клона.
- **PDA.** Адрес копилки — `sha256("vault" ‖ owner ‖ bump ‖ program_id ‖ "ProgramDerivedAddress")`,
  лежащий вне кривой ed25519. Приватного ключа у него нет — «подписывает» программа
  через `invoke_signed`, и только за свои PDA.
- **CPI.** `Deposit` вызывает `transfer` System Program, `Initialize` — `create_account`
  с подписью PDA. Уровень вложенности виден в логах: `invoke [1]`, `invoke [2]`.
- **Rent.** Аккаунт держит неснимаемый минимум `(128 + байты данных) × 6960` лампортов:
  копилка на 57 байт — 1 287 600. При закрытии аккаунта депозит возвращается.
- **Проверки в программе.** Подписант, владелец аккаунта, дискриминатор, адрес PDA
  и главная — что подписал именно владелец копилки (`has_one` в терминах Anchor).
- **Транзакции.** Подписи ed25519, комиссия 5000 лампортов за подпись (её списывают
  и при ошибке, но она не может оставить кошелёк ниже rent-минимума), атомарность
  нескольких инструкций (`init --deposit`), удаление аккаунтов с нулевым балансом,
  глубина CPI не больше 5 и запрет повторного входа (A → B → A).
- **Ошибки.** В блокчейне от ошибки остаётся число (`custom program error: 0x1770`),
  текст восстанавливает клиент через `TryFrom<u32>`.

## Как это соотносится с настоящим SDK

| здесь | в Solana |
|---|---|
| `crate::sdk::pubkey::Pubkey`, `declare_id!` | `solana_pubkey` (сейчас это псевдоним `solana_address::Address`) |
| `crate::sdk::account_info::{AccountInfo, next_account_info}` | `solana_account_info` |
| `crate::sdk::program_error::{ProgramError, ProgramResult}` | `solana_program_error` |
| `crate::sdk::instruction::{Instruction, AccountMeta}` | `solana_instruction` |
| `crate::sdk::program::{invoke, invoke_signed}` | `solana_cpi` |
| `crate::msg!`, `crate::entrypoint!` | `solana_msg::msg!`, `solana_program_entrypoint::entrypoint!` |
| `Rent::get()` | `solana_rent::Rent` + трейт `solana_sysvar::Sysvar` |
| `crate::system_program::instruction` | `solana_system_interface::instruction` |
| `runtime::Ledger` | LiteSVM или `solana-test-validator` |
| `client::{Keypair, Transaction}` | `solana_keypair`, `solana_transaction` |

Сознательные упрощения:

- в `AccountInfo` данные — `&mut Vec<u8>`, а владелец читается методом `owner()`.
  Настоящий SDK меняет длину данных и владельца `unsafe`-записью прямо в память
  рантайма, а здесь во всём проекте нет `unsafe` (`#![forbid(unsafe_code)]`);
- программа — обычная Rust-функция, а не байткод SBF; нет compute units,
  `recent_blockhash` и компактного формата сообщений;
- из sysvar есть только `Rent`, и он всегда со значениями по умолчанию.

## Сверено с настоящими крейтами

Отдельной программой проект сравнён с `solana-pubkey` 4.4, `solana-rent` 4.5,
`solana-system-interface` 3.3 и `solana-keypair` 4.0:

- PDA и bump — 3000 случайных случаев совпали байт в байт; три эталона вшиты
  в тест `pda_matches_real_solana_vectors`;
- base58-адреса, включая разбор при компиляции;
- rent-минимум для размеров от 0 до 20 000 байт;
- байты и флаги аккаунтов инструкций `transfer` и `create_account`;
- файлы ключей читаются в обе стороны, подписи совпадают;
- дискриминатор `sha256("account:Vault")[..8]` — такой же, как у Anchor.

## Упражнения

После каждой правки кода: `cargo test` — проверить, `cargo install --path .` —
обновить команду `piggy`.

1. **Сломайте Deposit.** Замените CPI на прямое списание
   `**accs.depositor.try_borrow_mut_lamports()? -= amount;` (и `+= amount` копилке).
   Рантайм ответит `instruction spent from the balance of an account it does not own`.
2. **Уберите проверку владельца** в `load_owned_vault`. Какой тест упадёт и что
   смог бы сделать Боб?
3. **Заимствование через CPI.** В `process_deposit` перед `invoke(…)` возьмите
   `let _data = accs.vault.try_borrow_mut_data()?;` и получите `AccountBorrowFailed`.
   Исправьте, ограничив заимствование блоком `{ … }`.
4. **Новое поле.** Добавьте в `Vault` поле `deposit_count: u32`: поправьте `SIZE`,
   разбор в `show` и тесты. Старые копилки из `~/.config/piggy` перестанут читаться —
   так в Solana возникает тема миграции аккаунтов.
5. **WithdrawAll.** Инструкция «снять всё доступное», команда CLI и тест.
6. **Лимит.** Владелец задаёт `max_withdraw` при создании; превышение — новая
   ошибка `VaultError::LimitExceeded` (6004). Не забудьте про `VaultError::ALL`.
7. **Таймлок.** Добавьте в `sdk` sysvar `Clock` по образцу `Rent::get()` и запретите
   снятие до `unlock_at`.
8. **Своя программа.** По образцу `thief` из тестов напишите программу, которая через
   CPI вызывает `Deposit`, и подключите её через `Ledger::add_program`.

## Куда дальше

- Настоящий тулчейн (Rust, Solana CLI, Anchor): <https://solana.com/docs/intro/installation>
- Anchor — те же идеи, но разбор и проверки аккаунтов генерируют макросы
  `#[derive(Accounts)]` и `#[account]`: <https://www.anchor-lang.com/docs>
- Pinocchio — нативные программы без внешних зависимостей, с ручной работой с байтами,
  как в `system_program.rs`: <https://github.com/anza-xyz/pinocchio>
- LiteSVM — быстрые тесты настоящих программ, API почти как у `Ledger`:
  <https://github.com/LiteSVM/litesvm>

Ключи в `~/.config/piggy/wallets` — в том же формате, что у `solana-keygen`, и, как у него,
лежат открытым текстом. Это ключи для песочницы: для настоящих денег их не используйте.
