# Отчёты по практическим работам Solana

**Автор:** Медиханов Тимур · **Специальность:** Data Science · **Год:** 2026

В этой папке собраны шесть исходных отчётов в формате Word. Имена файлов приведены
к единому формату; содержание, таблицы, скриншоты и оформление сохранены.
Чтобы прочитать отчёт, откройте ссылку и скачайте файл через **Download raw file** на GitHub.

| Работа | Файл | Что описано | Код и инструкция |
|---|---|---|---|
| Анализ транзакции Solana Devnet | [solana-devnet-transaction-analysis.docx](solana-devnet-transaction-analysis.docx) | Перевод 1 SOL, аккаунты, инструкции System Program и Compute Budget, изменения балансов, базовая и приоритетная комиссии | [Транзакция в Explorer](https://explorer.solana.com/tx/59eALrjdiNyPxoms24wnER4TBeDUd5rZJ1D5iGnz3JLWnbFMqZqGhY1gDTTMUrFYAYxuRGgbrSkanjfxzSAHJPT4?cluster=devnet) |
| Неделя 3 — Rust для Solana | [week-03-rust-piggy-vault.docx](week-03-rust-piggy-vault.docx) | CLI-копилка, учебный SDK и рантайм, PDA, Borsh, CPI, подписи, rent, тесты и демонстрация работы | [Исходники](../src/) · [Руководство Piggy](../docs/piggy-vault.md) |
| Неделя 4 — первая Solana Program | [week-04-first-solana-program.docx](week-04-first-solana-program.docx) | Перенос копилки на официальный SDK, сборка SBF, деплой в Devnet, вызовы Initialize, Deposit и Withdraw | [program/](../program/README.md) |
| Неделя 5 — Anchor Counter | [week-05-anchor-counter.docx](week-05-anchor-counter.docx) | Счётчик на Anchor, ограничения аккаунтов, IDL, клиент, тесты LiteSVM и транзакции в Devnet | [counter/](../counter/README.md) |
| Неделя 6 — профиль на PDA | [week-06-pda-profile.docx](week-06-pda-profile.docx) | Profile и UsernameRecord, уникальность имени, поиск профиля, обновление и закрытие аккаунтов, тесты | [profile/](../profile/README.md) |
| Неделя 7 — TMR на Token-2022 | [week-07-token-2022-tmr.docx](week-07-token-2022-tmr.docx) | Mint и ATA, метаданные, комиссия за перевод, выпуск, переводы, сбор комиссий, сжигание и передача прав | [token/](../token/README.md) |

Неделя 8 оформлена в [README каркаса vault](../vault/README.md):
схема аккаунтов, реализованные инструкции, проверки и ответы для защиты.
Исходные Word-отчёты по предыдущим работам сохранены без изменения содержания.

## Соответствие исходным документам

| Имя в репозитории | Исходное имя |
|---|---|
| `solana-devnet-transaction-analysis.docx` | `Анализ транзакции Solana Devnet перевод 1 SOL.docx` |
| `week-03-rust-piggy-vault.docx` | `Отчёт_3_задачи_Solana_Медиханов.docx` |
| `week-04-first-solana-program.docx` | `Отчёт_неделя_4_Первая_Solana_Program.docx` |
| `week-05-anchor-counter.docx` | `Отчёт_неделя_5_Anchor_Counter_Program.docx` |
| `week-06-pda-profile.docx` | `Отчёт_неделя_6_On-chain_профиль_PDA 1.docx` |
| `week-07-token-2022-tmr.docx` | `Отчёт_неделя_7_Токен_TMR_Token-2022.docx` |

Ссылки на архивы и локальные пути внутри документов относятся к первоначальной
сдаче работ. Соответствующий код теперь доступен в каталогах этого репозитория.

[Вернуться к общему описанию проекта](../README.md)
