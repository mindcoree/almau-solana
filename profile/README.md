# Неделя 6 — on-chain профиль на PDA

**Автор:** Медиханов Тимур

Децентрализованный профиль пользователя на Anchor 1.2 в Devnet. Данные разложены по двум PDA:

| Аккаунт | Сиды | Что хранит |
|---|---|---|
| `Profile` (401 байт) | `["profile", owner]` | владелец, `username`, `bio`, `url`, даты создания и изменения |
| `UsernameRecord` (41 байт) | `["username", username]` | владелец имени — реестр, благодаря которому имена уникальны |

Профиль находится и по кошельку, и по имени — адрес вычисляет любой клиент из сидов, базы данных нет.

| Инструкция | Что делает |
|---|---|
| `create_profile(username, bio, url)` | создаёт профиль и занимает имя; занятое имя — ошибка «already in use» |
| `update_profile(bio, url)` | меняет описание и ссылку — только владелец |
| `close_profile()` | закрывает оба аккаунта, возвращает rent, освобождает имя |

Имя — 3–32 символа: строчные латинские буквы, цифры и `_` (ошибка `InvalidUsername`, 6000).
Описание — до 200 байт (`BioTooLong`, 6001), ссылка — до 100 байт (`UrlTooLong`, 6002).

## Программа в Devnet

| | |
|---|---|
| Program ID | [`EuErHgT1GbzogqFpXhCdce5M9U1bAxZc8QwmcvkVLSuP`](https://explorer.solana.com/address/EuErHgT1GbzogqFpXhCdce5M9U1bAxZc8QwmcvkVLSuP?cluster=devnet) |
| Upgrade authority (мой кошелёк) | `8sJ49ZVUw8QXfxogwHRkXC1nfm6ZT7vrTWwq6xztBkVV` |
| Размер программы | 172 544 байта |
| Профиль @timur | [`4NJrgHDt6YDgKdPvw48q8Q8yhkVRMrbMphaqHT15rbnS`](https://explorer.solana.com/address/4NJrgHDt6YDgKdPvw48q8Q8yhkVRMrbMphaqHT15rbnS?cluster=devnet) |
| Запись имени «timur» | [`8pGxSpJtUKqVbnu5n1rGZcEQzJo2vBJSMHDcw5ZrrgdT`](https://explorer.solana.com/address/8pGxSpJtUKqVbnu5n1rGZcEQzJo2vBJSMHDcw5ZrrgdT?cluster=devnet) |

| Действие | Транзакция |
|---|---|
| create_profile @timur | [2hnCCp3b…](https://explorer.solana.com/tx/2hnCCp3b95upwosqPXkbGhLpMQu2ggp9fe8Cr2E3bDkobvrNPvrD1vqXAusB7JvUh84zRqSRaSUDUoKMiknLc6dc?cluster=devnet) |
| update_profile @timur | [4de9wdH5…](https://explorer.solana.com/tx/4de9wdH5bTuP5YYgaGhUeFFViRUAovbiaLX6FQXrF4263wVCNum9anMqS4G3kkBauTwRYdWy3891n6Z4oonHh8mo?cluster=devnet) |
| create_profile @sol_friend (второй кошелёк) | [21Uz2QBU…](https://explorer.solana.com/tx/21Uz2QBUoHkbbM3M5Kxtx2xtWaxywT3UZLaeGNTqjn4y6vjaz9oYZURxzdSxTsEYXnayLq9H8bhgnDXzRZpucFc3?cluster=devnet) |
| close_profile @sol_friend | [3ra57gVo…](https://explorer.solana.com/tx/3ra57gVo98zkRjECRijib5LMdyK2zfZEQyHB4tDq1P1GaaD9ZuaMSoHwYmdu3skgeYfDKNQayzcQjcAc2LwVwa6E?cluster=devnet) |

## Как повторить

Команды сборки ниже выполняются из `profile/`. Для собственного сценария
создайте основной кошелёк и пополните его тестовыми SOL:

```bash
mkdir -p keys
solana-keygen new --outfile keys/wallet.json
solana airdrop 2 --keypair keys/wallet.json --url devnet
```

Клиент сам создаёт второй кошелёк `keys/friend.json`, если его нет.
Демонстрационное имя `timur` может быть занято: для нового профиля выберите
своё имя в `client/profile.mjs`. Ключи не хранятся в репозитории.
Для деплоя своей копии нужны собственный `keys/profile-keypair.json`
и совпадающий Program ID в `declare_id!`, `Anchor.toml` и `idl/profile.json`.

```bash
anchor build                     # → target/deploy/profile.so и target/idl/profile.json
cp target/idl/profile.json idl/  # клиент берёт IDL отсюда

cargo test                       # 7 тестов в LiteSVM (нужен Rust ≥ 1.97.1)

# Деплой. Адрес программы — keys/profile-keypair.json (скопируйте в target/deploy/)
solana program deploy target/deploy/profile.so \
  --program-id keys/profile-keypair.json --url devnet --use-rpc

cd client
npm ci
node profile.mjs                 # демо: создать, обновить, уникальность имени, поиск, список, удаление
node profile.mjs show timur      # найти профиль по имени
node profile.mjs list            # все профили
node profile.mjs update "описание" "https://…"
```

## Что где лежит

```text
programs/profile/src/lib.rs             программа
programs/profile/tests/test_profile.rs  7 тестов в LiteSVM на том же profile.so
idl/profile.json                        IDL — описание программы для клиента
client/profile.mjs                      клиент на @anchor-lang/core
keys/                                   ключ программы и кошельки — в .gitignore
```

[Общий список работ](../README.md) · [Отчёт Word](../reports/week-06-pda-profile.docx)
