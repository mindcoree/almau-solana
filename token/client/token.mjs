// Токен TMR на Token-2022 (Token Extensions) в Devnet.
//
//   cd client && npm install
//   node token.mjs                 # создать токен (если его нет), выпустить, перевести,
//                                  # собрать комиссии, сжечь часть — и показать итог
//   node token.mjs info            # только показать токен и балансы
//   node token.mjs handover <кошелёк>   # передать все права на токен другому кошельку
//
// Своей программы здесь нет: токены выпускает и переводит системная программа Token-2022,
// а клиент только собирает её инструкции. Кошельки: ../keys/wallet.json — владелец и
// все права на токен, ../keys/friend.json — второй кошелёк. Адрес токена — ../keys/mint.json.
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import {
  Connection,
  Keypair,
  PublicKey,
  SystemProgram,
  Transaction,
  clusterApiUrl,
  sendAndConfirmTransaction,
} from '@solana/web3.js';
import {
  AuthorityType,
  ExtensionType,
  LENGTH_SIZE,
  TOKEN_2022_PROGRAM_ID as TOKEN_2022,
  TYPE_SIZE,
  burnChecked,
  createInitializeMetadataPointerInstruction,
  createInitializeMintInstruction,
  createInitializeTransferFeeConfigInstruction,
  createSetAuthorityInstruction,
  getAccount,
  getExtensionTypes,
  getMint,
  getMintLen,
  getOrCreateAssociatedTokenAccount,
  getTokenMetadata,
  getTransferFeeAmount,
  getTransferFeeConfig,
  mintTo,
  transferCheckedWithFee,
  withdrawWithheldTokensFromAccounts,
} from '@solana/spl-token';
import {
  createInitializeInstruction,
  createUpdateAuthorityInstruction,
  createUpdateFieldInstruction,
  pack,
} from '@solana/spl-token-metadata';

// --- параметры токена -------------------------------------------------------------------
const NAME = 'Timur Token';
const SYMBOL = 'TMR';
const URI = '';
const EXTRA_FIELDS = [
  ['course', 'Solana, неделя 7'],
  ['author', 'Медиханов Тимур'],
];
const DECIMALS = 6;
const FEE_BPS = 100; // комиссия за перевод 1% (в базисных пунктах: 100 = 1%)
const UNIT = 10n ** BigInt(DECIMALS); // 1 TMR = 1 000 000 минимальных единиц
const MAX_FEE = 1_000n * UNIT; // но не больше 1000 TMR за перевод

// --- подключение и кошельки -------------------------------------------------------------
const here = new URL('.', import.meta.url);
const connection = new Connection(process.env.RPC_URL ?? clusterApiUrl('devnet'), 'confirmed');
const opts = { commitment: 'confirmed' };
const keypair = (file) => Keypair.fromSecretKey(Uint8Array.from(JSON.parse(readFileSync(new URL(file, here)))));
const owner = keypair(process.env.WALLET ?? '../keys/wallet.json');
const friend = keypair('../keys/friend.json');
const MY_WALLET = new PublicKey('8sJ49ZVUw8QXfxogwHRkXC1nfm6ZT7vrTWwq6xztBkVV'); // основной кошелёк

const explorerTx = (sig) => `https://explorer.solana.com/tx/${sig}?cluster=devnet`;
const tmr = (amount) => (Number(amount) / Number(UNIT)).toLocaleString('ru-RU', { maximumFractionDigits: DECIMALS });
const log = (label, sig) => console.log(`${label.padEnd(34)} ${explorerTx(sig)}`);

// Комиссия Token-2022: amount × bps / 10000, округление вверх, но не больше MAX_FEE.
const feeFor = (amount) => {
  const fee = (amount * BigInt(FEE_BPS) + 9_999n) / 10_000n;
  return fee > MAX_FEE ? MAX_FEE : fee;
};

// --- 1. создание токена ------------------------------------------------------------------
async function createToken() {
  const mint = Keypair.generate();
  const metadata = {
    mint: mint.publicKey,
    name: NAME,
    symbol: SYMBOL,
    uri: URI,
    additionalMetadata: EXTRA_FIELDS,
    updateAuthority: owner.publicKey,
  };
  // Размер mint-аккаунта: базовые 82 байта + расширения. Метаданные Token-2022 допишет сам
  // (увеличит аккаунт), но rent за них платим сразу.
  const mintLen = getMintLen([ExtensionType.TransferFeeConfig, ExtensionType.MetadataPointer]);
  const metadataLen = TYPE_SIZE + LENGTH_SIZE + pack(metadata).length;
  const lamports = await connection.getMinimumBalanceForRentExemption(mintLen + metadataLen);

  const tx = new Transaction().add(
    SystemProgram.createAccount({
      fromPubkey: owner.publicKey,
      newAccountPubkey: mint.publicKey,
      space: mintLen,
      lamports,
      programId: TOKEN_2022,
    }),
    // Расширения инициализируются ДО самого mint.
    createInitializeTransferFeeConfigInstruction(
      mint.publicKey, owner.publicKey, owner.publicKey, FEE_BPS, MAX_FEE, TOKEN_2022,
    ),
    // Указатель на метаданные — на сам mint: метаданные хранятся в том же аккаунте.
    createInitializeMetadataPointerInstruction(mint.publicKey, owner.publicKey, mint.publicKey, TOKEN_2022),
    createInitializeMintInstruction(mint.publicKey, DECIMALS, owner.publicKey, null, TOKEN_2022),
    createInitializeInstruction({
      programId: TOKEN_2022,
      metadata: mint.publicKey,
      updateAuthority: owner.publicKey,
      mint: mint.publicKey,
      mintAuthority: owner.publicKey,
      name: NAME,
      symbol: SYMBOL,
      uri: URI,
    }),
    ...EXTRA_FIELDS.map(([field, value]) =>
      createUpdateFieldInstruction({
        programId: TOKEN_2022,
        metadata: mint.publicKey,
        updateAuthority: owner.publicKey,
        field,
        value,
      }),
    ),
  );
  const sig = await sendAndConfirmTransaction(connection, tx, [owner, mint], opts);
  writeFileSync(new URL('../keys/mint.json', here), JSON.stringify(Array.from(mint.secretKey)));
  log('Создание токена (mint + метаданные)', sig);
  return mint.publicKey;
}

// Токен-аккаунт (ATA) — PDA от кошелька и mint в программе Associated Token Account.
const ata = (mint, wallet) =>
  getOrCreateAssociatedTokenAccount(connection, owner, mint, wallet, false, 'confirmed', opts, TOKEN_2022);

async function transfer(label, mint, from, fromWallet, to, amount) {
  const sig = await transferCheckedWithFee(
    connection, fromWallet, from, mint, to, fromWallet, amount, DECIMALS, feeFor(amount), [], opts, TOKEN_2022,
  );
  log(`${label}: ${tmr(amount)} TMR (комиссия ${tmr(feeFor(amount))})`, sig);
}

// --- 2. операции -------------------------------------------------------------------------
async function demo(mint) {
  const ownerAta = await ata(mint, owner.publicKey);
  const friendAta = await ata(mint, friend.publicKey);
  const myAta = await ata(mint, MY_WALLET);

  log('Выпуск 1 000 000 TMR', await mintTo(connection, owner, mint, ownerAta.address, owner, 1_000_000n * UNIT, [], opts, TOKEN_2022));
  await transfer('Перевод второму кошельку', mint, ownerAta.address, owner, friendAta.address, 10_000n * UNIT);
  await transfer('Перевод на основной кошелёк', mint, ownerAta.address, owner, myAta.address, 5_000n * UNIT);
  await transfer('Обратный перевод от второго', mint, friendAta.address, friend, ownerAta.address, 1_000n * UNIT);

  // Комиссии лежат «замороженными» в токен-аккаунтах получателей — забираем их себе.
  const withheld = [ownerAta, friendAta, myAta].map((a) => a.address);
  log('Сбор комиссий', await withdrawWithheldTokensFromAccounts(
    connection, owner, mint, ownerAta.address, owner, [], withheld, opts, TOKEN_2022,
  ));
  log('Сжигание 10 000 TMR', await burnChecked(
    connection, owner, ownerAta.address, mint, owner, 10_000n * UNIT, DECIMALS, [], opts, TOKEN_2022,
  ));
}

// --- 3. что получилось -------------------------------------------------------------------
async function info(mint) {
  const m = await getMint(connection, mint, 'confirmed', TOKEN_2022);
  const fee = getTransferFeeConfig(m);
  const meta = await getTokenMetadata(connection, mint, 'confirmed', TOKEN_2022);
  const extensions = getExtensionTypes(m.tlvData).map((e) => ExtensionType[e]);

  console.log(`\nТокен ${meta.name} (${meta.symbol})  ${mint.toBase58()}`);
  console.log(`  программа        Token-2022 ${TOKEN_2022.toBase58()}`);
  console.log(`  расширения       ${extensions.join(', ')}`);
  console.log(`  decimals         ${m.decimals}`);
  console.log(`  supply           ${tmr(m.supply)} TMR`);
  console.log(`  mint authority   ${m.mintAuthority?.toBase58() ?? '—'}`);
  console.log(`  комиссия         ${fee.newerTransferFee.transferFeeBasisPoints / 100}%, не больше ${tmr(fee.newerTransferFee.maximumFee)} TMR`);
  console.log(`  метаданные       ${meta.additionalMetadata.map(([k, v]) => `${k} = ${v}`).join('; ')}`);

  console.log('\nБалансы:');
  for (const [name, wallet] of [['владелец', owner.publicKey], ['второй', friend.publicKey], ['основной', MY_WALLET]]) {
    const address = (await ata(mint, wallet)).address;
    const account = await getAccount(connection, address, 'confirmed', TOKEN_2022);
    const withheld = getTransferFeeAmount(account)?.withheldAmount ?? 0n;
    console.log(`  ${name.padEnd(9)} ${tmr(account.amount).padStart(9)} TMR  удержано ${tmr(withheld)}  ${address.toBase58()}`);
  }
}

// --- 4. передать права -------------------------------------------------------------------
async function handover(mint, to) {
  const set = (type) => createSetAuthorityInstruction(mint, owner.publicKey, type, to, [], TOKEN_2022);
  const tx = new Transaction().add(
    set(AuthorityType.MintTokens),
    set(AuthorityType.TransferFeeConfig),
    set(AuthorityType.WithheldWithdraw),
    set(AuthorityType.MetadataPointer),
    createUpdateAuthorityInstruction({ programId: TOKEN_2022, metadata: mint, oldAuthority: owner.publicKey, newAuthority: to }),
  );
  log(`Права на токен → ${to.toBase58().slice(0, 8)}…`, await sendAndConfirmTransaction(connection, tx, [owner], opts));
}

// ------------------------------------------------------------------------------------------
console.log(`Владелец ${owner.publicKey.toBase58()}`);
console.log(`Второй   ${friend.publicKey.toBase58()}`);
console.log(`Основной ${MY_WALLET.toBase58()}\n`);

const mintFile = new URL('../keys/mint.json', here);
const existing = existsSync(mintFile) ? keypair('../keys/mint.json').publicKey : null;
const [command, arg] = process.argv.slice(2);

if (command === 'info') {
  await info(existing);
} else if (command === 'handover') {
  await handover(existing, new PublicKey(arg ?? MY_WALLET));
  await info(existing);
} else {
  const mint = existing ?? (await createToken());
  await demo(mint);
  await info(mint);
}
