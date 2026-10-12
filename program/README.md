# Неделя 4 — первая Solana Program в Devnet

**Автор:** Медиханов Тимур

Копилка из piggy (неделя 3) — теперь настоящая нативная Solana-программа:
собрана в байткод SBF и задеплоена в Devnet. Логика та же, что в `../src/vault/`:
положить SOL может кто угодно, снять и закрыть — только владелец копилки.

## Программа в Devnet

| | |
|---|---|
| Program ID | [`DiN5bMhqftannN64aRSfAT1dRdavM5xaqgS9GmJrex9c`](https://explorer.solana.com/address/DiN5bMhqftannN64aRSfAT1dRdavM5xaqgS9GmJrex9c?cluster=devnet) |
| Upgrade authority (мой кошелёк) | `8sJ49ZVUw8QXfxogwHRkXC1nfm6ZT7vrTWwq6xztBkVV` |
| Размер программы | 79 512 байт |

Вызовы программы:

| Инструкция | Транзакция |
|---|---|
| Initialize | [2aqseiKD…](https://explorer.solana.com/tx/2aqseiKDsbr7JGX8r1GztZQ7E3osePDUu69VuiaGCnAk5R8RvEmSMgzSUT1hYhdDTow98rrB9ByZmR4DuLBxnRYk?cluster=devnet) |
| Deposit 0.1 SOL | [2hSMjKte…](https://explorer.solana.com/tx/2hSMjKtenLQiQew2GyMgUw5jdGi5mdNSWrgYqxscvrXDKXtmpjTjJ4U2qjkHNSD2Grxg8BVShueNNmD91UDSBFHE?cluster=devnet) |
| Withdraw 0.05 SOL | [3CTbAYry…](https://explorer.solana.com/tx/3CTbAYryXkje2P4E6basMtt9TJFChVvE7hnKFZDg5iP8SNVjgmks72FJGXPWSU5Z4nQPoCXsgJZxkVazmogQLwKM?cluster=devnet) |

Копилка (PDA `["vault", owner]`):
[`FiPxhEek3oc2F14EckmRMGjwzQKJrm6DRuLKc5synfbJ`](https://explorer.solana.com/address/FiPxhEek3oc2F14EckmRMGjwzQKJrm6DRuLKc5synfbJ?cluster=devnet).

Лог Initialize в Devnet — такой же, какой печатал симулятор piggy:

```text
Program DiN5bMhqftannN64aRSfAT1dRdavM5xaqgS9GmJrex9c invoke [1]
Program log: Instruction: Initialize
Program 11111111111111111111111111111111 invoke [2]
Program 11111111111111111111111111111111 success
Program log: Vault FiPxhEek3oc2F14EckmRMGjwzQKJrm6DRuLKc5synfbJ created, bump 254
Program DiN5bMhqftannN64aRSfAT1dRdavM5xaqgS9GmJrex9c consumed 8301 of 200000 compute units
Program DiN5bMhqftannN64aRSfAT1dRdavM5xaqgS9GmJrex9c success
```

## Что поменялось по сравнению с piggy

Логика программы не менялась — только то, чем учебный мини-SDK отличался от настоящего:

- `use crate::sdk::…` → настоящие крейты: `solana-account-info`, `solana-cpi`, `solana-msg`,
  `solana-program-entrypoint`, `solana-program-error`, `solana-pubkey`, `solana-rent`,
  `solana-sysvar`, `solana-system-interface`.
- `account.owner()` → поле `account.owner` (`state.rs`).
- `Rent::get()` — метод трейта `Sysvar`, его нужно импортировать.
- `entrypoint!` принимает только имя функции, а не путь `processor::…`.
- `declare_id!` — адрес из `keys/program-keypair.json`: у придуманного `Vau1tk…` нет ключа.
- Крейт — `cdylib`, собирается `cargo build-sbf`; фича `no-entrypoint` — чтобы подключать
  программу библиотекой.
- `Pubkey` сериализуется Borsh только с фичей `borsh` у `solana-pubkey`.
- `create_program_address` возвращает свою ошибку — переводим в `ProgramError::InvalidSeeds`.

## Как повторить

Команды сборки ниже выполняются из `program/`. Приватные ключи в репозитории
не хранятся. Для вызова опубликованной программы создайте собственный кошелёк:

```bash
mkdir -p keys
solana-keygen new --outfile keys/wallet.json
solana airdrop 2 --keypair keys/wallet.json --url devnet
```

Для нового деплоя также нужен локальный ключ `keys/program-keypair.json`.
Если создаёте свою копию программы, укажите его публичный адрес в `src/lib.rs`;
клиент читает Program ID из этого файла или из переменной `PROGRAM_ID`.

```bash
# один раз: Solana CLI и cargo build-sbf
sh -c "$(curl -sSfL https://release.anza.xyz/stable/install)"

cargo test                     # тесты на обычном Rust
cargo build-sbf                # → target/deploy/piggy_vault.so

# деплой (первый раз). --use-rpc — если запись байткода падает с «Max retries exceeded»
solana program deploy target/deploy/piggy_vault.so \
  --program-id keys/program-keypair.json --url devnet --use-rpc

# вызвать программу: Initialize (если копилки нет) → Deposit 0.1 → Withdraw 0.05
cd client
npm ci
node demo.mjs
node demo.mjs --close          # закрыть копилку и вернуть SOL
```

Обновить уже задеплоенную программу может только upgrade authority — кошелёк
`8sJ49…BkVV` (`solana program deploy … --upgrade-authority <его keypair>`).

## Что где лежит

```text
src/            программа: lib.rs, processor.rs, instruction.rs, state.rs, error.rs
client/demo.mjs клиент на @solana/web3.js: собирает те же байты Borsh и шлёт в Devnet
keys/           program-keypair.json (адрес программы) и wallet.json (кошелёк деплоя);
                в .gitignore — в репозиторий не попадают
```

[Общий список работ](../README.md) · [Отчёт Word](../reports/week-04-first-solana-program.docx)
