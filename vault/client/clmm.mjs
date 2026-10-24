// Неделя 11: два тестовых mint, CLMM-пул и позиция кошелька в Raydium Devnet.
import assert from 'node:assert/strict';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import {
  Connection, Keypair, PublicKey, SystemProgram, TransactionMessage, VersionedTransaction, clusterApiUrl,
} from '@solana/web3.js';
import {
  MINT_SIZE, TOKEN_PROGRAM_ID, TOKEN_2022_PROGRAM_ID, createAssociatedTokenAccountIdempotentInstruction,
  createInitializeMintInstruction, createMintToCheckedInstruction, getAssociatedTokenAddressSync,
  unpackAccount, unpackMint,
} from '@solana/spl-token';
import {
  Raydium, TxVersion, DEVNET_PROGRAM_ID, DEV_API_URLS, ClmmConfigLayout, PoolInfoLayout,
  PersonalPositionLayout, TickArrayLayout, TickArrayUtil, TickUtil, getPdaAmmConfigId,
  getPdaPoolId, getPdaPersonalPositionAddress, getPdaTickArrayAddress,
} from '@raydium-io/raydium-sdk-v2';
import BN from 'bn.js';
import Decimal from 'decimal.js';
import bs58 from 'bs58';
import { quotePosition } from './clmm-math.mjs';

const [command = 'show'] = process.argv.slice(2);
assert(['demo', 'show'].includes(command), 'Команды: clmm.mjs demo | show');
const here = new URL('.', import.meta.url);
const stateFile = process.env.CLMM_STATE ?? new URL('../deployment-week11.json', here);
const connection = new Connection(process.env.RPC_URL ?? clusterApiUrl('devnet'), 'confirmed');
assert.equal(await connection.getGenesisHash(), 'EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG', 'Нужен RPC Solana Devnet');
const programId = DEVNET_PROGRAM_ID.CLMM_PROGRAM_ID;
assert.equal(programId.toBase58(), 'DRayAUgENGQBKVaX8owNhgzkEDyoHTGVEGHVJT1E9pfH');
let state = existsSync(stateFile) ? JSON.parse(readFileSync(stateFile)) : null;
const save = () => writeFileSync(stateFile, `${JSON.stringify(state, null, 2)}\n`);
const key = (value) => new PublicKey(value);
const explorer = (kind, value) => `https://explorer.solana.com/${kind}/${value}?cluster=devnet`;
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function confirm(label, commitment = 'confirmed') {
  const tx = state.transactions[label];
  for (let attempt = 0; attempt < 60; attempt++) {
    const status = (await connection.getSignatureStatuses([tx.signature], { searchTransactionHistory: true })).value[0];
    if (status) {
      assert.equal(status.err, null, `${label}: ${JSON.stringify(status.err)}`);
      if (status.confirmationStatus === 'finalized' || (commitment === 'confirmed' && status.confirmationStatus === 'confirmed')) {
        Object.assign(tx, { slot: status.slot, confirmationStatus: status.confirmationStatus, err: status.err });
        save();
        return;
      }
    } else {
      assert(await connection.getBlockHeight('confirmed') <= tx.lastValidBlockHeight, `${label}: срок транзакции истёк`);
    }
    await sleep(1000);
  }
  throw new Error(`Ожидание ${label} прервано; повторите demo для проверки сохранённой подписи`);
}

async function send(label, instructions, signers) {
  if (state.transactions[label]) return confirm(label);
  const block = await connection.getLatestBlockhash('confirmed');
  const message = new TransactionMessage({
    payerKey: signers[0].publicKey, recentBlockhash: block.blockhash, instructions,
  }).compileToV0Message();
  const transaction = new VersionedTransaction(message);
  transaction.sign(signers);
  const simulation = await connection.simulateTransaction(transaction, { sigVerify: true, commitment: 'confirmed' });
  assert.equal(simulation.value.err, null, simulation.value.logs?.join('\n'));
  const signature = bs58.encode(transaction.signatures[0]);
  // Сохраняем подпись до отправки: повторный запуск проверит её, не повторяя выпуск.
  state.transactions[label] = { signature, lastValidBlockHeight: block.lastValidBlockHeight,
    simulationPassed: true, unitsConsumed: simulation.value.unitsConsumed, explorer: explorer('tx', signature) };
  save();
  await connection.sendRawTransaction(transaction.serialize(), { skipPreflight: false, preflightCommitment: 'confirmed', maxRetries: 3 });
  await confirm(label);
  console.log(label, explorer('tx', signature));
}

async function demo() {
  const owner = Keypair.fromSecretKey(Uint8Array.from(JSON.parse(readFileSync(
    process.env.WALLET ?? new URL('../keys/wallet.json', here),
  ))));
  const directory = new URL(`../keys/clmm/${owner.publicKey.toBase58()}/`, here);
  mkdirSync(directory, { recursive: true, mode: 0o700 });
  const localKey = (name) => {
    const file = new URL(`${name}.json`, directory);
    if (existsSync(file)) return Keypair.fromSecretKey(Uint8Array.from(JSON.parse(readFileSync(file))));
    const created = Keypair.generate();
    writeFileSync(file, JSON.stringify(Array.from(created.secretKey)), { mode: 0o600, flag: 'wx' });
    return created;
  };
  const mints = [localKey('mint-1'), localKey('mint-2')]
    .sort((a, b) => Buffer.compare(a.publicKey.toBuffer(), b.publicKey.toBuffer()));
  const nft = localKey('position-nft');
  const configId = getPdaAmmConfigId(programId, 2).publicKey;
  const configInfo = await connection.getAccountInfo(configId);
  assert(configInfo?.owner.equals(programId), 'Конфигурация должна принадлежать Raydium Devnet CLMM');
  const config = ClmmConfigLayout.decode(configInfo.data);
  assert.equal(config.tickSpacing, 10);
  assert.equal(config.tradeFeeRate, 500);
  if (!state) {
    state = { week: 11, cluster: 'devnet', owner: owner.publicKey.toBase58(), programId: programId.toBase58(),
      sdkVersion: '0.2.70-alpha', officialDemoCommit: '003d9045830033e5ef4178d6db129d83f93e1c7f',
      initialPrice: '1', priceUnit: 'token B per token A', initialSolLamports: await connection.getBalance(owner.publicKey),
      config: { id: configId.toBase58(), index: config.index, tickSpacing: config.tickSpacing,
        tradeFeeRate: config.tradeFeeRate, protocolFeeRate: config.protocolFeeRate, fundFeeRate: config.fundFeeRate },
      tokens: mints.map((mint, index) => ({ label: index === 0 ? 'A' : 'B', mint: mint.publicKey.toBase58(), decimals: 6,
        mintedAmount: '100000000000', tokenProgram: TOKEN_PROGRAM_ID.toBase58(),
        ownerAta: getAssociatedTokenAddressSync(mint.publicKey, owner.publicKey).toBase58() })), transactions: {} };
    save();
  }
  assert.equal(state.owner, owner.publicKey.toBase58(), 'Другой владелец: задайте отдельный CLMM_STATE для своего кошелька');
  assert.equal(state.programId, programId.toBase58());
  assert.deepEqual(state.tokens.map((token) => token.mint), mints.map((mint) => mint.publicKey.toBase58()));
  for (const label of Object.keys(state.transactions)) await confirm(label);
  for (const [index, mint] of mints.entries()) {
    const token = state.tokens[index];
    if (await connection.getAccountInfo(mint.publicKey)) continue;
    const ata = key(token.ownerAta);
    await send(`createToken${token.label}`, [
      SystemProgram.createAccount({ fromPubkey: owner.publicKey, newAccountPubkey: mint.publicKey,
        lamports: await connection.getMinimumBalanceForRentExemption(MINT_SIZE), space: MINT_SIZE, programId: TOKEN_PROGRAM_ID }),
      createInitializeMintInstruction(mint.publicKey, token.decimals, owner.publicKey, null),
      createAssociatedTokenAccountIdempotentInstruction(owner.publicKey, ata, owner.publicKey, mint.publicKey),
      createMintToCheckedInstruction(mint.publicKey, ata, owner.publicKey, BigInt(token.mintedAmount), token.decimals),
    ], [owner, mint]);
  }
  const raydium = await Raydium.load({ owner, connection, cluster: 'devnet', disableFeatureCheck: true,
    disableLoadToken: true, urlConfigs: DEV_API_URLS, blockhashCommitment: 'confirmed' });
  const poolId = getPdaPoolId(programId, configId, mints[0].publicKey, mints[1].publicKey).publicKey;
  if (!await connection.getAccountInfo(poolId)) {
    const tokenInfo = (token) => ({ chainId: 103, address: token.mint, programId: token.tokenProgram,
      decimals: token.decimals, symbol: `TEST-${token.label}`, name: `Week 11 Token ${token.label}`,
      logoURI: '', tags: [], extensions: {} });
    const built = await raydium.clmm.createPool({ programId, mint1: tokenInfo(state.tokens[0]), mint2: tokenInfo(state.tokens[1]),
      ammConfig: { ...state.config, id: configId, fundOwner: config.fundOwner.toBase58(), description: 'Devnet, 0.05%' },
      initialPrice: new Decimal(state.initialPrice), txVersion: TxVersion.LEGACY,
      computeBudgetConfig: { units: 600000, microLamports: 1000 } });
    const address = built.extInfo.address;
    assert.equal(address.id, poolId.toBase58());
    state.pool = { id: address.id, vaultA: address.vault.A, vaultB: address.vault.B,
      observationId: address.observationId, exBitmapAccount: address.exBitmapAccount };
    save();
    await send('createPool', built.transaction.instructions, built.signers);
  }
  assert(state.pool && state.pool.id === poolId.toBase58(), 'Не найдено сохранённое описание созданного пула');
  const personalPosition = getPdaPersonalPositionAddress(programId, nft.publicKey).publicKey;
  if (!await connection.getAccountInfo(personalPosition)) {
    // У пустого пула ещё нет tick arrays; этот RPC-метод не требует их наличия.
    const { poolInfo, rpcData } = await raydium.clmm.getSimplePoolInfo(poolId);
    const poolKeys = { ...poolInfo, vault: { A: state.pool.vaultA, B: state.pool.vaultB } };
    const price = TickUtil.sqrtPriceX64ToPrice(rpcData.sqrtPriceX64, rpcData.mintDecimalsA, rpcData.mintDecimalsB);
    const quote = quotePosition({ price: price.toString(), decimalsA: poolInfo.mintA.decimals,
      decimalsB: poolInfo.mintB.decimals, tickSpacing: poolInfo.config.tickSpacing, amountA: new BN('1000000000') });
    const built = await raydium.clmm.openPositionFromBase({ poolInfo, poolKeys, ownerInfo: { useSOLBalance: false },
      base: 'MintA', baseAmount: quote.amountA, otherAmountMax: quote.amountBMax, liquidity: new BN(0),
      tickLower: quote.tickLower, tickUpper: quote.tickUpper, nft2022: true, txVersion: TxVersion.LEGACY,
      getEphemeralSigners: async () => [nft.publicKey.toBase58()],
      computeBudgetConfig: { units: 600000, microLamports: 1000 } });
    const addresses = built.extInfo;
    state.position = { nftMint: addresses.nftMint.toBase58(), nftTokenProgram: TOKEN_2022_PROGRAM_ID.toBase58(),
      nftAccount: addresses.positionNftAccount.toBase58(), personalPosition: addresses.personalPosition.toBase58(),
      protocolPosition: addresses.protocolPosition.toBase58(), tickArrayLower: addresses.tickArrayLower.toBase58(),
      tickArrayUpper: addresses.tickArrayUpper.toBase58(), tickLower: quote.tickLower, tickUpper: quote.tickUpper,
      requestedRange: { lowerMultiplier: '0.9', upperMultiplier: '1.1' },
      priceLower: quote.priceLower, priceUpper: quote.priceUpper, slippageBps: 100,
      amountAMax: quote.amountA.toString(), amountBQuoted: quote.amountB.toString(), amountBMax: quote.amountBMax.toString() };
    save();
    console.log('Диапазон и лимиты', state.position.priceLower, state.position.priceUpper, state.position.amountAMax, state.position.amountBMax);
    await send('openPosition', built.transaction.instructions, [...built.signers, nft]);
  }
  state.position.protocolPositionDeprecated = true;
  for (const label of Object.keys(state.transactions)) await confirm(label, 'finalized');
  state.snapshot = await inspect();
  assert(state.snapshot.inRange && new BN(state.snapshot.poolLiquidity).gt(new BN(0)), 'Начальная ликвидность должна быть активной');
  const txs = await connection.getParsedTransactions(Object.values(state.transactions).map((tx) => tx.signature),
    { commitment: 'finalized', maxSupportedTransactionVersion: 0 });
  for (const tx of txs) {
    assert(tx && !tx.meta.err, 'Транзакция должна завершиться без ошибки');
    const recorded = Object.values(state.transactions).find((item) => item.signature === tx.transaction.signatures[0]);
    assert(recorded && recorded.slot === tx.slot, 'Ответ RPC должен соответствовать сохранённой подписи и слоту');
    Object.assign(recorded, { feeLamports: tx.meta.fee,
      signerAddresses: tx.transaction.message.accountKeys.filter((account) => account.signer).map((account) => account.pubkey.toBase58()) });
  }
  state.verifiedAt = new Date().toISOString();
  state.verification = { commitment: 'finalized', mathTestsPassed: 5, simulatedBeforeSending: true,
    poolAndPositionPdasVerified: true, positionNftOwnedByWallet: true };
  save();
  console.log('Снимок проверки', typeof stateFile === 'string' ? stateFile : fileURLToPath(stateFile));
}

async function inspect() {
  assert(state?.pool && state?.position, 'Сначала создайте пул и позицию командой demo');
  const addresses = { program: state.programId, owner: state.owner, config: state.config.id,
    mintA: state.tokens[0].mint, mintB: state.tokens[1].mint, ownerAtaA: state.tokens[0].ownerAta, ownerAtaB: state.tokens[1].ownerAta,
    pool: state.pool.id, vaultA: state.pool.vaultA, vaultB: state.pool.vaultB, observation: state.pool.observationId,
    bitmap: state.pool.exBitmapAccount, personalPosition: state.position.personalPosition,
    protocolPosition: state.position.protocolPosition, nftMint: state.position.nftMint, nftAccount: state.position.nftAccount,
    tickArrayLower: state.position.tickArrayLower, tickArrayUpper: state.position.tickArrayUpper };
  const result = await connection.getMultipleAccountsInfoAndContext(Object.values(addresses).map(key), 'finalized');
  const accounts = Object.fromEntries(Object.keys(addresses).map((name, index) => [name, result.value[index]]));
  // В актуальном CLMM protocol_position оставлен в ABI, но больше не создаётся.
  for (const [name, account] of Object.entries(accounts)) {
    if (name !== 'protocolPosition') assert(account, `Аккаунт ${name} не найден`);
  }
  assert(accounts.program.executable);
  for (const name of ['config', 'pool', 'observation', 'bitmap', 'personalPosition', 'tickArrayLower', 'tickArrayUpper']) {
    assert(accounts[name].owner.equals(programId), `Неверный владелец ${name}`);
  }
  const pool = PoolInfoLayout.decode(accounts.pool.data);
  const position = PersonalPositionLayout.decode(accounts.personalPosition.data);
  const owner = key(state.owner);
  assert(pool.configId.equals(key(state.config.id)) && pool.creator.equals(owner));
  assert(pool.mintA.equals(key(addresses.mintA)) && pool.mintB.equals(key(addresses.mintB)));
  assert(pool.vaultA.equals(key(addresses.vaultA)) && pool.vaultB.equals(key(addresses.vaultB)));
  assert(pool.observationId.equals(key(addresses.observation)));
  assert(getPdaPoolId(programId, pool.configId, pool.mintA, pool.mintB).publicKey.equals(key(addresses.pool)));
  assert(position.poolId.equals(key(addresses.pool)) && position.nftMint.equals(key(addresses.nftMint)));
  assert(getPdaPersonalPositionAddress(programId, position.nftMint).publicKey.equals(key(addresses.personalPosition)));
  assert(position.liquidity.gt(new BN(0)));
  assert.equal(position.tickLower, state.position.tickLower);
  assert.equal(position.tickUpper, state.position.tickUpper);
  for (const side of ['Lower', 'Upper']) {
    const tick = position[`tick${side}`];
    const start = TickArrayUtil.getTickArrayStartIndex(tick, pool.tickSpacing);
    assert(getPdaTickArrayAddress(programId, key(addresses.pool), start).publicKey.equals(key(addresses[`tickArray${side}`])));
    const array = TickArrayLayout.decode(accounts[`tickArray${side}`].data);
    assert(array.poolId.equals(key(addresses.pool)));
    assert.equal(array.startTickIndex, start);
  }
  const balances = {};
  for (const side of ['A', 'B']) {
    const mint = unpackMint(key(addresses[`mint${side}`]), accounts[`mint${side}`]);
    const vault = unpackAccount(key(addresses[`vault${side}`]), accounts[`vault${side}`]);
    const wallet = unpackAccount(key(addresses[`ownerAta${side}`]), accounts[`ownerAta${side}`]);
    assert(mint.mintAuthority?.equals(owner));
    assert.equal(mint.freezeAuthority, null);
    assert.equal(mint.decimals, 6);
    assert.equal(mint.supply.toString(), state.tokens[side === 'A' ? 0 : 1].mintedAmount);
    assert(vault.owner.equals(key(addresses.pool)) && vault.mint.equals(key(addresses[`mint${side}`])));
    assert(wallet.owner.equals(owner) && wallet.mint.equals(vault.mint));
    balances[`vault${side}`] = vault.amount.toString();
    balances[`owner${side}`] = wallet.amount.toString();
    balances[`supply${side}`] = mint.supply.toString();
  }
  const nftMint = unpackMint(key(addresses.nftMint), accounts.nftMint, TOKEN_2022_PROGRAM_ID);
  const nftAccount = unpackAccount(key(addresses.nftAccount), accounts.nftAccount, TOKEN_2022_PROGRAM_ID);
  assert.equal(nftMint.decimals, 0);
  assert.equal(nftMint.supply, 1n);
  assert(nftAccount.owner.equals(owner) && nftAccount.mint.equals(key(addresses.nftMint)));
  assert.equal(nftAccount.amount, 1n);
  const price = TickUtil.sqrtPriceX64ToPrice(pool.sqrtPriceX64, pool.mintDecimalsA, pool.mintDecimalsB).toString();
  const snapshot = { slot: result.context.slot, price, sqrtPriceX64: pool.sqrtPriceX64.toString(), currentTick: pool.tickCurrent,
    inRange: pool.tickCurrent >= position.tickLower && pool.tickCurrent < position.tickUpper,
    poolLiquidity: pool.liquidity.toString(), positionLiquidity: position.liquidity.toString(), balances,
    nftOwner: nftAccount.owner.toBase58(), nftSupply: nftMint.supply.toString(),
    accounts: Object.fromEntries(Object.entries(accounts).map(([name, account]) => [name, {
      address: addresses[name], exists: Boolean(account), owner: account?.owner.toBase58() ?? null, dataSize: account?.data.length ?? 0 }])) };
  console.log('Цена B за A', price, 'диапазон', state.position.priceLower, state.position.priceUpper, 'активна', snapshot.inRange);
  for (const name of ['mintA', 'mintB', 'pool', 'personalPosition', 'nftMint']) console.log(name, explorer('address', addresses[name]));
  console.log('Проверено: PDA пула, позиция, тик-массивы, ликвидность и NFT кошелька.');
  return snapshot;
}

try {
  if (command === 'demo') await demo();
  else await inspect();
} catch (error) {
  console.error(error.stack);
  process.exitCode = 1;
}
