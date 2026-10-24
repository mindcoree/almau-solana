# Официальный CPI-интерфейс Raydium

`raydium_clmm.json` получен из
[raydium-io/raydium-idl](https://github.com/raydium-io/raydium-idl/blob/e7e0c96fe77bcf6a020b84a44c47a722aac8e359/raydium_clmm/raydium_clmm.json),
коммит `e7e0c96fe77bcf6a020b84a44c47a722aac8e359`.
Единственное изменение данных JSON — поле `address`, заменённое на
официальный Devnet ID `DRayAUgENGQBKVaX8owNhgzkEDyoHTGVEGHVJT1E9pfH`.
Инструкции, дискриминаторы, аргументы и типы аккаунтов сохранены полностью.

`declare_program!(raydium_clmm)` генерирует типизированный CPI-клиент,
который программа vault использует для `swap_v2`. Это стандартный
[механизм совместимости Anchor](https://www.anchor-lang.com/docs/features/declare-program)
между программами на разных версиях Anchor. Готовый официальный Rust-адаптер
[raydium-cpi](https://github.com/raydium-io/raydium-cpi/blob/115df2779d53bacc7db9d0be2773a4b48a6d372b/programs/clmm-cpi/Cargo.toml)
закреплён на Anchor 1.0.2, тогда как существующий vault использует 1.2.0.

Порядок CPI-аккаунтов и обработка remaining accounts соответствуют
[официальному примеру proxy_swap](https://github.com/raydium-io/raydium-cpi-example/blob/336e096e92b547809f5d71c6d480c25ff9432be7/clmm-cpi/programs/clmm-cpi/src/instructions/proxy_swap.rs).
В vault аккаунт `payer` этого CPI — PDA vault с `invoke_signed`;
администратор только разрешает выполнение инструкции.
