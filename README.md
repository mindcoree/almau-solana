# AlMaU Solana — практические работы

**Автор:** Медиханов Тимур

**Специальность:** Data Science

**Год:** 2026

Учебный репозиторий по разработке на Solana: от локальной CLI-копилки на Rust
до программ в Devnet, собственного токена TMR и каркаса токенового vault.
Здесь собраны исходный код, клиенты на JavaScript, тесты и шесть отчётов
с результатами работ.

Проект `piggy` в корне моделирует устройство Solana локально. Каталоги `program/`,
`counter/`, `profile/`, `token/` и `vault/` содержат следующие практические работы,
связанные с настоящей сетью Solana Devnet.

## Выполненные задания

| Работа | Реализованный результат | Где смотреть код | Отчёт Word |
|---|---|---|---|
| Анализ транзакции Devnet | Разобран перевод 1 SOL: аккаунты, инструкции, логи, изменения балансов и комиссия | [Ссылка на транзакцию](https://explorer.solana.com/tx/59eALrjdiNyPxoms24wnER4TBeDUd5rZJ1D5iGnz3JLWnbFMqZqGhY1gDTTMUrFYAYxuRGgbrSkanjfxzSAHJPT4?cluster=devnet) | [Отчёт](reports/solana-devnet-transaction-analysis.docx) |
| Неделя 3 — Rust для Solana | CLI-копилка с кошельками, PDA, Borsh, CPI, проверкой подписей и прав доступа | [src/](src/) · [Подробное руководство](docs/piggy-vault.md) | [Отчёт](reports/week-03-rust-piggy-vault.docx) |
| Неделя 4 — первая Solana Program | Копилка перенесена на официальный SDK, собрана в SBF и развёрнута в Devnet | [program/](program/README.md) | [Отчёт](reports/week-04-first-solana-program.docx) |
| Неделя 5 — Anchor Counter | Счётчик пользователя на PDA: создание, увеличение и уменьшение, IDL и клиент | [counter/](counter/README.md) | [Отчёт](reports/week-05-anchor-counter.docx) |
| Неделя 6 — профиль на PDA | Профиль с уникальным именем, поиском по кошельку и имени, обновлением и закрытием | [profile/](profile/README.md) | [Отчёт](reports/week-06-pda-profile.docx) |
| Неделя 7 — Token-2022 | Выпущен Timur Token (TMR) с метаданными и комиссией за перевод; выполнены операции через клиент | [token/](token/README.md) | [Отчёт](reports/week-07-token-2022-tmr.docx) |
| Неделя 8 — каркас vault | Настройки, share-mint, хранилище и позиции пользователей на PDA; деплой ожидает Devnet SOL | [vault/](vault/README.md) | [Схема и защита](vault/README.md#схема-аккаунтов) |

### Неделя 3 — локальная копилка Piggy

Реализованы создание кошельков Ed25519, начисление учебных SOL, вычисление PDA,
создание, пополнение, снятие и закрытие копилки. Пополнить её может любой
пользователь; снимать деньги и закрывать аккаунт может владелец.

В проекте есть собственный мини-SDK, System Program и рантайм с проверкой
подписей, атомарным исполнением транзакций и проверкой владения аккаунтами.
Данные инструкций и аккаунтов сериализуются через Borsh, создание и пополнение
копилки выполняются через CPI. CLI показывает логи и структуру данных по байтам.

В Rust использованы `struct`, `enum`, `match`, трейты, `Result`, `Option`,
времена жизни, `Rc<RefCell<…>>`, макросы, модули и тесты.
Состояние хранится локально в JSON; SOL этого симулятора существуют внутри него.

### Неделя 4 — нативная Solana Program

Логика копилки перенесена на официальные крейты Solana. Реализованы инструкции
`Initialize`, `Deposit`, `Withdraw` и `Close`, проверки подписанта и PDA,
сохранение rent-резерва и возврат средств при закрытии.
Программа собрана в SBF и развёрнута в Devnet. Клиент `program/client/demo.mjs`
формирует инструкции, отправляет транзакции и читает состояние копилки.

### Неделя 5 — счётчик на Anchor

Создана программа с инструкциями `initialize`, `increment` и `decrement`.
Каждому владельцу соответствует PDA `["counter", authority]`.
Anchor проверяет подпись, адрес и владельца аккаунта; арифметика защищена
от переполнения и ухода ниже нуля. Добавлены IDL, клиент на `@anchor-lang/core`
и пять сценариев тестирования в LiteSVM.

### Неделя 6 — on-chain профиль

Созданы два аккаунта: `Profile` с данными пользователя и `UsernameRecord`
с владельцем уникального имени. Профиль можно найти по кошельку или имени,
обновить и закрыть с возвратом rent и освобождением имени.

Имя ограничено 3–32 символами (`a–z`, `0–9`, `_`), описание — 200 байтами,
ссылка — 100 байтами. Изменять профиль может владелец.
Добавлены IDL, JavaScript-клиент и семь сценариев LiteSVM.

### Неделя 7 — Timur Token на Token-2022

Создан токен TMR с шестью знаками после запятой и расширениями
`TransferFeeConfig`, `MetadataPointer` и `TokenMetadata`.
Клиент выполняет создание mint и ATA, выпуск, переводы, сбор удержанных комиссий,
сжигание и передачу прав управления токеном.

В демонстрации из отчёта выпущено 1 000 000 TMR и сожжено 10 000 TMR;
итоговый supply составил 990 000 TMR. Комиссия за перевод — 1%,
максимум 1 000 TMR. Операции выполняет программа Token-2022,
а инструкции собирает клиент из `token/client/token.mjs`.

### Неделя 8 — каркас токенового vault

Реализованы `create_vault` и `open_position` на Anchor. PDA настроек vault
управляет выпуском отдельного share-токена и хранилищем принимаемого TMR.
Карточка пользователя вычисляется по vault и его кошельку. При создании
балансы и счётчики равны нулю; депозиты и выводы относятся к следующим этапам.
Восемь тестов LiteSVM проверяют создание аккаунтов и ограничения доступа,
включая совместимость с расширениями TMR из недели 7.

Схема аккаунтов, seeds, клиент, команды и ответы для рубежного контроля —
в [README недели 8](vault/README.md).

## Структура репозитория

```text
almau-solana/
├── README.md              # общий обзор и список выполненных работ
├── Cargo.toml             # локальная CLI-копилка Piggy
├── Cargo.lock
├── src/
│   ├── main.rs            # CLI
│   ├── client.rs          # ключи, транзакции и работа с суммами
│   ├── runtime.rs         # локальная модель исполнения транзакций
│   ├── system_program.rs  # создание аккаунтов и переводы
│   ├── sdk/               # учебные типы и функции Solana
│   └── vault/             # инструкции, состояние, ошибки и логика копилки
├── tests/                 # интеграционные тесты локальной копилки
├── program/               # неделя 4: нативная Solana Program и клиент
├── counter/               # неделя 5: Anchor Counter, IDL, клиент и тесты
├── profile/               # неделя 6: Anchor Profile, IDL, клиент и тесты
├── token/                 # неделя 7: клиент Token-2022
├── vault/                 # неделя 8: Anchor vault, IDL, клиент и тесты
├── docs/
│   └── piggy-vault.md     # подробное руководство и разбор устройства Piggy
└── reports/
    ├── README.md          # каталог отчётов и соответствие исходным именам
    ├── solana-devnet-transaction-analysis.docx
    ├── week-03-rust-piggy-vault.docx
    ├── week-04-first-solana-program.docx
    ├── week-05-anchor-counter.docx
    ├── week-06-pda-profile.docx
    └── week-07-token-2022-tmr.docx
```

Каждый подпроект сохраняет свой `README.md`, зависимости и команды сборки.
Отчёты доступны в [каталоге reports/](reports/README.md).

## Быстрый запуск локальной копилки

Для Piggy достаточно Rust и Cargo. Проект использует Rust edition 2024;
при подготовке репозитория проверен на Rust 1.98.1.
Solana CLI и подключение к Devnet для запуска симулятора не требуются.

```bash
git clone https://github.com/mindcoree/almau-solana.git
cd almau-solana

cargo test --locked
cargo run --locked -- --help
```

Пример полного цикла с отдельной временной папкой данных, macOS/Linux:

```bash
export PIGGY_HOME="$(mktemp -d)"

cargo run --locked -- keygen alice
cargo run --locked -- keygen bob
cargo run --locked -- airdrop alice 10
cargo run --locked -- airdrop bob 5
cargo run --locked -- init alice --deposit 2
cargo run --locked -- deposit bob 1 --to alice
cargo run --locked -- withdraw alice 0.5
cargo run --locked -- show alice
cargo run --locked -- close alice
cargo run --locked -- wallets
```

Можно установить CLI командой `cargo install --locked --path .`
и далее выполнять `piggy <команда>`. Без `PIGGY_HOME` данные хранятся
в `~/.config/piggy`; другой каталог задаётся флагом `--home`.
Полный список команд, устройство аккаунтов и примеры ошибок —
в [руководстве Piggy](docs/piggy-vault.md).

## Работа с Devnet

Для сетевых работ нужны Solana CLI (`solana-keygen`, `cargo build-sbf`),
Node.js и npm. Для `anchor build` также требуется Anchor CLI 1.2.0,
указанный в `counter/Anchor.toml`, `profile/Anchor.toml` и `vault/Anchor.toml`.
Интеграционные тесты LiteSVM требуют Rust 1.97.1 или новее
и предварительно собранный SBF-байткод соответствующего подпроекта:
`target/deploy/counter.so`, `profile.so` или `vault.so`.

Приватные ключи хранятся локально в `keys/` соответствующего подпроекта.
После клонирования создайте собственный кошелёк; пример для нативной копилки:

```bash
cd program
mkdir -p keys
solana-keygen new --outfile keys/wallet.json
solana airdrop 2 --keypair keys/wallet.json --url devnet

cd client
npm ci
node demo.mjs
```

Кошелёк нужно пополнить тестовыми SOL; доступность airdrop зависит от лимитов
Devnet. Собственный кошелёк позволяет вызвать опубликованную программу.
Обновление существующей программы требует ключа её upgrade authority.
Для развёртывания своей копии создайте собственный ключ программы
и согласуйте новый Program ID в коде, конфигурации и IDL.

Команды установки клиентских зависимостей выполняются в нужном каталоге:
`program/client`, `counter/client`, `profile/client`, `token/client` или `vault/client`.
Используйте `npm ci`, чтобы установить версии из `package-lock.json`.
Подробные инструкции и транзакции приведены в README подпроектов:
[копилка](program/README.md), [счётчик](counter/README.md),
[профиль](profile/README.md), [токен](token/README.md), [vault](vault/README.md).

В демо токена основной получатель задан константой `MY_WALLET`
в `token/client/token.mjs`. Для собственного сценария задайте свой адрес
перед запуском и создайте локальные `keys/wallet.json` и `keys/friend.json`.
Новый mint клиент создаёт при отсутствии `keys/mint.json`.

## Адреса из выполненных работ

| Объект | Адрес в Solana Explorer, Devnet |
|---|---|
| Нативная копилка | [DiN5bMhqftannN64aRSfAT1dRdavM5xaqgS9GmJrex9c](https://explorer.solana.com/address/DiN5bMhqftannN64aRSfAT1dRdavM5xaqgS9GmJrex9c?cluster=devnet) |
| Anchor Counter | [HN2SRai9T3z7Rsw2avdz7Ns5tewqHctg9JeTeiSU6jTW](https://explorer.solana.com/address/HN2SRai9T3z7Rsw2avdz7Ns5tewqHctg9JeTeiSU6jTW?cluster=devnet) |
| Anchor Profile | [EuErHgT1GbzogqFpXhCdce5M9U1bAxZc8QwmcvkVLSuP](https://explorer.solana.com/address/EuErHgT1GbzogqFpXhCdce5M9U1bAxZc8QwmcvkVLSuP?cluster=devnet) |
| Mint TMR | [Cq9bsK12BiRunwpBNhGG9K8Di9QX4G2xfCcoG7TmmU31](https://explorer.solana.com/address/Cq9bsK12BiRunwpBNhGG9K8Di9QX4G2xfCcoG7TmmU31?cluster=devnet) |

Адреса, supply и результаты демонстраций зафиксированы в исходных отчётах.
Состояние аккаунтов и доступность истории Devnet могут измениться.

## Проверки перед публикацией

12 октября 2026 года выполнены:

- `cargo test --locked` в корне — **34 теста прошли**: 21 модульный,
  11 интеграционных и 2 теста документации.
- `cargo test --locked` в `program/` — **8 тестов прошли**.
- Проверка синтаксиса четырёх JavaScript-клиентов через `node --check`.
- Проверка полного локального сценария Piggy и ссылок на файлы в документации.
- Проверка идентичности шести опубликованных отчётов исходным документам.
- Для недели 8: сборка SBF `vault`, **8 тестов LiteSVM**, генерация IDL
  и полный сценарий нового JavaScript-клиента на локальном валидаторе.

Повторная сборка SBF и тесты Counter/Profile в LiteSVM при подготовке
этой публикации не выполнялись.
Результаты этих работ представлены в отчётах и исходных тестах подпроектов.

## Какие файлы хранятся только локально

`.gitignore` исключает приватные ключи и кошельки, локальный `ledger.json`,
`.env`, каталоги `target/` и `node_modules/`, служебные файлы macOS
и исходные ZIP-архивы. Локальная папка `archives/` сохраняет старые архивы;
исходники и отчёты опубликованы отдельными файлами.
