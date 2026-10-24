# Неделя 12 — Swap через Raydium

**Автор:** Медиханов Тимур. **Сеть:** Solana Devnet.

В существующую Anchor-программу vault добавлены `initialize_market` и
`swap(amount_in, minimum_out)`. Администратор разрешает обмен, а программа
вызывает `SwapV2` Raydium и подписывает расход токенов от имени PDA vault.
Выполнены успешный обмен и настоящая отклонённая транзакция с завышенным
минимумом. Результаты RPC — в [deployment-week12.json](deployment-week12.json).

## Связь с предыдущими неделями

Использованы прежние Program ID
`J6x4mnjWyfsAYVVd1zNSfJwf9FG1k1LNwzjkoodkKmnN`, администратор и CLMM-пул
недели 11. Этот пул содержит A/B, поэтому через уже существующий `create_vault`
создан дополнительный vault для A. Адрес vault по схеме недели 8 зависит
от принимаемого mint; старый vault для TMR остаётся отдельным аккаунтом.
Форматы `VaultConfig` (145 байт) и `UserPosition` (81 байт) сохранены.
Данные старых настроек TMR, позиции, share-mint и хранилища после обновления
побайтно совпадают со снимком до обновления.

Добавлены два PDA:

| Аккаунт | Seeds | Содержимое / размер |
|---|---|---|
| `MarketConfig` | `["market", vault]` | `vault`, `quote_mint`, `quote_vault`, `pool_state`, `bump`; 137 байт |
| Хранилище B | `["quote_vault", vault]` | SPL token account, mint B, authority vault; 165 байт |

Хранилище A использует прежние seeds `["token_vault", vault]`.
Оба хранилища принадлежат Token Program, а распоряжается ими PDA vault.
В настройках рынка закреплены пул и второй mint. Они проверяются вместе
с данными реального пула, конфигурацией, oracle и адресами хранилищ Raydium.

Снимок ниже отражает неделю 12. В [неделе 13](README-week13.md) ограничение
shares уточнено: операции разрешаются после закрытия LP и возврата всего B
в A; клиент также передаёт readonly `quote_vault` и `position_state`.

## Защита учёта shares

Формула недель 9–10 учитывает один актив. После обмена часть стоимости
находится в B, поэтому использовать ту же формулу для новых shares нельзя.
`initialize_market` разрешён только при нулевом supply shares; после его
выполнения `deposit` и `withdraw` для этого торгового vault отклоняются
с `TradingVault`. Переход с существующими вкладчиками и обход проверки
через подмену PDA запрещены.

Для демонстрации администратор перевёл в торговый vault **100 A** обычным
SPL-переводом без выпуска shares. Supply равен **0**. Оценка общего портфеля
в единицах A и операции пользователей с двумя активами относятся к
следующим этапам работы. `total_deposited` после swap отражает оставшийся
баланс A, а B читается отдельно; это не NAV всего портфеля.

Для однотокеновых vault прежние операции сохраняются. В их списке аккаунтов
теперь обязателен readonly PDA `market`, даже если аккаунт ещё не создан.
Клиент и опубликованный IDL обновлены; старым клиентам нужен новый IDL
и этот дополнительный аккаунт.

## CPI и ограничения доступа

Официальный CPI-интерфейс генерируется Anchor из полного
[официального IDL Raydium](idls/README.md) через `declare_program!`.
Это сохраняет Anchor 1.2.0 существующего проекта при несовпадении версии
готового Rust-адаптера Raydium. Wire-формат `SwapV2` взят из официального IDL;
схема вызова соответствует примеру `raydium-cpi-example/clmm-cpi`.

Реализация: [src/swap.rs](programs/vault/src/swap.rs).

1. Проверяются подпись администратора и `vault.admin`, PDA vault и рынка,
   основной и второй mint, выбранный пул и оба его хранилища.
2. `amount_in` и `minimum_out` должны быть положительными; в A должно
   хватать средств. Вариант обмена — exact input.
3. Выполняется `raydium_clmm::cpi::swap_v2` с `is_base_input = true`.
   `payer` внутри CPI — PDA vault. Подпись задаётся через
   `["vault", admin, asset_mint, bump]` и `invoke_signed`.
4. Bitmap и tick arrays передаются как remaining accounts согласно
   расчёту SDK. Raydium вызывает Token Program для переводов A и B.
5. После CPI хранилища перечитываются: проверяются полный расход входа
   и фактический выход не ниже минимума. Обновляется остаток A в настройках.

Администратор — единственный внешний подписант swap-транзакции. Он не
становится authority хранилищ: Token Program разрешает расход благодаря
подписи PDA внутри CPI. Произвольную стороннюю программу вместо Raydium
передать нельзя: Program ID закреплён в сгенерированном типе программы.

## Результат в Devnet

Проверено **24 октября 2026 года**. Обновление программы выполнено в слоте
**514490132**, байткод — **372664 байта**. Он побайтно совпадает со сборкой,
проверенной в LiteSVM и на локальном валидаторе.

| Объект | Адрес в Explorer, Devnet |
|---|---|
| Токен A | [99DCQk2bTNf51imVsZmekBvQe3WzMBzSnnbFheJ6XmhG](https://explorer.solana.com/address/99DCQk2bTNf51imVsZmekBvQe3WzMBzSnnbFheJ6XmhG?cluster=devnet) |
| Токен B | [EweCVMPvYeFojEmbphGnMchjAvLVMLaFbt3emjyNqrWB](https://explorer.solana.com/address/EweCVMPvYeFojEmbphGnMchjAvLVMLaFbt3emjyNqrWB?cluster=devnet) |
| Прежний CLMM-пул | [6YkuE21oSujgDiG7G1JqwvfvJD3f2Ud5KkViDXysjNBY](https://explorer.solana.com/address/6YkuE21oSujgDiG7G1JqwvfvJD3f2Ud5KkViDXysjNBY?cluster=devnet) |
| Vault для A | [DYc1DGr8GjgfT8syxUmT6MVrgYwwGuWbKTkqBi24noT9](https://explorer.solana.com/address/DYc1DGr8GjgfT8syxUmT6MVrgYwwGuWbKTkqBi24noT9?cluster=devnet) |
| MarketConfig | [CxGk323DcH8ME6DJYvqrW5iSdVfWb3XhENfuq39xnzQi](https://explorer.solana.com/address/CxGk323DcH8ME6DJYvqrW5iSdVfWb3XhENfuq39xnzQi?cluster=devnet) |
| Хранилище A | [G7gy5do6ZKzo3CwFmRQY77ZmTJAUZWbukuPXjDwoc22U](https://explorer.solana.com/address/G7gy5do6ZKzo3CwFmRQY77ZmTJAUZWbukuPXjDwoc22U?cluster=devnet) |
| Хранилище B | [8eEHPwsPkXLZApCKNK1ZGVkgTrpuQVmk7dfNfRJk15wv](https://explorer.solana.com/address/8eEHPwsPkXLZApCKNK1ZGVkgTrpuQVmk7dfNfRJk15wv?cluster=devnet) |
| Share-mint, supply 0 | [2Pxx8Pjcc1KsFgPPYAiP5ZPVtubej6ZphiMXgDA1wTNP](https://explorer.solana.com/address/2Pxx8Pjcc1KsFgPPYAiP5ZPVtubej6ZphiMXgDA1wTNP?cluster=devnet) |

SDK рассчитал выход для **10 A** с учётом комиссии пула 0,05% и price impact:

```text
amount_in   = 10000000  = 10 A
expected_out = 9990320  = 9,990320 B
minimum_out  = floor(expected_out × 99 / 100)
             = 9890416 = 9,890416 B
```

| Параметр | До swap | После успешного swap |
|---|---:|---:|
| A в vault | 100 | 90 |
| B в vault | 0 | 9,990320 |
| Цена B за A | 1 | 0,99906384792694307035 |
| Текущий tick | 0 | -10 |
| A в пуле | 1000 | 1010 |
| B в пуле | 1101,433008 | 1091,442688 |

Цена снизилась примерно на **0,0936152%**: добавление A в пул снижает его
цену в B. Расчёт SDK уже учитывал это изменение; 1% slippage tolerance
применяется к ожидаемому выходу, а не к исходной цене 1:1.

### Транзакции

| Действие | Слот | Результат и ссылка |
|---|---:|---|
| Обновление программы | 514490132 | [Успешно](https://explorer.solana.com/tx/24PyWXDBxCwjqD3CWrDavkUqbqMFNJE4S3X6rMMSN69swZnh3aps4oZBEsqHwyrY1Ti8LhtUdiinvwe2KwzGutHB?cluster=devnet) |
| Создание vault для A | 514490448 | [Успешно](https://explorer.solana.com/tx/4xZt2T4aWw443BeRgPMuw5oZvH3c82AyUmHVZjMVnir32veRouhBPkXiVCP2ZyxNXxNevocNB24ArCNjCTrvnBjj?cluster=devnet) |
| Создание рынка и хранилища B | 514490495 | [Успешно](https://explorer.solana.com/tx/31XVjtJdYHCKubna8tNPbDhKU677eEMTiyMiFyCzeMCCcrtm2t8WiEPHMLXTTj9kV4tFuuyDrXhFkrf7FPdHD631?cluster=devnet) |
| Перевод 100 A в vault | 514490502 | [Успешно](https://explorer.solana.com/tx/ASiadPzk9ednHs9gqDSWb8MBXXx5QYKfrT1eroNQiTpoMZpPzv5FRurjPFvzM6wppQwinboQ29LRBqU8xobW4TM?cluster=devnet) |
| Обмен 10 A → B | 514490513 | [Успешно](https://explorer.solana.com/tx/31yJhfKiqAaAXWs2C6nappQjqwBNuKDBC65ragSLnxSGb71qjTJ86ucSS9MJYDEapsnDJkmSYxEgJiz7GUwHgM66?cluster=devnet) |
| Обмен с завышенным минимумом | 514490524 | [Отклонён по slippage](https://explorer.solana.com/tx/aYzsM4noJeMaN3uQsWH3K93fy5zcMAgW2uLSDNuQ6ft9hqFUfVfpdfroDCFtyMNMFBWufcK3UhDpvggJ4Y6YmTn?cluster=devnet) |

Все транзакции имеют статус `finalized`; у последней `err` содержит
**6018 / `TooLittleOutputReceived`** от Raydium. Для неё намеренно задан
`minimum_out = u64::MAX`. Она действительно отправлена в devnet с пропуском
preflight после проверки ожидаемой ошибки в симуляции.

Raydium начал расчёт и переводы, затем отклонил результат по минимуму.
Solana откатила всю транзакцию: A/B, счётчик vault, pool state, observation,
bitmap и tick arrays сохранили прежние данные. Их общий SHA-256 до и после
ошибки совпал; цена осталась `0,99906384792694307035`. Оплачена только
комиссия неуспешной транзакции — **5600 lamports**.

В Inner Instructions успешного swap видны Raydium `SwapV2` и два
`TransferChecked` Token Program: A из PDA vault в пул и B из пула обратно
в другое хранилище vault. Внешний подписант — администратор; authority
первого перевода — PDA `DYc1…noT9`, второго — PDA пула `6Yku…sjNBY`.

## Команды

```bash
cd vault/client
npm ci
npm run swap:show           # читать состояния без файла кошелька
npm run swap:quote -- 10000000
npm run test:swap
```

С локальным кошельком администратора:

```bash
npm run swap:demo           # создать/проверить аккаунты, 10 A swap, отказ по slippage
node swap.mjs swap 1000000  # отдельный обмен 1 A с минимумом на 1% ниже свежего quote
```

Повторный `demo` использует сохранённые подписи и не повторяет пополнение
и обмен. Приватный ключ хранится в игнорируемом `vault/keys/wallet.json`.
Настройки: `WALLET` — подписант, `CLMM_STATE` — файл пула недели 11,
`SWAP_STATE` — отдельный файл результата, `RPC_URL` — Devnet RPC.
Поддерживается локальный валидатор на loopback-адресе.
Для своего кошелька и своих тестовых токенов сначала выполните CLMM-сценарий
недели 11, затем укажите соответствующие `CLMM_STATE` и `SWAP_STATE`.
Старый TMR-vault просматривается прежней командой `node vault.mjs show`.

### Сборка и тесты

```bash
cd vault/client
npm ci
npm run swap:fixture  # только чтение Devnet; публичный байткод и данные в target/fixtures/
cd ..
cargo build-sbf --manifest-path programs/vault/Cargo.toml --arch v2 -- --locked
cargo test --locked
cd client
npm run test:clmm
npm run test:swap
```

Прошли **29 тестов LiteSVM**: 21 прежний и 8 новых. Новые тесты используют
настоящий байткод Raydium, проверяют CPI и точный результат SDK, откат
по slippage, права администратора, связи mint/хранилищ, ограничения сумм,
невозможность включить рынок с непогашенными shares и защиту учёта shares.
После скачивания публичного снимка эти тесты выполняются локально без RPC.

Прошли **8 JavaScript-тестов**: 5 для CLMM и 3 для slippage/точности `u64`.
Восемь неверных аргументов CLI отклонены до чтения кошелька и сети.
Полный клиентский сценарий проверен на локальном валидаторе с копией пула
и в Devnet, включая настоящие неуспешные транзакции. Просмотр swap, CLMM
и старого TMR-vault проверен без файла кошелька.

## Вопросы для защиты

**Что такое slippage и как защититься?** Это изменение условий исполнения
между расчётом quote и фактическим обменом. Exact-input swap задаёт
`minimum_out`: при меньшем выходе программа откатывает операцию. Здесь
минимум составляет 99% рассчитанного SDK выхода, с округлением вниз.

**Почему при малой ликвидности цена движется сильнее?** Один и тот же вход
поглощает большую долю доступных активов и быстрее проходит диапазоны тиков.
При большей активной ликвидности для такого же движения цены нужно больше
токенов. Поэтому размер сделки сравнивают с ликвидностью около текущей цены.

**Почему подписывает программа, а не администратор?** Authority хранилищ —
PDA vault. Подпись администратора разрешает вызов `swap`, но не даёт ему
прав на токены напрямую. Программа передаёт seeds в `invoke_signed`,
рантайм подтверждает PDA, а Raydium использует эту подпись в Token Program.
