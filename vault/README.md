# Недели 8–9 — токеновый vault

**Автор:** Медиханов Тимур

Anchor-программа создаёт настройки vault, новый share-токен, хранилище
принимаемых токенов и отдельные позиции пользователей. Настройки vault
одновременно служат PDA authority для share-mint и хранилища.

В неделе 8 реализованы `create_vault` и `open_position`; в неделе 9 добавлен
`deposit(amount)`. После создания счётчики и балансы равны нулю, а депозит
переводит токены пользователя в хранилище и выпускает ему shares.
Формат аккаунтов и Program ID сохранены: используются vault и позиции недели 8.

**Статус Devnet:** программа опубликована 12 октября 2026 года; созданы
vault, share-mint, хранилище TMR и позиции двух пользователей.
Адреса и подтверждённые транзакции — в разделе [Devnet и Explorer](#devnet-и-explorer).

## Неделя 9: депозит и share-токены

Реализована инструкция `deposit(amount)` на существующем каркасе:

1. Пользователь подписывает CPI `transfer_checked` из своего токен-аккаунта в `token_vault`.
2. Программа перечитывает баланс хранилища и определяет фактически полученную сумму.
3. Vault PDA подписывает CPI `mint_to` и выпускает shares в аккаунт пользователя.
4. Обновляются `VaultConfig.total_deposited` и `UserPosition.deposited_amount`.

```text
received = баланс после перевода − баланс до перевода
первый депозит: shares = received
следующие: shares = floor(received × share_mint.supply / баланс до перевода)
```

Расчёт использует `u128` и проверяемое преобразование в `u64`.
Баланс и supply читаются из токеновых аккаунтов: если vault заработал,
поздний вкладчик получает долю по новой цене. Например, при 1000 shares
и 1100 токенах поступление 110 токенов выпускает 100 shares.

У TMR из недели 7 комиссия Token-2022 — 1%. При переводе 100 TMR хранилищу
доступно 99 TMR, а 1 TMR удерживается расширением `TransferFeeAmount`.
Первый вкладчик получает 99 shares; удержанная сумма не обеспечивает доли.
Это комиссия самого принимаемого токена, дополнительная комиссия vault не добавлена.

`total_deposited` после депозита отражает доступный баланс хранилища, включая
доход, поступивший до этого депозита. Карточка пользователя накапливает только
его фактические поступления. Shares — обычные переводимые токены; карточка
фиксирует внесённую сумму и не ограничивает передачу shares.

Нулевой депозит, нулевой результат после комиссии и выпуск нуля shares запрещены.
PDA, mint, Token Program и authority проверяются у всех переданных аккаунтов.
Любая ошибка, включая переполнение счётчика после CPI, откатывает всю транзакцию.

### Команда депозита

```bash
cd vault/client
npm ci
node vault.mjs deposit 100000000  # 100 TMR при decimals=6
node vault.mjs show
```

Сумма задаётся целым числом минимальных единиц. Клиент использует существующий
`vault/keys/wallet.json`, ATA принимаемого TMR и создаёт ATA shares при необходимости.
Если карточка подписанта ещё не открыта, сначала вызывает `open_position`.
Для другого пользователя задайте `WALLET=/путь/к/кошельку.json`; выбранный vault
по умолчанию остаётся vault администратора недели 8.

### Проверки недели 9

22 октября 2026 года после сборки прошли **14 тестов LiteSVM**: восемь прежних
проверок каркаса и шесть новых сценариев депозитов:

- Три пользователя вносят токены; shares и счётчики проверяются для SPL Token и Token-2022.
- После дохода 100 токенов депозит 110 выпускает 100 shares по примеру методички.
- Для снимка TMR комиссия исключается из shares и учёта вклада; полностью удержанный депозит откатывается.
- Нулевой депозит и недостаточный баланс не меняют аккаунты vault и токенов.
- Подмена источника, получателя shares и карточки другого пользователя отклоняется.
- Нулевой выпуск shares и переполнение карточки откатывают уже выполненный перевод.

Также проверяется корректность аргументов CLI: ноль, отрицательная, дробная
и превышающая `u64` сумма не отправляются в сеть.

### Devnet: выполненные депозиты недели 9

22 октября 2026 года обновлена существующая программа
`J6x4mnjWyfsAYVVd1zNSfJwf9FG1k1LNwzjkoodkKmnN`. Использованы vault, share-mint, хранилище и карточка
администратора, созданные в неделе 8. Миграция аккаунтов не потребовалась.

| Действие | Отправлено TMR | Поступило TMR / выпущено shares | Транзакция |
|---|---:|---:|---|
| Первый депозит | 100 | 99 / 99 | [Explorer](https://explorer.solana.com/tx/nef6HbQdLqXLJv7zfQAG994WhECBBMdhP9mAQp4ZE9672L36rTNQEnCWMn3gXKZTzH8aH8ckq1cFRpnwpMAG37z?cluster=devnet) |
| Повторный депозит | 50 | 49,5 / 49,5 | [Explorer](https://explorer.solana.com/tx/Ssha4ABzFMKi9tKQ1rP8XLt2AVPke8FCdHaf7qH5i1LrxZWsToapWFk7j2VRSzoq3rBHLsmKjBKHUzNp2nkhajK?cluster=devnet) |

После обоих депозитов `total_deposited`, `deposited_amount`, доступный баланс
хранилища, supply и баланс shares пользователя равны **148,5 токена**.
В расширении хранилища удержано **1,5 TMR** комиссии Token-2022.
Позиция второго пользователя недели 8 осталась доступной и имеет нулевой вклад.

[Обновление программы](https://explorer.solana.com/tx/epvuewZpkcCKgVWhv3AKfLGDDvgvAwovANzsvrCyjWS3sGcjW7NqVaRp59aeQE9RGx1yp64Qu2EQmgRcGV4jJ5x?cluster=devnet) и оба депозита проверены
через RPC: статус `finalized`, ошибок нет. Опубликованный байткод размером
252544 байт побайтно совпадает с протестированной сборкой.
Опубликованный IDL также совпадает с [локальным IDL](idl/vault.json).

В депозитной транзакции участвуют программа vault и Token-2022.
В `inner instructions` видны `TransferChecked` с подписью пользователя
и `MintTo` с подписью PDA vault. Создание ATA shares клиент выполняет отдельной
транзакцией при первом запуске.

Адреса, размеры аккаунтов, состояния, слоты, транзакции и SHA-256 сохранены
в [снимке проверки недели 9](deployment-week9.json).
Проверены Anchor 1.2.0, Agave 4.2.2, cargo-build-sbf 4.4.0 / platform-tools 1.57,
Rust 1.99.1 для тестов и Node.js 25.6.1 для клиента.

## Связь с предыдущими неделями

В неделе 6 уже использовались PDA и отдельные аккаунты пользователей.
В неделе 7 был создан принимаемый токен TMR на Token-2022. Эти работы дают
основу для vault, однако mint TMR управлялся кошельком, а настроек vault,
share-mint и позиций в тех проектах не было. Здесь они объединены
в самостоятельную программу в каталоге `vault/`.

В демонстрации принимается существующий TMR. Share-токен создаётся отдельно:
он представляет долю пользователя в vault и выпускается только
через подпись PDA программы.

## Схема аккаунтов

```mermaid
flowchart TD
    A[Администратор: подписывает create_vault] --> V[VaultConfig PDA: настройки и total_deposited]
    M[Существующий mint TMR] --> T[Token vault PDA: хранилище TMR]
    V -->|mint authority| S[Share-mint PDA: токен-квитанция]
    V -->|token authority| T
    U[Пользователь: подписывает open_position] --> P[UserPosition PDA: owner и deposited_amount]
    P -->|принадлежит vault| V
    R[Программа vault] -->|владеет данными| V
    R -->|владеет данными| P
    SPL[Token-2022 Program] -->|владеет данными| S
    SPL -->|владеет данными| T
```

| Аккаунт | Что хранится | Программа-владелец данных (`AccountInfo.owner`) | Кто управляет |
|---|---|---|---|
| `VaultConfig` | `admin`, `asset_mint`, `share_mint`, `token_vault`, `total_deposited`, `bump` | Программа vault | Логика vault; администратор подписывает создание |
| `share_mint` | Decimals как у принимаемого mint, supply, mint authority | Выбранная SPL Token Program | Mint authority — PDA `VaultConfig`; freeze authority отсутствует |
| `token_vault` | Принимаемый mint, баланс, token authority | Выбранная SPL Token Program | Token authority — PDA `VaultConfig`; delegate и отдельной close authority нет |
| `UserPosition` | `vault`, `owner`, `deposited_amount`, `bump` | Программа vault | Пользователь подписывает создание собственной позиции |

Владение данными и право распоряжаться токенами — разные поля. Token-2022
владеет данными mint и token account, а для выпуска и перевода проверяет
authority. В каркасе этой authority назначена PDA vault, у которой нет
приватного ключа. Администратор не может подписать прямую операцию SPL Token
от её имени.

Сохраняется upgrade authority программы: она позволяет обновлять код.
Поэтому права PDA защищают от прямых операций кошелька, а доверие к будущим
обновлениям зависит от управления upgrade authority.

### Адреса PDA и размеры

Все PDA вычисляются с Program ID vault и каноническим bump:

| Аккаунт | Seeds | Размер |
|---|---|---|
| Настройки | `["vault", admin, asset_mint]` | 145 байт: 8 discriminator + 137 данных |
| Share-mint | `["share_mint", vault]` | 82 байта для mint без расширений |
| Хранилище | `["token_vault", vault]` | 178 байт для TMR, включая `TransferFeeAmount`; 165 для обычного SPL Token |
| Позиция | `["position", vault, user]` | 81 байт: 8 discriminator + 73 данных |

Один администратор может создать по одному vault для каждого принимаемого
mint. Повторное создание той же пары отклоняется. У пользователя одна
позиция в конкретном vault; адреса позиций разных пользователей различаются.

## Неделя 8: выполненные пункты каркаса

| Требование | Реализация |
|---|---|
| Схема аккаунтов и их владельцев | Диаграмма, таблица владельцев и seeds в этом README |
| Один раз создать настройки, share-токен и хранилище | `create_vault`: три аккаунта создаются атомарно через `init` |
| Только программа выпускает share-токен | `mint::authority = vault`; приватного ключа этой authority нет |
| Программа распоряжается хранилищем | `token::authority = vault`; прав кошелька администратора нет |
| Карточка пользователя по его адресу | `open_position`: PDA из vault и подписавшего пользователя |
| Деплой и аккаунты в Devnet | Программа опубликована; vault, share-mint, хранилище и две позиции созданы и проверены через RPC |
| Короткая защита | Ответы на контрольные вопросы приведены ниже |

Исходники инструкций и структур: [programs/vault/src/lib.rs](programs/vault/src/lib.rs).

## Сборка и проверка

Использованы Anchor 1.2.0, Agave / Solana CLI 4.3.0,
SBF platform-tools 1.57, LiteSVM 0.17.0 и Node.js 25.6.1.
Версии Rust-зависимостей зафиксированы в `Cargo.lock`, клиента —
в `client/package-lock.json`. Для сборки нужен установленный `cargo-build-sbf`.

Из корня репозитория:

```bash
cd vault
cargo build-sbf --manifest-path programs/vault/Cargo.toml --arch v2 -- --locked
cargo test --locked

cd client
npm ci
node --check vault.mjs
```

Перед Rust-тестами обязательно собрать `target/deploy/vault.so`:
LiteSVM исполняет этот SBF-байткод. Сгенерированный IDL находится
в [idl/vault.json](idl/vault.json). При изменении программы обновите его
через Anchor CLI, чтобы клиент соответствовал инструкциям и данным аккаунтов.

12 октября 2026 года прошли **8 интеграционных тестов**:

- Создание пустого vault для SPL Token и Token-2022 с правильными authority.
- Создание хранилища для снимка реального TMR с расширением комиссии.
- Создание отдельных пустых позиций двух пользователей.
- Отклонение повторного создания vault и позиции.
- Отклонение попытки пользователя создать позицию по чужому адресу.
- Отклонение mint, не принадлежащего выбранной Token Program; откат аккаунтов.
- Запрет администратору выпускать share-токены и менять mint authority.
- Запрет администратору менять authority или закрывать хранилище.

Тесты: [programs/vault/tests/test_vault.rs](programs/vault/tests/test_vault.rs).
Происхождение публичного снимка TMR:
[fixtures/README.md](programs/vault/tests/fixtures/README.md).

JavaScript-клиент дополнительно проверен на локальном валидаторе с копией
TMR из Devnet: создание vault, открытие позиций двух пользователей,
повторный запуск `demo` и просмотр без кошелька через `show`.

После деплоя выполнены `demo`, открытие второй позиции и `show` в Devnet.
Через RPC проверены владельцы данных, seeds и bump PDA, authority токеновых
аккаунтов, нулевые балансы и успешность всех четырёх транзакций.
Опубликованный SBF-байткод побайтно совпадает с проверенной сборкой.

## Клиент и воспроизведение

Просмотр опубликованного vault не требует приватного ключа:

```bash
cd vault/client
npm ci
node vault.mjs show
node vault.mjs show АДРЕС_ПОЛЬЗОВАТЕЛЯ
```

Для создания своего vault в опубликованной программе создайте кошелёк
в игнорируемой папке `vault/keys/` и пополните его Devnet SOL:

```bash
cd vault
mkdir -p keys
solana-keygen new --outfile keys/wallet.json
solana airdrop 2 --keypair keys/wallet.json --url devnet

cd client
node vault.mjs demo
```

`demo` создаёт vault и позицию подписанта, затем читает аккаунты и проверяет
authority. Повторный запуск читает существующие аккаунты. Новый кошелёк
создаёт собственный vault для того же TMR.

Открыть позицию другого пользователя в vault из демонстрации:

```bash
cd vault/client
WALLET=/полный/путь/к/кошельку.json node vault.mjs open
```

Пользователь оплачивает rent своей позиции и подписывает транзакцию.
Переменные окружения клиента: `WALLET` — файл подписанта, `ADMIN` —
администратор выбранного vault, `ASSET_MINT` — принимаемый mint,
`RPC_URL` — адрес RPC. По умолчанию используются Devnet и TMR из недели 7.
Для собственного vault при `show` или `open` укажите его `ADMIN`.

### Деплой собственной копии

Создайте отдельный ключ программы и согласуйте его адрес в `declare_id!`,
`Anchor.toml` и IDL, затем пересоберите программу. Ключ программы из этой
демонстрации хранится только локально.

```bash
cd vault
solana-keygen new --outfile keys/vault-keypair.json
solana address --keypair keys/vault-keypair.json
# Обновите Program ID, пересоберите SBF и IDL.
solana-keygen new --outfile keys/buffer-keypair.json
solana program deploy target/deploy/vault.so \
  --program-id keys/vault-keypair.json \
  --buffer keys/buffer-keypair.json \
  --keypair keys/wallet.json --url devnet --use-tpu-client
```

Для деплоя нужны Devnet SOL на rent байткода и комиссии; временный buffer
тоже требует rent до завершения деплоя. Если RPC airdrop недоступен,
используйте [официальный Devnet faucet](https://faucet.solana.com/).
Файлы `keys/`, `target/` и `node_modules/` исключены из Git.

## Devnet и Explorer

Программа развёрнута 12 октября 2026 года в слоте **510295509**.
Все ссылки ниже открывают Solana Devnet.

| Объект | Адрес в Explorer |
|---|---|
| Программа vault | [J6x4mnjWyfsAYVVd1zNSfJwf9FG1k1LNwzjkoodkKmnN](https://explorer.solana.com/address/J6x4mnjWyfsAYVVd1zNSfJwf9FG1k1LNwzjkoodkKmnN?cluster=devnet) |
| ProgramData с байткодом | [CPkPhPnJszzjeAGNz185bp9mKTHRErsPe3YEHV3qwMRw](https://explorer.solana.com/address/CPkPhPnJszzjeAGNz185bp9mKTHRErsPe3YEHV3qwMRw?cluster=devnet) |
| Администратор / upgrade authority | [8ijYtQTzr7MSqvxxGZpGr9w3FKMkyY1W8pcUBYNymLSi](https://explorer.solana.com/address/8ijYtQTzr7MSqvxxGZpGr9w3FKMkyY1W8pcUBYNymLSi?cluster=devnet) |
| Принимаемый mint TMR из недели 7 | [Cq9bsK12BiRunwpBNhGG9K8Di9QX4G2xfCcoG7TmmU31](https://explorer.solana.com/address/Cq9bsK12BiRunwpBNhGG9K8Di9QX4G2xfCcoG7TmmU31?cluster=devnet) |
| Настройки `VaultConfig` | [FovmQp6HZYcLGGBxqCzwiRApoHmmRmAFnMkXkwGJPT35](https://explorer.solana.com/address/FovmQp6HZYcLGGBxqCzwiRApoHmmRmAFnMkXkwGJPT35?cluster=devnet) |
| Share-mint | [EZhrX7Gnxzs3gDMwCmMUUccvTZf2q3KBcGAZ7qqT2BP6](https://explorer.solana.com/address/EZhrX7Gnxzs3gDMwCmMUUccvTZf2q3KBcGAZ7qqT2BP6?cluster=devnet) |
| Хранилище TMR | [4CADSCz6sFS5DDrmQpeET7gd5M3f4HHnWMXAZnLZEiFr](https://explorer.solana.com/address/4CADSCz6sFS5DDrmQpeET7gd5M3f4HHnWMXAZnLZEiFr?cluster=devnet) |
| Позиция администратора | [EeLZf5W1GJ5Q5UW6nxYmxb4h7d2W2r2D2m2ZaN5DNzGw](https://explorer.solana.com/address/EeLZf5W1GJ5Q5UW6nxYmxb4h7d2W2r2D2m2ZaN5DNzGw?cluster=devnet) |
| Позиция второго пользователя `96CkS5…QxTBj1` | [EbSfwjZiaRNDup3juLkoqouBmDHaDdWWjsnsUESU8bSY](https://explorer.solana.com/address/EbSfwjZiaRNDup3juLkoqouBmDHaDdWWjsnsUESU8bSY?cluster=devnet) |

| Действие | Подтверждённая транзакция |
|---|---|
| Деплой программы | [Открыть в Explorer](https://explorer.solana.com/tx/66w5Ad6tsN8FmgUan8hQp6KXfuCfBWHghUZPmbePEdQvnuBNrY4fba4itomzXVatN5ACpMj5pqqxTHJ8JfkP6XbS?cluster=devnet) |
| `create_vault` | [Открыть в Explorer](https://explorer.solana.com/tx/M4vEzSKtTegytLENsSeazGxftSkjsTdQX4e6oT4vE6piibcGTLSi64efkLQnVpqdGXwPPGkLaEVfkfB5MBFjDfA?cluster=devnet) |
| `open_position` администратора | [Открыть в Explorer](https://explorer.solana.com/tx/2DEvtG7BADfuov69e6C5KaBha4gyagADDvSqWWLCpJL1PbWnFZYKuZZXdWB5wvWtMgtZnrCgAziJDQFqpGmmxX8c?cluster=devnet) |
| `open_position` второго пользователя | [Открыть в Explorer](https://explorer.solana.com/tx/4JCME29BwLw551xhcwg4HuukdK3tfyZXaDguL4VeuX85Bqf6AE55SWEGSGBSJrQyCiKBihpt3NgGvm7ABZ3PZKpt?cluster=devnet) |

Снимок недели 8 от 12 октября: все четыре транзакции имеют статус `finalized`
и завершились без ошибок. В этом снимке `total_deposited`, supply share-токена, баланс хранилища, удержанная комиссия
и вклады обеих позиций равны **0**. Временный буфер деплоя закрыт.

Публичный снимок проверки: [deployment.json](deployment.json).
Он содержит адреса, владельцев, authority, bump, слоты, транзакции
и SHA-256 байткода размером 198 384 байта. Состояние зафиксировано
12 октября 2026 года; текущее состояние можно проверить командой `show`.

В неделе 9 депозит учитывает комиссию TMR через изменение доступного баланса
хранилища. Снимок недели 8 выше сохраняется как история исходного каркаса.

## Вопросы для защиты

**Почему share-токен выпускает программа, а не администратор?**

Share-токен должен подтверждать долю пользователя. Если человек может
выпускать его произвольно, он создаст квитанции без соответствующих вкладов.
Поэтому mint authority назначена PDA vault. При депозите выпуск выполняется
через CPI `mint_to` с подписью PDA после перевода и проверки полученной суммы.
Сразу после `create_vault` supply равен нулю; депозит увеличивает его.

**Что такое PDA и почему у неё нет приватного ключа?**

Program Derived Address вычисляется из seeds, Program ID и bump.
Solana подбирает адрес вне кривой Ed25519, поэтому обычная подпись приватным
ключом для него невозможна. Рантайм разрешает программе подписать CPI
за свой PDA через `invoke_signed`, проверяя совпадение seeds и адреса.

**Как найти карточку конкретного пользователя?**

Вычислить PDA по известным vault, кошельку пользователя и Program ID:

```javascript
const [position] = web3.PublicKey.findProgramAddressSync(
  [Buffer.from('position'), vault.toBuffer(), user.toBuffer()],
  program.programId,
);
```

Список всех пользователей для этого не нужен. Затем аккаунт можно прочитать
через `program.account.userPosition.fetch(position)` или RPC `getAccountInfo`.

**Зачем нужны четыре аккаунта?**

`VaultConfig` связывает настройки и хранит общий счётчик. Share-mint задаёт
токен-квитанцию и право выпуска. Token vault хранит принимаемые токены.
`UserPosition` хранит принадлежность и вклад конкретного пользователя.
Первые три создаёт администратор; свою позицию создаёт пользователь.

Официальные пояснения Anchor:
[создание mint и PDA authority](https://www.anchor-lang.com/docs/tokens/basics/create-mint),
[token accounts и их владельцы](https://www.anchor-lang.com/docs/tokens/basics/create-token-account).
