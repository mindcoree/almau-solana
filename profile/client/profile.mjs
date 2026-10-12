// Клиент программы профилей в Devnet на @anchor-lang/core.
//
//   cd client && npm install
//   node profile.mjs              # демо: создать, обновить, проверить уникальность имени,
//                                 # второй профиль, поиск по имени, список, удаление
//   node profile.mjs show timur   # найти профиль по имени
//   node profile.mjs list         # все профили в программе
//   node profile.mjs update "описание" "ссылка"   # изменить свой профиль
//
// Кошельки: ../keys/wallet.json — основной, ../keys/friend.json — второй для проверки
// уникальности имени (создаётся сам и получает 0,02 SOL с основного).
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import anchor from '@anchor-lang/core';

const { AnchorProvider, Program, Wallet, web3 } = anchor;

const here = new URL('.', import.meta.url);
const idl = JSON.parse(readFileSync(new URL('../idl/profile.json', here), 'utf8'));
const connection = new web3.Connection(process.env.RPC_URL ?? web3.clusterApiUrl('devnet'), 'confirmed');

function loadKeypair(file, createIfMissing = false) {
  const path = new URL(file, here);
  if (!existsSync(path) && createIfMissing) {
    writeFileSync(path, JSON.stringify(Array.from(web3.Keypair.generate().secretKey)));
  }
  return web3.Keypair.fromSecretKey(Uint8Array.from(JSON.parse(readFileSync(path))));
}

const programFor = (keypair) =>
  new Program(idl, new AnchorProvider(connection, new Wallet(keypair), { commitment: 'confirmed' }));

const me = loadKeypair(process.env.WALLET ?? '../keys/wallet.json');
const program = programFor(me);

// Адреса — те же сиды, что в программе.
const pda = (...seeds) => web3.PublicKey.findProgramAddressSync(seeds, program.programId)[0];
const profileAddress = (owner) => pda(Buffer.from('profile'), owner.toBuffer());
const usernameAddress = (username) => pda(Buffer.from('username'), Buffer.from(username));

const explorer = (sig) => `https://explorer.solana.com/tx/${sig}?cluster=devnet`;
const date = (ts) => new Date(Number(ts) * 1000).toISOString().replace('T', ' ').slice(0, 19);

function print(address, p) {
  console.log(`  @${p.username}  ${address.toBase58()}`);
  console.log(`    owner    ${p.owner.toBase58()}`);
  console.log(`    bio      ${p.bio}`);
  console.log(`    url      ${p.url}`);
  console.log(`    created  ${date(p.createdAt)}   updated ${date(p.updatedAt)}`);
}

// Поиск по имени: запись реестра ["username", имя] → владелец → профиль ["profile", владелец].
async function findByUsername(username) {
  const record = await program.account.usernameRecord.fetchNullable(usernameAddress(username));
  if (!record) return null;
  const address = profileAddress(record.owner);
  return { address, profile: await program.account.profile.fetch(address) };
}

// Все профили программы: getProgramAccounts с фильтром по дискриминатору Profile.
async function listAll() {
  const all = await program.account.profile.all();
  console.log(`Профилей в программе: ${all.length}`);
  for (const { publicKey, account } of all) print(publicKey, account);
}

async function step(label, promise) {
  const sig = await promise;
  console.log(`${label.padEnd(26)} ${explorer(sig)}`);
  return sig;
}

async function demo() {
  // 1. Профиль основного кошелька: create (если его ещё нет) и update.
  //    В .accounts() — только owner: profile и username_record Anchor вычислит по IDL.
  if (!(await program.account.profile.fetchNullable(profileAddress(me.publicKey)))) {
    await step(
      'create_profile @timur',
      program.methods.createProfile('timur', 'Изучаю Solana', 'https://solana.com').accounts({ owner: me.publicKey }).rpc(),
    );
  }
  await step(
    'update_profile @timur',
    program.methods
      .updateProfile('Изучаю Solana. Неделя 6: on-chain профиль на PDA', 'https://solana.com/docs/core/pda')
      .accounts({ owner: me.publicKey })
      .rpc(),
  );

  // 2. Второй кошелёк пробует занять то же имя — программа не даст.
  const friend = loadKeypair('../keys/friend.json', true);
  const friendProgram = programFor(friend);
  if ((await connection.getBalance(friend.publicKey)) < 0.01 * web3.LAMPORTS_PER_SOL) {
    const tx = new web3.Transaction().add(
      web3.SystemProgram.transfer({ fromPubkey: me.publicKey, toPubkey: friend.publicKey, lamports: 0.02 * web3.LAMPORTS_PER_SOL }),
    );
    await program.provider.sendAndConfirm(tx);
  }
  try {
    await friendProgram.methods.createProfile('timur', 'я тоже timur', '').accounts({ owner: friend.publicKey }).rpc();
    console.log('!!! второй профиль @timur создался — так быть не должно');
  } catch (err) {
    const logs = err.logs ?? err.transactionLogs ?? [];
    const reason = logs.find((l) => l.includes('already in use')) ?? String(err.message).split('\n')[0];
    console.log(`create_profile @timur (второй кошелёк) отклонено: ${reason.replace(/^Allocate: /, '')}`);
  }

  // 3. Второй кошелёк создаёт профиль со своим именем.
  if (!(await friendProgram.account.profile.fetchNullable(profileAddress(friend.publicKey)))) {
    await step(
      'create_profile @sol_friend',
      friendProgram.methods.createProfile('sol_friend', 'Второй кошелёк для проверки', '').accounts({ owner: friend.publicKey }).rpc(),
    );
  }

  // 4. Поиск по имени и список всех профилей.
  console.log('\nПоиск по имени @timur:');
  const found = await findByUsername('timur');
  print(found.address, found.profile);
  console.log();
  await listAll();

  // 5. Второй кошелёк удаляет профиль: rent возвращается, имя освобождается.
  const before = await connection.getBalance(friend.publicKey);
  console.log();
  await step('close_profile @sol_friend', friendProgram.methods.closeProfile().accounts({ owner: friend.publicKey }).rpc());
  const back = (await connection.getBalance(friend.publicKey)) - before;
  console.log(`  вернулось ${back / web3.LAMPORTS_PER_SOL} SOL (rent двух аккаунтов минус комиссия)`);
  console.log(`  имя @sol_friend свободно: ${!(await findByUsername('sol_friend'))}`);
}

console.log(`Программа ${program.programId.toBase58()}`);
console.log(`Кошелёк   ${me.publicKey.toBase58()}\n`);

const [command, arg] = process.argv.slice(2);
if (command === 'show') {
  const found = await findByUsername(arg);
  if (found) print(found.address, found.profile);
  else console.log(`@${arg} не найден`);
} else if (command === 'list') {
  await listAll();
} else if (command === 'update') {
  const [bio, url = ''] = process.argv.slice(3);
  await step('update_profile', program.methods.updateProfile(bio, url).accounts({ owner: me.publicKey }).rpc());
  const address = profileAddress(me.publicKey);
  print(address, await program.account.profile.fetch(address));
} else {
  await demo();
}
