# Неделя 13 — позиция vault в Raydium CLMM

**Автор:** Медиханов Тимур. **Сеть:** Solana Devnet. **Проверено:** 24 октября 2026.

Vault открывает CLMM-позицию через CPI, хранит её NFT, собирает комиссии и
закрывает всю ликвидность. Все три действия доступны только администратору.
Выполнен полный цикл с обменами другого кошелька, затем выход цены за границу
и открытие новой позиции. Публичные адреса, лимиты, снимки и 13 финализированных
транзакций сохранены в [deployment-week13.json](deployment-week13.json).

## Продолжение существующего проекта

Использованы прежние программа `J6x4mnjWyfsAYVVd1zNSfJwf9FG1k1LNwzjkoodkKmnN`,
пул недели 11 `6YkuE21oSujgDiG7G1JqwvfvJD3f2Ud5KkViDXysjNBY` и торговый vault
недели 12 `DYc1DGr8GjgfT8syxUmT6MVrgYwwGuWbKTkqBi24noT9`.
Mint A: `99DCQk2bTNf51imVsZmekBvQe3WzMBzSnnbFheJ6XmhG`;
mint B: `EweCVMPvYeFojEmbphGnMchjAvLVMLaFbt3emjyNqrWB`.
Оба токена имеют 6 decimals; все суммы ниже приведены в целых токенах,
а аргументы инструкций и JSON — в минимальных единицах.

`VaultConfig` (145 байт), `UserPosition` (81 байт), `MarketConfig` (137 байт)
сохранены без миграции. Старые аккаунты TMR проверены по SHA-256: данные
совпадают с результатом недели 12. Исходная позиция кошелька недели 11
сохранена: именно она обеспечивает основную ликвидность для тестовых обменов.

Добавлены два PDA:

| Аккаунт | Seeds | Назначение |
|---|---|---|
| `VaultPosition`, 105 байт | `["vault_position", vault]` | Связывает vault, mint NFT и Raydium personal position; одна активная позиция |
| `liquidity_payer`, без данных | `["liquidity_payer", vault]` | System-owned PDA для rent при открытии; остаток SOL возвращается администратору |

Оба хранилища A/B и NFT находятся под управлением PDA vault.
`UserPosition` вкладчика и `VaultPosition` CLMM — разные аккаунты.

## Реальный результат в devnet

Цена обозначает **B за один A**. Для первого размещения выбран диапазон ±5%
от текущей цены 0,9990638479. После округления наружу по шагу 10 тиков:
**[0,9483825255; 1,0491681375]**, тики **−530 / 480**.
Это умеренный диапазон для демонстрации: три небольших обмена оставляют
позицию активной. Внесены **8 A + 8,496564 B**;
верхние лимиты расходов с запасом 1% записаны в `operations.openInitial.quote`.

Отдельный трейдер `CQ4zUKdUhjjm15yeXryMXU1A1T7w8dcRcNkThfqkqzXi`
получил тестовые A/B и devnet SOL. Он подписал обмены **25 A → B**, **20 A → B**
и **10 B → A**; администратор эти обмены не подписывал.
Цена после трёх обменов — **0,9958516373**.

| Шаг | Результат |
|---|---|
| Открытие | 8 A + 8,496564 B в позиции; NFT принадлежит vault |
| Сбор комиссий | **0,000288 A + 0,000064 B** в хранилища vault; L не изменилось |
| Закрытие | **8,533383 A + 7,964536 B** вернулись в vault |
| Минимумы закрытия | 8,448049 A и 7,884890 B, на 1% ниже расчёта SDK |
| Возврат к одному активу | Все оставшиеся **9,458356 B** обменены обратно в A; B = 0, A = **100,022464** |

Возвращённые при закрытии суммы относятся к ликвидности после движения цены;
комиссии из предыдущей строки уже собраны отдельно. Комиссии малы, поскольку
тестовая позиция vault значительно меньше исходной позиции кошелька.

Транзакции: [openInitial](https://explorer.solana.com/tx/3aCCNZfRdowJpFVWy3ZBi4L8AfKbZtqBbFizWCuFLHrm6ZfnGEVJUUHuXqbXyzmAnjDMC1i28zzyLiKU5WFoHS63?cluster=devnet), [traderSwapA1](https://explorer.solana.com/tx/2Zxxz1QpVRE2BKkW1huN2RBep4izCe4bxQyKDeBt2XTULAtNNX7TLv5bpD4z2Bz9hYeavu22PJ2eUTqUNXRSWVJk?cluster=devnet), [traderSwapA2](https://explorer.solana.com/tx/51Wyvv4vGJaMRJn8EDYgBBLXMtTdLFiEtYaepvS26NDXvHPTLGCNHFYMdPmdxPCDFkYpLmhz2XUpy5QKj2Tz4LX7?cluster=devnet), [traderSwapB](https://explorer.solana.com/tx/4b7vh1z96QmnEx6rePBSq9dxtKk1SDuQxb8YxbiypEsDUvEZuFy6yKxQYSs1cyXNXXR2jDJdRhHoAtXcqEgTvti8?cluster=devnet), [harvestInitial](https://explorer.solana.com/tx/5B3T2oPMYVv9xp4CmBege8LZxSvvds2Ub368hGyroYGkKMCCAhDAY3D6CzBkE8s7QNATDM2Gy5q9BYANMMjy89vL?cluster=devnet), [closeInitial](https://explorer.solana.com/tx/2Tez6ca2TaLSnMMj6LzCMvSd1a6ExXKo31HMTkLSta3weAnkTQNTjKNSphDqWid7uYxzAZv6YVCYcz7phPgFuTxC?cluster=devnet),
[convertAllB](https://explorer.solana.com/tx/32AZNjRrXCQRugeBMj6ASNceCV73c6o7hAB9Aq77cvbowYGjGjxMCZGkWi2JWieYrpv5snTZN7pTWJGPsmcT3xsi?cluster=devnet). Остальные ссылки и слоты находятся в JSON.

## Перебалансировка

После обратного обмена администратор снова обменял 10 A на B и открыл
узкий диапазон ±0,5%: **[0,9900503287; 1,0010004501]**, тики **−100 / 10**,
вложив **5 A + 5,543504 B**.
Трейдер обменял ещё **150 A → B**. Цена упала до **0,9824955838**,
тик **−177**, ниже нижней границы. Клиент проверил `inRange = false`.

При закрытии этой позиции вернулось **10,585358 A + 0 B**.
Минимум A — **10,477181**, минимум B — **0**: за нижней границей ликвидность
целиком представлена A. Требовать положительный минимум B здесь неверно.
После этого vault открыл диапазон ±5% вокруг новой цены:
**[0,9333298998; 1,0325158534]**, тики **−690 / 320**,
вложив **3 A + 3,045889 B**. Новая позиция находится внутри диапазона.

Транзакции: [openNarrow](https://explorer.solana.com/tx/4QneDmfedHx2JyKtbpD8UdgvnwjfjNejNsq6owMb8YaNVyWQfotw2z5yEvKMhD84dtgMiN8wyeg3Pzc5JTYwSHvc?cluster=devnet), [moveOutsideRange](https://explorer.solana.com/tx/2UZJkkAUHKxxoXH6dHxguiJGBX3e7v83sGn2hJ8AG42kHzjPTo16nr1RehoxTEGYTivtwxZS4ibjrzfQ6UZnqBYB?cluster=devnet), [closeOutsideRange](https://explorer.solana.com/tx/3qV2X8LCdM5HtPPY7vVbcU6YwVBsNMdXJ1mirsDoTruetPdVbnEVkBegvuRXgnSzQ17sYYWtAkhTuonZ9ruraBxj?cluster=devnet), [reopenNewRange](https://explorer.solana.com/tx/25tBXbapGXcSVBQMc6LVAQ8jWWE2Jwm2pyCfba4RUAgkHgZXqh3MqcHjnF5ZvKwvusCmXFN2oefwdigE7USudztx?cluster=devnet).

Итоговая NFT: `8NSgsA1HD4ii7wdYmbhfqtEYo4pHDdGhm8UyLKCZ7FAV`.
Её token account: `p2kCSLN8VxZa878CyHyCuMf3UmnVCeeBvf5wTLDzg3m`,
authority: `DYc1DGr8GjgfT8syxUmT6MVrgYwwGuWbKTkqBi24noT9`.
Raydium personal position: `3SJBzgD5PbJRkJDXb3APw8Ai3DEjpn6FRgUmWgewnKsZ`.
На момент проверки свободный остаток — **92,607822 A + 1,368327 B**;
ликвидность новой позиции **121257661** учитывается отдельно.
Эта позиция оставлена открытой для следующего этапа проекта.

## Инструкции и проверки

Код: [position.rs](programs/vault/src/position.rs),
[swap.rs](programs/vault/src/swap.rs), [клиент](client/position.mjs).
Используется официальный Raydium IDL и `declare_program!`, как в неделе 12;
источник и версия указаны в [idls/README.md](idls/README.md).
Схема вызовов соответствует [официальным CPI-примерам Raydium](https://github.com/raydium-io/raydium-cpi-example/tree/master/clmm-cpi).

| Инструкция vault | CPI Raydium | Ограничения |
|---|---|---|
| `open_clmm_position(lower_tick, upper_tick, L, maximum_a, maximum_b, rent_budget)` | `OpenPositionWithToken22Nft` | Положительное L, упорядоченные тики по tick spacing, максимум расходов A/B |
| `collect_fees()` | `DecreaseLiquidityV2(0, 0, 0)` | Возвращает комиссии, оставляет L |
| `close_clmm_position(minimum_a, minimum_b)` | `DecreaseLiquidityV2(all_L, min_a, min_b)` + `ClosePosition` | Вся ликвидность, минимум возврата; NFT сжигается, mint/ATA/personal position и маркер закрываются |
| `swap_back(amount_in, minimum_out)` | `SwapV2` в направлении B → A | Для возвращения к одному активу перед пользовательским выводом |

Везде проверяются подпись и `vault.admin`, канонические PDA, привязка рынка
к пулу и mint, адреса хранилищ Raydium и authority собственных токеновых
аккаунтов. Сбор и закрытие дополнительно связывают сохранённый mint NFT,
NFT account с количеством 1 и Raydium personal position с этим пулом.
Программа Raydium закреплена в типизированном `Program`, подменить её нельзя.

Особенность официального открытия Raydium: `payer` одновременно оплачивает
rent и разрешает расход A/B. PDA настроек vault содержит данные и не может
оплачивать System Program transfer. Поэтому внутри **одной атомарной инструкции**:
администратор переводит бюджет 0,2 SOL в System-owned `liquidity_payer`;
оба token account временно получают authority этого PDA; Raydium открывает
позицию с NFT owner = vault; authority обоих токенов возвращается vault,
неиспользованный rent возвращается администратору. Оба PDA подписывают CPI
через свои seeds; приватных ключей у них нет. При любой ошибке откатываются
также смена authority, rent, NFT, хранилища, tick arrays и счётчики.
Rent закрываемой NFT/позиции возвращается администратору; rent общих tick arrays
остаётся в пуле Raydium. Исходный rent vault не затрагивается.

## Пользовательский вывод

Учебное ограничение из методички реализовано явно: `deposit` и `withdraw`
отклоняются с `TradingVault`, если есть CLMM-позиция **или** ненулевой B.
Администратор сначала вызывает `position:close`, затем `position:back`.
После исчезновения маркера и B = 0 старые формулы shares снова применимы.
Проверка выполняется программой, поэтому её нельзя обойти через другой клиент.
`VaultUser` теперь требует три readonly PDA: `market`, `quote_vault`,
`position_state`, в том числе для TMR-vault, где эти аккаунты не созданы.
Клиент и опубликованный IDL обновлены.

Демонстрация продолжает запас из недели 12: 100 A внесены администратором
обычным SPL-переводом без shares. Supply shares этого торгового vault равен 0;
`total_deposited` показывает свободный A, а не стоимость всей позиции.
Автоматический вывод из LP и оценка NAV не реализованы. Реальные операции
вкладчиков, округление и возврат к shares после `close` + `back` проверяются
тестами LiteSVM. Для нового круга с вкладчиками сначала следует внести депозит
в пустой vault: при нулевом supply первый вкладчик получает право на весь
существующий запас согласно формуле недели 9.

## Запуск

```bash
cd vault/client
npm ci
npm run position:show                   # без кошелька, только чтение
npm run position:demo                   # сохранённый сценарий; повтор не двигает средства
npm run position:collect                # администратор собирает комиссии текущей позиции
npm run position:close                  # администратор закрывает всю текущую позицию
npm run position:back                   # обменивает весь оставшийся B обратно в A
npm run position:open -- 3000000 0.05    # новая позиция, 3 A, диапазон ±5%
# Или явно задать цены нижней и верхней границы:
npm run position:open -- 3000000 0.93:1.04
npm run position:verify                 # проверка сохранённых devnet-транзакций и bytecode
```

Команды `open/collect/close/back` изменяют devnet; они продолжают текущее
состояние, поэтому после закрытия для нового размещения могут понадобиться
B через `swap.mjs swap [raw A]`. Максимумы расходов рассчитаны с запасом 1%,
минимумы возврата — на 1% ниже котировки SDK. Инструкция позволяет задать
свои `maximum_a/b` и `minimum_a/b` через IDL. Нулевой минимум допустим для
отсутствующей стороны при выходе за диапазон.

По умолчанию используется игнорируемый `vault/keys/wallet.json`;
`WALLET` позволяет указать другой файл, но подписант должен совпадать
с администратором этого vault. Новые NFT и трейдерские ключи сохраняются
только в игнорируемом `vault/keys/week13/` с mode 0600.
`RPC_URL` задаёт devnet RPC. Для localhost обязателен отдельный
`POSITION_STATE=/tmp/week13-local.json`, чтобы не затереть devnet-результаты.
Подписи сохраняются перед отправкой, проверяются HTTP polling; повтор `demo`
использует уже выполненные шаги. При истечении неподтверждённой транзакции
клиент останавливается и сообщает подпись для проверки.

## Проверка

```bash
cd vault
cargo build-sbf --manifest-path programs/vault/Cargo.toml --arch v2 -- --locked
cd client
npm run swap:fixture    # публичные аккаунты и настоящий bytecode Raydium в target/fixtures
npm run test:clmm
npm run test:swap
cd ..
cargo test --locked
```

Пройдены **33 Rust-теста vault + 10 JavaScript-тестов**; прежний корневой
проект также проходит свои 34 теста. LiteSVM выполняет CPI в настоящий
Raydium bytecode: открытие и NFT owner, восстановление authority и возврат
rent, начисление комиссий без изменения L, полный close/burn/reopen,
откат лимитов, запрет чужого администратора и подмены аккаунтов,
некорректные тики/L, единственная активная позиция, блокировка shares даже
при B = 0 в активной позиции, обратный swap и возобновление shares.

Полный сценарий также выполнен в локальном валидаторе с клоном devnet-пула
и затем в самом devnet. Все 13 транзакций финализированы; проверены подписанты,
изменения token balances, совпадение опубликованного bytecode и IDL, сохранность
старых аккаунтов TMR. Исторические снимки недель 11–12 сохранены.

## Ответы для защиты

**Почему доли A/B зависят от текущей цены?** При P = B/A и границах Pa/Pb
внутри диапазона x = L × (1/√P − 1/√Pb), y = L × (√P − √Pa).
По мере роста цены трейдеры покупают A у позиции, поэтому A становится меньше,
B — больше. При падении происходит обратное. Вне диапазона остаётся один токен.

**Что будет выше верхней границы?** Вся ликвидность станет B; позиция перестанет
участвовать в обменах и получать новые комиссии, пока цена не вернётся.
Уже заработанные комиссии сохраняются. В демонстрации проверен симметричный
случай ниже нижней границы: весь возврат — в A.

**Зачем и кто делает ребалансировку?** Чтобы снова разместить ликвидность
вокруг актуальной цены и зарабатывать комиссии. В этой версии администратор
закрывает прежнюю позицию, при необходимости меняет соотношение A/B и открывает
новую. Узкий диапазон даёт большую концентрацию L на тот же капитал, но чаще
требует такого вмешательства и расходов на транзакции/rent.
