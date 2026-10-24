import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { AnchorProvider, BN, Program, Wallet, web3 } from '@anchor-lang/core';
import {
  getAccount, getMint, getAssociatedTokenAddress, getOrCreateAssociatedTokenAccount,
  getTransferFeeAmount, TOKEN_PROGRAM_ID, TOKEN_2022_PROGRAM_ID,
} from '@solana/spl-token';

const here = new URL('.', import.meta.url);
const idl = JSON.parse(readFileSync(new URL('../idl/vault.json', here)));
const connection = new web3.Connection(process.env.RPC_URL ?? web3.clusterApiUrl('devnet'), 'confirmed');
const [command = 'demo', argument] = process.argv.slice(2);
assert(['demo', 'open', 'show', 'deposit', 'withdraw'].includes(command), 'Команды: demo, open, show [адрес], deposit <единицы токена>, withdraw <единицы shares>');
let amount;
if (command === 'deposit' || command === 'withdraw') {
  assert(/^\d+$/.test(argument ?? ''), 'Укажите целую сумму в минимальных единицах токена');
  amount = BigInt(argument);
  assert(amount > 0n && amount <= 0xffffffffffffffffn, 'Сумма должна быть положительным u64');
}

const signer = command === 'show' ? null : web3.Keypair.fromSecretKey(
  Uint8Array.from(JSON.parse(readFileSync(process.env.WALLET ?? new URL('../keys/wallet.json', here)))),
);
const provider = signer
  ? new AnchorProvider(connection, new Wallet(signer), { commitment: 'confirmed' })
  : { connection };
const program = new Program(idl, provider);
const assetMint = new web3.PublicKey(process.env.ASSET_MINT ?? 'Cq9bsK12BiRunwpBNhGG9K8Di9QX4G2xfCcoG7TmmU31');
const admin = new web3.PublicKey(process.env.ADMIN ?? (
  command === 'demo' ? signer.publicKey : '8ijYtQTzr7MSqvxxGZpGr9w3FKMkyY1W8pcUBYNymLSi'
));
const user = new web3.PublicKey((command === 'show' ? argument : undefined) ?? signer?.publicKey ?? admin);
const pda = (...seeds) => web3.PublicKey.findProgramAddressSync(seeds, program.programId)[0];
const vault = pda(Buffer.from('vault'), admin.toBuffer(), assetMint.toBuffer());
const shareMint = pda(Buffer.from('share_mint'), vault.toBuffer());
const tokenVault = pda(Buffer.from('token_vault'), vault.toBuffer());
const position = pda(Buffer.from('position'), vault.toBuffer(), user.toBuffer());
const market = pda(Buffer.from('market'), vault.toBuffer());
const explorerCluster = connection.rpcEndpoint === web3.clusterApiUrl('devnet')
  ? 'cluster=devnet'
  : `cluster=custom&customUrl=${encodeURIComponent(connection.rpcEndpoint)}`;
const explorer = (kind, address) => `https://explorer.solana.com/${kind}/${address}?${explorerCluster}`;

const assetInfo = await connection.getAccountInfo(assetMint);
assert(assetInfo, 'Принимаемый mint не найден');
const tokenProgram = assetInfo.owner;
assert([TOKEN_PROGRAM_ID, TOKEN_2022_PROGRAM_ID].some((id) => id.equals(tokenProgram)), 'Mint должен принадлежать SPL Token Program');

let config = await program.account.vaultConfig.fetchNullable(vault);
if (command === 'demo' && !config) {
  assert(admin.equals(signer.publicKey), 'Создать vault может подписавший администратор');
  const signature = await program.methods.createVault().accountsStrict({
    admin, assetMint, vault, shareMint, tokenVault, tokenProgram,
    systemProgram: web3.SystemProgram.programId,
  }).rpc();
  console.log('create_vault', explorer('tx', signature));
  config = await program.account.vaultConfig.fetch(vault);
}
assert(config, 'Vault не создан: сначала запустите demo с кошельком администратора');
if (command === 'deposit' || command === 'withdraw') {
  const trading = await connection.getAccountInfo(market);
  assert(!trading?.owner.equals(program.programId), 'Торговый vault требует учёта обоих активов перед операциями с shares');
}

let userPosition = await program.account.userPosition.fetchNullable(position);
if (signer && !userPosition) {
  assert(user.equals(signer.publicKey), 'Пользователь подписывает создание собственной позиции');
  const signature = await program.methods.openPosition().accountsStrict({
    user, vault, position, systemProgram: web3.SystemProgram.programId,
  }).rpc();
  console.log('open_position', explorer('tx', signature));
  userPosition = await program.account.userPosition.fetch(position);
}

if (command === 'deposit' || command === 'withdraw') {
  const depositing = command === 'deposit';
  const sourceMint = depositing ? assetMint : shareMint;
  const destinationMint = depositing ? shareMint : assetMint;
  const sourceAddress = await getAssociatedTokenAddress(sourceMint, user, false, tokenProgram);
  const source = await getAccount(connection, sourceAddress, 'confirmed', tokenProgram);
  assert(source.amount >= amount, depositing ? 'Недостаточно принимаемых токенов на кошельке' : 'Недостаточно shares на кошельке');
  const destinationAddress = (await getOrCreateAssociatedTokenAccount(
    connection, signer, destinationMint, user, false, 'confirmed', undefined, tokenProgram,
  )).address;
  const userTokenAccount = depositing ? sourceAddress : destinationAddress;
  const userShareAccount = depositing ? destinationAddress : sourceAddress;
  const signature = await program.methods[command](new BN(amount.toString())).accountsStrict({
    user, vault, position, assetMint, shareMint, tokenVault,
    userTokenAccount, userShareAccount, tokenProgram, market,
  }).rpc();
  console.log(command, explorer('tx', signature));
  config = await program.account.vaultConfig.fetch(vault);
  userPosition = await program.account.userPosition.fetch(position);
  console.log('user_shares', (await getAccount(connection, userShareAccount, 'confirmed', tokenProgram)).amount.toString());
}

const mint = await getMint(connection, shareMint, 'confirmed', tokenProgram);
const storage = await getAccount(connection, tokenVault, 'confirmed', tokenProgram);
assert(config.admin.equals(admin));
assert(config.assetMint.equals(assetMint));
assert(config.shareMint.equals(shareMint));
assert(config.tokenVault.equals(tokenVault));
assert(mint.mintAuthority?.equals(vault), 'Mint authority share-токена должна быть PDA vault');
assert.equal(mint.freezeAuthority, null);
assert(storage.owner.equals(vault), 'Хранилищем должна распоряжаться PDA vault');
assert(storage.mint.equals(assetMint));
assert.equal(storage.delegate, null);
assert.equal(storage.closeAuthority, null);
if (userPosition) {
  assert(userPosition.vault.equals(vault));
  assert(userPosition.owner.equals(user));
}

for (const [name, address] of Object.entries({ program: program.programId, admin, assetMint, vault, shareMint, tokenVault, position })) {
  console.log(name.padEnd(12), address.toBase58(), explorer('address', address));
}
console.log('total_deposited', config.totalDeposited.toString());
console.log('share_supply', mint.supply.toString());
console.log('storage_amount', storage.amount.toString());
console.log('storage_withheld', (getTransferFeeAmount(storage)?.withheldAmount ?? 0n).toString());
console.log('position_deposited', userPosition?.depositedAmount.toString() ?? 'позиция ещё не открыта');
console.log('Проверено: authority share-mint и хранилища — PDA vault.');
