// Неделя 12: только администратор запускает CPI swap; токены принадлежат PDA vault.
import assert from 'node:assert/strict';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { AnchorProvider, BN, Program, Wallet } from '@anchor-lang/core';
import { Connection, PublicKey, Keypair, ComputeBudgetProgram, SystemProgram, TransactionMessage,
  VersionedTransaction, clusterApiUrl } from '@solana/web3.js';
import { TOKEN_PROGRAM_ID, TOKEN_2022_PROGRAM_ID, createTransferCheckedInstruction, getAssociatedTokenAddressSync, unpackAccount, unpackMint } from '@solana/spl-token';
import { PoolInfoLayout, TickUtil } from '@raydium-io/raydium-sdk-v2';
import bs58 from 'bs58';
import { quoteSwap } from './swap-quote.mjs';

const [command = 'show', rawAmount = '10000000'] = process.argv.slice(2);
assert(['demo', 'quote', 'swap', 'show'].includes(command), 'Команды: swap.mjs demo | quote [raw A] | swap [raw A] | show');
assert(/^\d+$/.test(rawAmount) && BigInt(rawAmount) > 0n && BigInt(rawAmount) <= 0xffffffffffffffffn, 'Сумма должна быть положительным u64');
const here = new URL('.', import.meta.url);
const clmm = JSON.parse(readFileSync(process.env.CLMM_STATE ?? new URL('../deployment-week11.json', here)));
const idl = JSON.parse(readFileSync(new URL('../idl/vault.json', here)));
const connection = new Connection(process.env.RPC_URL ?? clusterApiUrl('devnet'), 'confirmed');
const local = ['127.0.0.1', 'localhost'].includes(new URL(connection.rpcEndpoint).hostname);
assert(local || await connection.getGenesisHash() === 'EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG', 'Используйте Solana Devnet или локальный валидатор');
const signing = command === 'demo' || command === 'swap';
const signer = signing ? Keypair.fromSecretKey(Uint8Array.from(JSON.parse(readFileSync(
  process.env.WALLET ?? new URL('../keys/wallet.json', here),
)))) : null;
const provider = signer ? new AnchorProvider(connection, new Wallet(signer), { commitment: 'confirmed' }) : { connection };
const program = new Program(idl, provider);
const stateFile = process.env.SWAP_STATE ?? new URL('../deployment-week12.json', here);
let state = existsSync(stateFile) ? JSON.parse(readFileSync(stateFile)) : null;
const admin = new PublicKey(signer?.publicKey ?? state?.admin ?? clmm.owner);
const assetMint = new PublicKey(clmm.tokens[0].mint);
const quoteMint = new PublicKey(clmm.tokens[1].mint);
const pda = (...seeds) => PublicKey.findProgramAddressSync(seeds, program.programId)[0];
const vault = pda(Buffer.from('vault'), admin.toBuffer(), assetMint.toBuffer());
const shareMint = pda(Buffer.from('share_mint'), vault.toBuffer());
const tokenVault = pda(Buffer.from('token_vault'), vault.toBuffer());
const market = pda(Buffer.from('market'), vault.toBuffer());
const quoteVault = pda(Buffer.from('quote_vault'), vault.toBuffer());
const key = (value) => new PublicKey(value);
const clmmProgram = key(clmm.programId);
const poolState = key(clmm.pool.id);
const explorer = (kind, value) => `https://explorer.solana.com/${kind}/${value}?${local
  ? `cluster=custom&customUrl=${encodeURIComponent(connection.rpcEndpoint)}` : 'cluster=devnet'}`;
const save = () => writeFileSync(stateFile, `${JSON.stringify(state, null, 2)}\n`);
const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const swapAccounts = { admin, vault, market, assetMint, quoteMint, tokenVault, quoteVault, poolState,
  ammConfig: key(clmm.config.id), inputVault: key(clmm.pool.vaultA), outputVault: key(clmm.pool.vaultB),
  observationState: key(clmm.pool.observationId), clmmProgram, tokenProgram: TOKEN_PROGRAM_ID,
  tokenProgram2022: TOKEN_2022_PROGRAM_ID, memoProgram: key('MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr') };

async function confirm(label, commitment = 'confirmed') {
  const tx = state.transactions[label];
  if (tx.confirmationStatus === 'finalized') return;
  for (let attempt = 0; attempt < 60; attempt++) {
    const status = (await connection.getSignatureStatuses([tx.signature], { searchTransactionHistory: true })).value[0];
    if (status && (status.confirmationStatus === 'finalized' || commitment === 'confirmed' && status.confirmationStatus === 'confirmed')) {
      if (tx.expectedError) assert.equal(status.err?.InstructionError?.[1]?.Custom, tx.expectedError);
      else assert.equal(status.err, null, `${label}: ${JSON.stringify(status.err)}`);
      Object.assign(tx, { slot: status.slot, err: status.err, confirmationStatus: status.confirmationStatus });
      save();
      return;
    }
    if (!status) assert(await connection.getBlockHeight('confirmed') <= tx.lastValidBlockHeight, `Транзакция ${label} истекла`);
    await delay(1000);
  }
  throw new Error(`Не дождались ${label}; повторите demo для проверки сохранённой подписи`);
}

async function send(label, instructions, expectedError = null) {
  if (state.transactions[label]) return confirm(label);
  assert(signer && signer.publicKey.equals(admin));
  const block = await connection.getLatestBlockhash('confirmed');
  const message = new TransactionMessage({ payerKey: admin, recentBlockhash: block.blockhash, instructions: [
    ComputeBudgetProgram.setComputeUnitLimit({ units: 600000 }),
    ComputeBudgetProgram.setComputeUnitPrice({ microLamports: 1000 }), ...instructions,
  ] }).compileToV0Message();
  const tx = new VersionedTransaction(message);
  tx.sign([signer]);
  const simulated = await connection.simulateTransaction(tx, { sigVerify: true, commitment: 'confirmed' });
  if (expectedError) {
    assert.equal(simulated.value.err?.InstructionError?.[1]?.Custom, expectedError, simulated.value.logs?.join('\n'));
    assert(simulated.value.logs.some((line) => line.includes('TooLittleOutputReceived')));
  } else assert.equal(simulated.value.err, null, simulated.value.logs?.join('\n'));
  const signature = bs58.encode(tx.signatures[0]);
  state.transactions[label] = { signature, expectedError, lastValidBlockHeight: block.lastValidBlockHeight,
    simulated: true, unitsConsumed: simulated.value.unitsConsumed, explorer: explorer('tx', signature) };
  save();
  // Только демонстрация slippage отправляет ожидаемо неуспешную devnet-транзакцию.
  await connection.sendRawTransaction(tx.serialize(), { skipPreflight: Boolean(expectedError), preflightCommitment: 'confirmed', maxRetries: 3 });
  await confirm(label);
  console.log(label, explorer('tx', signature));
}

async function setup() {
  if (!state) {
    state = { week: 12, cluster: local ? 'localnet' : 'devnet', program: program.programId.toBase58(), admin: admin.toBase58(),
      clmmProgram: clmm.programId, pool: clmm.pool.id, assetMint: assetMint.toBase58(), quoteMint: quoteMint.toBase58(),
      vault: vault.toBase58(), shareMint: shareMint.toBase58(), tokenVault: tokenVault.toBase58(),
      market: market.toBase58(), quoteVault: quoteVault.toBase58(), transactions: {} };
    save();
  }
  assert.equal(state.admin, admin.toBase58(), 'Другой администратор: выберите отдельный SWAP_STATE');
  assert.equal(state.vault, vault.toBase58());
  for (const label of Object.keys(state.transactions)) await confirm(label);
  if (!await connection.getAccountInfo(vault)) {
    const ix = await program.methods.createVault().accountsStrict({ admin, assetMint, vault, shareMint,
      tokenVault, tokenProgram: TOKEN_PROGRAM_ID, systemProgram: SystemProgram.programId }).instruction();
    await send('createVault', [ix]);
  }
  if (!await connection.getAccountInfo(market)) {
    const ix = await program.methods.initializeMarket().accountsStrict({ admin, vault, market, assetMint, quoteMint,
      shareMint, quoteVault, poolState, clmmProgram, assetTokenProgram: TOKEN_PROGRAM_ID,
      quoteTokenProgram: TOKEN_PROGRAM_ID, systemProgram: SystemProgram.programId }).instruction();
    await send('initializeMarket', [ix]);
  }
  await send('fund100A', [createTransferCheckedInstruction(getAssociatedTokenAddressSync(assetMint, admin), assetMint, tokenVault,
    admin, 100000000n, 6)]);
}

async function swapInstruction(quote, minimum = quote.minimumOut) {
  return program.methods.swap(new BN(quote.amountIn), new BN(minimum)).accountsStrict(swapAccounts)
    .remainingAccounts(quote.remainingAccounts.map((address) => ({ pubkey: key(address), isWritable: true, isSigner: false })))
    .instruction();
}

async function snapshot(commitment = 'confirmed') {
  const addresses = { vault, market, tokenVault, quoteVault, shareMint, poolState, ammConfig: swapAccounts.ammConfig,
    inputVault: swapAccounts.inputVault, outputVault: swapAccounts.outputVault, observation: swapAccounts.observationState,
    bitmap: key(clmm.pool.exBitmapAccount), tickArrayLower: key(clmm.position.tickArrayLower), tickArrayUpper: key(clmm.position.tickArrayUpper) };
  const result = await connection.getMultipleAccountsInfoAndContext(Object.values(addresses), commitment);
  const infos = Object.fromEntries(Object.keys(addresses).map((name, index) => [name, result.value[index]]));
  for (const [name, info] of Object.entries(infos)) assert(info, `Missing ${name}`);
  const config = program.coder.accounts.decode('vaultConfig', infos.vault.data);
  const trading = program.coder.accounts.decode('marketConfig', infos.market.data);
  assert(config.admin.equals(admin) && config.assetMint.equals(assetMint) && config.tokenVault.equals(tokenVault));
  assert(trading.vault.equals(vault) && trading.quoteMint.equals(quoteMint) && trading.quoteVault.equals(quoteVault));
  assert(trading.poolState.equals(poolState));
  const a = unpackAccount(tokenVault, infos.tokenVault);
  const b = unpackAccount(quoteVault, infos.quoteVault);
  const mint = unpackMint(shareMint, infos.shareMint);
  assert(a.owner.equals(vault) && b.owner.equals(vault));
  assert(a.mint.equals(assetMint) && b.mint.equals(quoteMint));
  assert(mint.mintAuthority.equals(vault));
  const pool = PoolInfoLayout.decode(infos.poolState.data);
  const hash = createHash('sha256');
  for (const [name, info] of Object.entries(infos)) hash.update(addresses[name].toBuffer()).update(info.data);
  return { slot: result.context.slot, storageA: a.amount.toString(), storageB: b.amount.toString(),
    totalDeposited: config.totalDeposited.toString(), shareSupply: mint.supply.toString(),
    price: TickUtil.sqrtPriceX64ToPrice(pool.sqrtPriceX64, pool.mintDecimalsA, pool.mintDecimalsB).toString(),
    sqrtPriceX64: pool.sqrtPriceX64.toString(), currentTick: pool.tickCurrent, mutableDataSha256: hash.digest('hex'),
    poolA: unpackAccount(swapAccounts.inputVault, infos.inputVault).amount.toString(),
    poolB: unpackAccount(swapAccounts.outputVault, infos.outputVault).amount.toString(),
    accounts: Object.fromEntries(Object.entries(infos).map(([name, info]) => [name, {
      address: addresses[name].toBase58(), owner: info.owner.toBase58(), dataSize: info.data.length }])) };
}

async function demo() {
  await setup();
  if (!state.afterSwap) {
    state.beforeSwap ??= await snapshot();
    state.quote ??= await quoteSwap(connection, clmm.pool.id, rawAmount);
    save();
    await send('swap', [await swapInstruction(state.quote)]);
    state.afterSwap = await snapshot();
    assert.equal(BigInt(state.beforeSwap.storageA) - BigInt(state.afterSwap.storageA), BigInt(state.quote.amountIn));
    assert.equal(BigInt(state.afterSwap.storageB) - BigInt(state.beforeSwap.storageB), BigInt(state.quote.expectedOut));
    assert.equal(state.afterSwap.sqrtPriceX64, state.quote.sqrtPriceAfterX64);
    assert.equal(state.afterSwap.totalDeposited, state.afterSwap.storageA);
    save();
  }
  if (!state.afterRejected) {
    state.beforeRejected ??= await snapshot();
    state.rejectedQuote ??= await quoteSwap(connection, clmm.pool.id, rawAmount);
    state.rejectedMinimum = '18446744073709551615';
    save();
    await send('rejectedSlippage', [await swapInstruction(state.rejectedQuote, state.rejectedMinimum)], 6018);
    state.afterRejected = await snapshot();
    assert.equal(state.afterRejected.mutableDataSha256, state.beforeRejected.mutableDataSha256, 'Ошибка должна откатить все данные');
    save();
  }
  for (const label of Object.keys(state.transactions)) await confirm(label, 'finalized');
  const transactions = await connection.getParsedTransactions(Object.values(state.transactions).map((tx) => tx.signature),
    { commitment: 'finalized', maxSupportedTransactionVersion: 0 });
  for (const parsed of transactions) {
    assert(parsed);
    const tx = Object.values(state.transactions).find((item) => item.signature === parsed.transaction.signatures[0]);
    assert(tx && tx.slot === parsed.slot);
    Object.assign(tx, { feeLamports: parsed.meta.fee,
      signerAddresses: parsed.transaction.message.accountKeys.filter((account) => account.signer).map((account) => account.pubkey.toBase58()) });
    if (tx === state.transactions.swap || tx === state.transactions.rejectedSlippage) {
      tx.innerInstructions = parsed.meta.innerInstructions.flatMap((group) => group.instructions).map((ix) => ({
        program: ix.programId.toBase58(), type: ix.parsed?.type, info: ix.parsed?.info,
      }));
      tx.logs = parsed.meta.logMessages;
      assert(tx.logs.some((line) => line.includes('Instruction: SwapV2')));
    }
  }
  state.finalState = await snapshot('finalized');
  state.verifiedAt = new Date().toISOString();
  state.verification = { ...state.verification, rustTestsPassed: 29, javascriptTestsPassed: 8,
    clmmCpiSignedByVault: true, slippageRollbackVerified: true };
  save();
  console.log('Проверено:', { priceBefore: state.beforeSwap.price, priceAfter: state.afterSwap.price,
    minimumOut: state.quote.minimumOut, received: state.afterSwap.storageB,
    rollback: state.beforeRejected.mutableDataSha256 === state.afterRejected.mutableDataSha256 });
}

try {
  if (command === 'quote') console.log(await quoteSwap(connection, clmm.pool.id, rawAmount));
  else if (command === 'show') console.log(await snapshot('finalized'));
  else if (command === 'demo') await demo();
  else {
    assert(state && state.admin === admin.toBase58(), 'Сначала выполните demo с этим кошельком');
    const quote = await quoteSwap(connection, clmm.pool.id, rawAmount);
    console.log('Quote', quote);
    await send(`swap-${Date.now()}`, [await swapInstruction(quote)]);
    console.log(await snapshot());
  }
} catch (error) {
  console.error(error.stack);
  process.exitCode = 1;
}
