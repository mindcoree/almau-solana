// Вызвать задеплоенную копилку в Devnet: Initialize → Deposit → Withdraw.
//
//   cd client && npm install && node demo.mjs
//
// Кошелёк — ../keys/wallet.json (или путь в WALLET), программа — из ../src/lib.rs
// (или адрес в PROGRAM_ID). Данные инструкций собираются вручную — те же байты
// Borsh, что в src/instruction.rs: [номер варианта][u64 little-endian].
import { readFileSync } from 'node:fs';
import {
  Connection,
  Keypair,
  LAMPORTS_PER_SOL,
  PublicKey,
  SystemProgram,
  Transaction,
  TransactionInstruction,
  clusterApiUrl,
  sendAndConfirmTransaction,
} from '@solana/web3.js';

const here = new URL('.', import.meta.url);
const connection = new Connection(process.env.RPC_URL ?? clusterApiUrl('devnet'), 'confirmed');
const wallet = Keypair.fromSecretKey(
  Uint8Array.from(JSON.parse(readFileSync(process.env.WALLET ?? new URL('../keys/wallet.json', here)))),
);
const programId = new PublicKey(
  process.env.PROGRAM_ID ??
    readFileSync(new URL('../src/lib.rs', here), 'utf8').match(/declare_id!\("(\w+)"\)/)[1],
);

// PDA копилки — те же сиды, что в программе: ["vault", owner].
const [vault] = PublicKey.findProgramAddressSync(
  [Buffer.from('vault'), wallet.publicKey.toBuffer()],
  programId,
);

const u64 = (n) => {
  const bytes = Buffer.alloc(8);
  bytes.writeBigUInt64LE(BigInt(n));
  return bytes;
};
const owner = { pubkey: wallet.publicKey, isSigner: true, isWritable: true };
const vaultMeta = { pubkey: vault, isSigner: false, isWritable: true };
const system = { pubkey: SystemProgram.programId, isSigner: false, isWritable: false };

const instructions = {
  initialize: () => ({ data: Buffer.from([0]), keys: [owner, vaultMeta, system] }),
  deposit: (lamports) => ({ data: Buffer.concat([Buffer.from([1]), u64(lamports)]), keys: [owner, vaultMeta, system] }),
  withdraw: (lamports) => ({ data: Buffer.concat([Buffer.from([2]), u64(lamports)]), keys: [owner, vaultMeta] }),
  close: () => ({ data: Buffer.from([3]), keys: [owner, vaultMeta] }),
};

async function send(label, { data, keys }) {
  const tx = new Transaction().add(new TransactionInstruction({ programId, keys, data }));
  const signature = await sendAndConfirmTransaction(connection, tx, [wallet]);
  console.log(`${label.padEnd(10)} https://explorer.solana.com/tx/${signature}?cluster=devnet`);
  return signature;
}

// Разбор 57 байт данных копилки — раскладка из src/state.rs.
async function showVault() {
  const account = await connection.getAccountInfo(vault);
  if (!account) return console.log('Копилки нет');
  const d = account.data;
  console.log(`Копилка   ${vault.toBase58()}`);
  console.log(`  баланс           ${account.lamports / LAMPORTS_PER_SOL} SOL`);
  console.log(`  owner            ${new PublicKey(d.subarray(8, 40)).toBase58()}`);
  console.log(`  bump             ${d[40]}`);
  console.log(`  total_deposited  ${Number(d.readBigUInt64LE(41)) / LAMPORTS_PER_SOL} SOL`);
  console.log(`  total_withdrawn  ${Number(d.readBigUInt64LE(49)) / LAMPORTS_PER_SOL} SOL`);
}

console.log(`Программа ${programId.toBase58()}`);
console.log(`Кошелёк   ${wallet.publicKey.toBase58()}\n`);

if (process.argv.includes('--close')) {
  await send('Close', instructions.close());
} else {
  // Повторный запуск не падает на «already in use»: копилку создаём, только если её нет.
  if (!(await connection.getAccountInfo(vault))) await send('Initialize', instructions.initialize());
  await send('Deposit', instructions.deposit(0.1 * LAMPORTS_PER_SOL));
  await send('Withdraw', instructions.withdraw(0.05 * LAMPORTS_PER_SOL));
}
console.log();
await showVault();
