// Клиент Counter Program в Devnet на @anchor-lang/core.
//
//   cd client && npm install
//   node counter.mjs          # создать счётчик (если его нет), +1 три раза, −1 один раз
//   node counter.mjs show     # только показать счётчик
//
// В отличие от клиента недели 4, байты инструкций здесь руками не собираются: Anchor читает
// IDL (../idl/counter.json — его генерирует `anchor build`) и сам кодирует вызов по имени
// метода. Адрес счётчика (PDA) и System Program он тоже подставляет сам — они описаны в IDL.
import { readFileSync } from 'node:fs';
import anchor from '@anchor-lang/core';

const { AnchorProvider, Program, Wallet, web3 } = anchor;

const here = new URL('.', import.meta.url);
const idl = JSON.parse(readFileSync(new URL('../idl/counter.json', here), 'utf8'));
const keypair = web3.Keypair.fromSecretKey(
  Uint8Array.from(JSON.parse(readFileSync(process.env.WALLET ?? new URL('../keys/wallet.json', here)))),
);

const connection = new web3.Connection(process.env.RPC_URL ?? web3.clusterApiUrl('devnet'), 'confirmed');
const provider = new AnchorProvider(connection, new Wallet(keypair), { commitment: 'confirmed' });
const program = new Program(idl, provider);

// Тот же PDA, что в программе: seeds = ["counter", authority].
const [counterPda] = web3.PublicKey.findProgramAddressSync(
  [Buffer.from('counter'), keypair.publicKey.toBuffer()],
  program.programId,
);

const explorer = (sig) => `https://explorer.solana.com/tx/${sig}?cluster=devnet`;

async function call(method) {
  // .accounts() — только authority: counter и systemProgram Anchor вычислит по IDL.
  const sig = await program.methods[method]().accounts({ authority: keypair.publicKey }).rpc();
  const { count } = await program.account.counter.fetch(counterPda);
  console.log(`${method.padEnd(10)} count = ${count.toString().padEnd(3)} ${explorer(sig)}`);
  return sig;
}

async function show() {
  const counter = await program.account.counter.fetchNullable(counterPda);
  if (!counter) return console.log('Счётчика ещё нет');
  console.log(`Счётчик  ${counterPda.toBase58()}`);
  console.log(`  count      ${counter.count.toString()}`);
  console.log(`  authority  ${counter.authority.toBase58()}`);
  console.log(`  bump       ${counter.bump}`);
}

console.log(`Программа ${program.programId.toBase58()}`);
console.log(`Кошелёк   ${keypair.publicKey.toBase58()}\n`);

if (process.argv[2] !== 'show') {
  if (!(await program.account.counter.fetchNullable(counterPda))) await call('initialize');
  await call('increment');
  await call('increment');
  await call('increment');
  const last = await call('decrement');

  // Логи последней транзакции — видно, что Anchor сам пишет имя инструкции.
  const tx = await connection.getTransaction(last, { commitment: 'confirmed', maxSupportedTransactionVersion: 0 });
  console.log('\nЛоги decrement:');
  for (const line of tx.meta.logMessages) console.log(`  ${line}`);
  console.log();
}
await show();
