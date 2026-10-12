# Неделя 7 — свой токен на Token-2022

**Автор:** Медиханов Тимур

Токен **Timur Token (TMR)** в Devnet на программе Token-2022 (Token Extensions) и клиент
на `@solana/spl-token`: выпуск, переводы, метаданные, комиссия за перевод, сжигание.
Своей программы нет — токены выпускает и переводит системная программа Token-2022.

| | |
|---|---|
| Mint (адрес токена) | [`Cq9bsK12BiRunwpBNhGG9K8Di9QX4G2xfCcoG7TmmU31`](https://explorer.solana.com/address/Cq9bsK12BiRunwpBNhGG9K8Di9QX4G2xfCcoG7TmmU31?cluster=devnet) |
| Программа | Token-2022 `TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb` |
| Decimals | 6 |
| Supply | 990 000 TMR (выпущено 1 000 000, сожжено 10 000) |
| Расширения | TransferFeeConfig (1 %, не больше 1000 TMR), MetadataPointer, TokenMetadata |
| Метаданные | name «Timur Token», symbol «TMR», course «Solana, неделя 7», author «Медиханов Тимур» |
| Все права на токен | мой кошелёк `8sJ49ZVUw8QXfxogwHRkXC1nfm6ZT7vrTWwq6xztBkVV` |

| Действие | Транзакция |
|---|---|
| Создание токена с расширениями и метаданными | [2qMrfT2R…](https://explorer.solana.com/tx/2qMrfT2RrfiGKNJazgyM7xmd2LfXEuRr9Rx39oR1rzeZxhmqudVRUJ1T3rteqzwFDJh2tMX4BWRpkuyoj9vxCS23?cluster=devnet) |
| Выпуск 1 000 000 TMR | [3zSSJTji…](https://explorer.solana.com/tx/3zSSJTjiRSNUA4Y2zb8qZ9iogX4GhkCk3zmjbq3eTYKr9dZeRMenxA25AeidknESeAtvhc9kgGw4Nb7jE9rFk82N?cluster=devnet) |
| Перевод 10 000 TMR второму кошельку (комиссия 100) | [4R5xBBkD…](https://explorer.solana.com/tx/4R5xBBkDF8Tm1hFkvhPwKtQnLJp6v1SPsAuMsUYpokDvWDWvbKZQTDohPLLBXiYuvv8cX1eZAGTJP3qUd7XwsGit?cluster=devnet) |
| Перевод 5 000 TMR на основной кошелёк (комиссия 50) | [4xQKSGjJ…](https://explorer.solana.com/tx/4xQKSGjJkFCcn7dhsCTWPjssxmcgUjp8Cv3HgrCjKriWhuozfTMvKgqvtBsdAKbgkZTFmLTzxEbLcbUtWafwA1pZ?cluster=devnet) |
| Обратный перевод 1 000 TMR от второго (комиссия 10) | [33JBKkMD…](https://explorer.solana.com/tx/33JBKkMEDq1w7KLtdy7dBkZyjHA4VP3amzKXuDuzpeBAWDDv2oVZiYdyVsPERJNaHajf8eL5QdkmDu4BwJNJ5SVb?cluster=devnet) |
| Сбор удержанных комиссий (160 TMR) | [5Mto8PRE…](https://explorer.solana.com/tx/5Mto8PREGCRGSJiAsDBQ6ZSWFk68TEXFV47Cj3D1Xkjyu2NJQfyhx325NjPxVdL5B1AkDreBGZdqorbh84iEDn5u?cluster=devnet) |
| Сжигание 10 000 TMR | [3DeWQnTt…](https://explorer.solana.com/tx/3DeWQnTtvwXfaX3dXzXdtwdFiYRt2uNsf1H7Yu9tiviZDxvjict5hmfeamqJaJt6PqSi47sakqMvTjhJ88FB347?cluster=devnet) |
| Передача прав на основной кошелёк | [3Np77EKU…](https://explorer.solana.com/tx/3Np77EKUWXYPr3xBo7UP2yZB1J7bEnhHbXckpCnFe7Wfdn24Va6HU76eShSfMT4PiNA7x7MAoqMWewWVWAzCWoE4?cluster=devnet) |

## Как повторить

Начните в `token/`. Ключи исходной демонстрации остаются у автора;
для нового токена создайте собственные кошельки:

```bash
mkdir -p keys
solana-keygen new --outfile keys/wallet.json
solana-keygen new --outfile keys/friend.json
solana airdrop 2 --keypair keys/wallet.json --url devnet
```

В `client/token.mjs` замените адрес `MY_WALLET` на свой публичный адрес:
демонстрация отправляет часть токенов на этот кошелёк.
Файл `keys/mint.json` клиент создаёт сам при выпуске нового токена.

```bash
cd client
npm ci
node token.mjs                  # новый токен (если нет keys/mint.json), выпуск, переводы,
                                # сбор комиссий, сжигание, итог
node token.mjs info             # токен и балансы
node token.mjs handover <адрес> # передать права на токен другому кошельку
```

Права на этот токен уже у кошелька `8sJ49…`, поэтому `node token.mjs` с кошельком деплоя
создаст новый токен только если удалить `keys/mint.json`.

## Что где лежит

```text
client/token.mjs   клиент: создание токена, операции, чтение метаданных и балансов
keys/              wallet.json, friend.json, mint.json — в .gitignore
```

[Общий список работ](../README.md) · [Отчёт Word](../reports/week-07-token-2022-tmr.docx)
