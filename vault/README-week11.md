# Неделя 11 — AMM и концентрированная ликвидность

**Автор:** Медиханов Тимур. **Сеть:** Solana Devnet.

Созданы два собственных тестовых токена, CLMM-пул Raydium и одна позиция
вокруг начальной цены. Позиция открыта кошельком, её NFT принадлежит этому
кошельку. Это отдельная позиция Raydium, не `UserPosition` программы vault.
Все адреса для недель 12–13 сохранены в [deployment-week11.json](deployment-week11.json).

Использованы тот же учебный репозиторий и кошелёк недели 8
`8ijYtQTzr7MSqvxxGZpGr9w3FKMkyY1W8pcUBYNymLSi`. Перед запуском
на нём было 8,837270695 Devnet SOL; дополнительное пополнение не потребовалось.
Новый скрипт: [client/clmm.mjs](client/clmm.mjs).

## Результат в Devnet

Проверено **24 октября 2026 года**. В таблице A и B — обозначения двух mint
в порядке, установленном Raydium; цена измеряется в **B за 1 A**.
Оба токена выпущены через обычную SPL Token Program, имеют `decimals = 6`,
mint authority кошелька и отсутствующую freeze authority. На кошелёк выпущено
по **100000 токенов каждого вида**. Это новые учебные токены, отдельные от TMR.
NFT позиции создан в Token-2022, его supply равен 1, decimals равен 0.

| Объект | Адрес в Solana Explorer, Devnet |
|---|---|
| Токен A | [99DCQk2bTNf51imVsZmekBvQe3WzMBzSnnbFheJ6XmhG](https://explorer.solana.com/address/99DCQk2bTNf51imVsZmekBvQe3WzMBzSnnbFheJ6XmhG?cluster=devnet) |
| Токен B | [EweCVMPvYeFojEmbphGnMchjAvLVMLaFbt3emjyNqrWB](https://explorer.solana.com/address/EweCVMPvYeFojEmbphGnMchjAvLVMLaFbt3emjyNqrWB?cluster=devnet) |
| Программа Raydium CLMM | [DRayAUgENGQBKVaX8owNhgzkEDyoHTGVEGHVJT1E9pfH](https://explorer.solana.com/address/DRayAUgENGQBKVaX8owNhgzkEDyoHTGVEGHVJT1E9pfH?cluster=devnet) |
| Конфигурация пула | [FZdkW5jiYsjTnCVqFqPrxrQisQkCYrohd7ArZhoKnM8q](https://explorer.solana.com/address/FZdkW5jiYsjTnCVqFqPrxrQisQkCYrohd7ArZhoKnM8q?cluster=devnet) |
| Пул | [6YkuE21oSujgDiG7G1JqwvfvJD3f2Ud5KkViDXysjNBY](https://explorer.solana.com/address/6YkuE21oSujgDiG7G1JqwvfvJD3f2Ud5KkViDXysjNBY?cluster=devnet) |
| Хранилище A | [8e19LHdxsqbNdWrtDWQF3hVE874AQznaBDq2dppgXzrR](https://explorer.solana.com/address/8e19LHdxsqbNdWrtDWQF3hVE874AQznaBDq2dppgXzrR?cluster=devnet) |
| Хранилище B | [ByaCprokWgVWQL8J2ucteQoGW4Su6m869wBG2UGHnAYP](https://explorer.solana.com/address/ByaCprokWgVWQL8J2ucteQoGW4Su6m869wBG2UGHnAYP?cluster=devnet) |
| Observation | [AA17hTGhZmVpZoWGEMgKVFbW62dniofMnM9inmtkUjpw](https://explorer.solana.com/address/AA17hTGhZmVpZoWGEMgKVFbW62dniofMnM9inmtkUjpw?cluster=devnet) |
| Расширение bitmap | [ALQxAjXyVArBRENY2gCwVHGswCJsq3nQu3ABWDSCPnhz](https://explorer.solana.com/address/ALQxAjXyVArBRENY2gCwVHGswCJsq3nQu3ABWDSCPnhz?cluster=devnet) |
| Позиция (`PersonalPositionState`) | [BqC3iXcjMLENa3kQGN4ZvUTPb3cGwHSzuKjXf7Cn4T5w](https://explorer.solana.com/address/BqC3iXcjMLENa3kQGN4ZvUTPb3cGwHSzuKjXf7Cn4T5w?cluster=devnet) |
| NFT позиции | [H7VCa2Sv77oVWAiNCHr7dVjh153Y6qVyEaWn3ibHmsC6](https://explorer.solana.com/address/H7VCa2Sv77oVWAiNCHr7dVjh153Y6qVyEaWn3ibHmsC6?cluster=devnet) |
| NFT-аккаунт кошелька | [G6rzBySgHw6zs3cCJY25rJ6C2979a6bJVuPof4h9XAHB](https://explorer.solana.com/address/G6rzBySgHw6zs3cCJY25rJ6C2979a6bJVuPof4h9XAHB?cluster=devnet) |
| Нижний tick array | [BCJhYme2FJsu2Jbtt5AJxkFvndwyphkM6Mc44BsKywTs](https://explorer.solana.com/address/BCJhYme2FJsu2Jbtt5AJxkFvndwyphkM6Mc44BsKywTs?cluster=devnet) |
| Верхний tick array | [7D4qXThEMB9kA6C8cS2XwcXkayTswVckAmjTH22mWh41](https://explorer.solana.com/address/7D4qXThEMB9kA6C8cS2XwcXkayTswVckAmjTH22mWh41?cluster=devnet) |

## Цена и выбранный диапазон

Начальная и текущая цена при проверке — **1 B за 1 A**, текущий tick — **0**.
`sqrtPriceX64 = 18446744073709551616`. В конфигурации №2 комиссия обмена
равна **0,05%** (`tradeFeeRate = 500` при знаменателе 1000000), шаг тиков — **10**.

Выбран диапазон **±10% от начальной цены**: от 0,9 до 1,1 B за A.
После округления наружу по шагу тиков фактические границы:

| Параметр | Значение |
|---|---:|
| Нижний tick | -1060 |
| Верхний tick | 960 |
| Нижняя цена | 0,89942941472144732833 |
| Верхняя цена | 1,1007537807153331814 |
| Внесено A | 1000 |
| Внесено B | 1101,433008 |
| Ликвидность позиции, L | 21338374629 |

У учебной пары нет рыночной истории, поэтому ±10% — выбранный для
демонстрации коридор, а не оценка волатильности. Он оставляет запас для
тестовых обменов и одновременно показывает концентрацию капитала около
цены 1:1. Более узкий коридор потребовал бы чаще переставлять позицию.
Текущая цена находится внутри диапазона: ликвидность позиции активна.
Количество A и B отличается, поскольку границы симметричны по цене,
а формулы CLMM используют квадратные корни цен.

Сначала из 1000 A вычисляется ликвидность, затем из неё — требуемая сумма B
с округлением вверх. Для B задаётся максимальный расход **1112,447339 B**,
то есть запас 1% относительно расчёта. Контракт потратил **1101,433008 B**.
Денежные суммы хранятся в `BN`, цены — в `Decimal`.

После открытия позиции в кошельке осталось 99000 A и 98898,566992 B.
В пуле находятся 1000 A и 1101,433008 B; вместе с остатками кошелька
это в точности первоначальный выпуск. NFT находится в ATA владельца.

## Подтверждённые транзакции

| Действие | Слот | Транзакция |
|---|---:|---|
| Создание A, ATA и выпуск 100000 A | 514481464 | [Explorer](https://explorer.solana.com/tx/4scssBqTktM31b7e2UfhWyg2SZUSHiNYe17wdYF7p718kXySDvRrHWCjzAeLWVwKuoPmHbPWa1nv3TbuMYgx87RS?cluster=devnet) |
| Создание B, ATA и выпуск 100000 B | 514481473 | [Explorer](https://explorer.solana.com/tx/45FBgmyEwxYL5yzXpraZSsCS1w5Rg8zEM97YMwuL4uXCqtm3UtyrgT5onPGTBSUkDPkiLquXM4PeU7x537JB5MSu?cluster=devnet) |
| Создание CLMM-пула | 514481482 | [Explorer](https://explorer.solana.com/tx/5Ro69Z674XN1xbaC7XJDtKE2ZnfGgMbt8Mrui34gZG9EQmUhs8spSSzVKSWvyqyTsaLq2WHqjGNUcci7oGsfqzAV?cluster=devnet) |
| Открытие позиции и внесение ликвидности | 514481874 | [Explorer](https://explorer.solana.com/tx/4PhedtKtyt76f3hz9xT9jNcbFZ1s4daBKCQjc8axrBv6nNVxu2JKGi8pYrmzgwT2gdAjuUz74QeRbq86a6YPykWg?cluster=devnet) |

Все четыре транзакции завершились без ошибок и имеют статус `finalized`.
Откройте пул по ссылке выше для просмотра его аккаунта, позицию — через
`PersonalPositionState`, а право владения — через NFT и его ATA. В транзакции
открытия видны вызовы Raydium, создание NFT и переводы обоих токенов в пул.

## Запуск

Просмотр существующего пула и проверка аккаунтов не требуют файла кошелька:

```bash
cd vault/client
npm ci
npm run clmm:show
npm run test:clmm
```

Создание пула и позиции с локальным кошельком:

```bash
cd vault/client
npm run clmm:demo
```

По умолчанию используются `vault/keys/wallet.json` и
`vault/deployment-week11.json`. Повторный запуск с тем же кошельком проверяет
существующие транзакции и аккаунты: дополнительные токены, пул и позиция
не создаются. Ключи новых mint хранятся в игнорируемой папке
`vault/keys/clmm/<адрес кошелька>/` с правами файла `0600`.

После клонирования для своего кошелька задайте отдельный файл результата:

```bash
WALLET=/полный/путь/кошелёк.json CLMM_STATE=/tmp/my-clmm.json npm run clmm:demo
CLMM_STATE=/tmp/my-clmm.json npm run clmm:show
```

Кошелёк должен иметь Devnet SOL; при необходимости пополните его через
[официальный faucet](https://faucet.solana.com/). `RPC_URL` позволяет выбрать
другой Devnet RPC. Скрипт проверяет genesis hash сети до чтения кошелька
и формирования транзакций.

## Проверки и адаптация официального примера

SDK закреплён на **`@raydium-io/raydium-sdk-v2@0.2.70-alpha`**, как в
официальном demo на коммите `003d9045830033e5ef4178d6db129d83f93e1c7f`.
Остальные версии закреплены в `client/package-lock.json`.
Примеры использованы для настройки вызовов SDK, с поправками для этой работы:

- Явно выбраны Devnet cluster и Devnet Program ID, mint и config читаются из RPC.
- Для первой позиции используется `getSimplePoolInfo`: у нового пула ещё нет
  tick arrays, которые ожидает общий `getPoolInfoFromRpc` этой версии SDK.
- Сумма B вычисляется через `getAmountsForLiquidity` из рассчитанной ликвидности.
- `protocol_position` в актуальной программе Raydium помечен устаревшим:
  адрес остаётся в ABI, но аккаунт не создаётся. В JSON для него `exists = false`;
  фактическая позиция — `personalPosition`.
- Каждая транзакция сначала симулируется; подпись сохраняется до отправки.
  Подтверждение проверяется через HTTP RPC, данные транзакций сопоставляются
  по подписи и слоту.

Пять тестов проверяют округление диапазона, разные decimals и цены, точные
сумму B и ликвидность, лимит 1%, большие целые числа и неверные параметры.
В Devnet дополнительно проверены владельцы и PDA пула, позиции и tick arrays,
выпуск токенов, балансы, активность ликвидности и владение NFT.
Повторный `demo` не изменил supply, балансы и список транзакций.
`clmm:show` и прежний `vault.mjs show` проверены без файла кошелька.
Rust-программа vault на этом этапе не менялась; её SBF-тесты повторно не запускались.

Официальные источники:
[создание пула](https://github.com/raydium-io/raydium-sdk-V2-demo/blob/003d9045830033e5ef4178d6db129d83f93e1c7f/src/clmm/createPool.ts),
[открытие позиции](https://github.com/raydium-io/raydium-sdk-V2-demo/blob/003d9045830033e5ef4178d6db129d83f93e1c7f/src/clmm/createPosition.ts),
[адреса программ](https://github.com/raydium-io/raydium-docs-v1/blob/main/reference/program-addresses.mdx),
[аккаунты инструкции NFT-позиции](https://github.com/raydium-io/raydium-clmm/blob/master/programs/amm/src/instructions/open_position_with_token22_nft.rs).

## Вопросы для защиты

**Чем CLMM отличается от простого пула?** В обычном constant-product пуле
ликвидность распределена по всем ценам. В CLMM каждый поставщик выбирает
свой диапазон и получает отдельную NFT-позицию. Для обмена используется
ликвидность тех позиций, в чей диапазон входит текущая цена.

**Что происходит вне диапазона?** Позиция остаётся существовать, но её
ликвидность становится неактивной и перестаёт получать новые комиссии
от обменов. Средства оказываются в одном из токенов: ниже нижней границы —
в A, выше верхней — в B. Начисленные ранее комиссии сохраняются. Для
возобновления участия нужна цена внутри диапазона или новая позиция с
другими границами.

**Почему узкий диапазон эффективнее, но требует внимания?** При той же сумме
капитала больше ликвидности сосредоточено около текущей цены. Это может
увеличить долю комиссий при прочих равных, но цена быстрее выйдет за
границы. Доход зависит от объёма обменов и конкурирующей ликвидности;
выбранный узкий диапазон сам по себе дохода не гарантирует.
