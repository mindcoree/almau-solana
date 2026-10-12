# Неделя 5 — Counter Program на Anchor

**Автор:** Медиханов Тимур

Счётчик на Anchor 1.2, задеплоенный в Devnet, и клиент на `@anchor-lang/core`.
У каждого пользователя свой счётчик — PDA `["counter", authority]`; менять его может только владелец.

| Инструкция | Что делает |
|---|---|
| `initialize` | создаёт счётчик со значением 0 (Anchor сам создаёт аккаунт через CPI в System Program) |
| `increment` | +1 |
| `decrement` | −1; ниже нуля — ошибка `Underflow` (6001) |

## Программа в Devnet

| | |
|---|---|
| Program ID | [`HN2SRai9T3z7Rsw2avdz7Ns5tewqHctg9JeTeiSU6jTW`](https://explorer.solana.com/address/HN2SRai9T3z7Rsw2avdz7Ns5tewqHctg9JeTeiSU6jTW?cluster=devnet) |
| Upgrade authority (мой кошелёк) | `8sJ49ZVUw8QXfxogwHRkXC1nfm6ZT7vrTWwq6xztBkVV` |
| Размер программы | 140 232 байта |
| Счётчик (PDA) | [`Fyo5n9KjgDGah4rHv71f9pcr3EPMtwtchEYTkDEn4mD`](https://explorer.solana.com/address/Fyo5n9KjgDGah4rHv71f9pcr3EPMtwtchEYTkDEn4mD?cluster=devnet) |

Вызовы из клиента (`node counter.mjs`):

| Инструкция | count | Транзакция |
|---|---|---|
| initialize | 0 | [64C2akU8…](https://explorer.solana.com/tx/64C2akU8J55g19nYVRpzzesJfE47HAkaw9RKWVtVnx7kPPhnNWNRFY9q4KyAVV1qTqqZ6YVAAgqggPnzMcgzcJxD?cluster=devnet) |
| increment | 1 | [3vVz244H…](https://explorer.solana.com/tx/3vVz244HAPm6JtQKPZSWNuEN9kJf7WrbmHZeA5BBRhZer8A3qRpsM4xJCYuyhfN1HEk27pY5cW31tTxVu45qRD6t?cluster=devnet) |
| increment | 2 | [3FDtSY9E…](https://explorer.solana.com/tx/3FDtSY9EX9JqF4bZRr6zSuGL7d35tZtF9y18QD9d6i7PgiSVQmEshu9yibiteRvtuuornyPATMEvS4Xxbe9m25Fi?cluster=devnet) |
| increment | 3 | [67DbttLK…](https://explorer.solana.com/tx/67DbttLK9PYSKXHukMzTuBga6FJP1w9JMVvcZWGg59xqLX4VnMHK4Z9KEe3b1prSk7P1tKXg1qaxvpHBC9ChzB4e?cluster=devnet) |
| decrement | 2 | [27M94Q48…](https://explorer.solana.com/tx/27M94Q48VWCvdjYdF8Y8WjNjVGr3ev81yiziEWmVEMbuExbLJUDABhkbsqn99Vi6yFTQ3XQBExQTTF4yybFWh6ux?cluster=devnet) |

## Как повторить

Команды сборки ниже выполняются из `counter/`. Для вызова программы после
клонирования создайте собственный кошелёк и пополните его тестовыми SOL:

```bash
mkdir -p keys
solana-keygen new --outfile keys/wallet.json
solana airdrop 2 --keypair keys/wallet.json --url devnet
```

Приватный ключ опубликованной программы не входит в репозиторий.
Для своего деплоя создайте `keys/counter-keypair.json` и согласуйте новый
адрес в `declare_id!`, `Anchor.toml` и `idl/counter.json`.

```bash
# Anchor 1.2 (через avm) и Solana CLI — https://www.anchor-lang.com/docs/installation
anchor build                     # → target/deploy/counter.so и IDL target/idl/counter.json
cp target/idl/counter.json idl/  # клиент берёт IDL отсюда

cargo test                       # 5 тестов в LiteSVM (нужен Rust ≥ 1.97.1)

# Деплой. Адрес программы — keys/counter-keypair.json (скопируйте в target/deploy/)
anchor deploy --provider.cluster devnet
# или напрямую; --use-rpc — если загрузка падает с «Max retries exceeded»
solana program deploy target/deploy/counter.so \
  --program-id keys/counter-keypair.json --url devnet --use-rpc

cd client
npm ci
node counter.mjs                               # initialize (если нужно), +1 ×3, −1
node counter.mjs show                           # показать счётчик
```

Без Anchor CLI то же самое: `cargo build-sbf` собирает программу, а IDL генерирует
библиотека `anchor-lang-idl` (так и было сделано в этой работе).

## Что где лежит

```text
programs/counter/src/lib.rs          программа
programs/counter/tests/test_counter.rs  тесты в LiteSVM на том же counter.so
idl/counter.json                     IDL — описание программы для клиента
client/counter.mjs                   клиент на @anchor-lang/core
Anchor.toml                          настройки Anchor: кластер, кошелёк, адрес программы
keys/                                ключ программы и кошелёк деплоя — в .gitignore
```

[Общий список работ](../README.md) · [Отчёт Word](../reports/week-05-anchor-counter.docx)
