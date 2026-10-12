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
