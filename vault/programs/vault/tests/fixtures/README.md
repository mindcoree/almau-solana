# Данные mint для теста TMR

`tmr-mint.bin` — 523 байта публичных данных аккаунта mint
[`Cq9bsK12BiRunwpBNhGG9K8Di9QX4G2xfCcoG7TmmU31`](https://explorer.solana.com/address/Cq9bsK12BiRunwpBNhGG9K8Di9QX4G2xfCcoG7TmmU31?cluster=devnet),
полученные через `getAccountInfo` в Solana Devnet 12 октября 2026 года.
Это токен TMR из недели 7 с расширениями `TransferFeeConfig`,
`MetadataPointer` и `TokenMetadata`.

Тест помещает этот снимок в LiteSVM под тестовым адресом mint. Он проверяет,
что `create_vault` создаёт хранилище с требуемым расширением `TransferFeeAmount`
и нулевым балансом, а share-mint получает authority vault и нулевой supply.
Снимок позволяет запускать тест без RPC и изменений состояния Devnet.

## Raydium CLMM для недели 12

Перед тестами обмена из `vault/client` выполните `npm run swap:fixture`.
Скрипт [swap-fixture.mjs](../../../../client/swap-fixture.mjs) получает публичный
байткод Devnet CLMM и аккаунты пула недели 11, затем сохраняет их в
игнорируемой папке `vault/target/fixtures/` вместе с SHA-256, слотом и quote SDK.
Приватный ключ не требуется; никаких транзакций скрипт не отправляет.

Тесты загружают настоящий байткод Raydium в LiteSVM, создают тестовый vault
и выполняют CPI. Проверяются результат SDK, подпись PDA, права администратора,
связи аккаунтов, запрет однотокенового учёта shares в торговом vault и полный
откат данных при `TooLittleOutputReceived`. После получения снимка тесты
работают локально без RPC. После `cargo clean` снимок нужно получить снова.
