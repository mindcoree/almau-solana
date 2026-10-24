// Неделя 13: NFT и оба актива принадлежат vault; rent оплачивает отдельный PDA.
import assert from 'node:assert/strict';
import { existsSync, readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { AnchorProvider, BN, Program, Wallet } from '@anchor-lang/core';
import { Connection, PublicKey, Keypair, ComputeBudgetProgram, SystemProgram, TransactionMessage,
  VersionedTransaction, SYSVAR_RENT_PUBKEY, clusterApiUrl } from '@solana/web3.js';
import { TOKEN_PROGRAM_ID, TOKEN_2022_PROGRAM_ID, ASSOCIATED_TOKEN_PROGRAM_ID, getAssociatedTokenAddressSync,
  createAssociatedTokenAccountIdempotentInstruction, createTransferCheckedInstruction, unpackAccount } from '@solana/spl-token';
import { PoolInfoLayout, LiquidityMathUtil, TickUtil } from '@raydium-io/raydium-sdk-v2';
import bs58 from 'bs58';
import { quotePosition } from './clmm-math.mjs';
import { quoteSwap } from './swap-quote.mjs';

const [command = 'show', amount = '8000000', range = '0.05'] = process.argv.slice(2);
assert(['show', 'demo', 'open', 'collect', 'close', 'back'].includes(command), 'Команды: show | demo | open [raw A] [range 0.05 или lower:upper 0.95:1.05] | collect | close | back');
const here = new URL('.', import.meta.url);
const clmm = JSON.parse(readFileSync(new URL('../deployment-week11.json', here)));
const week12 = JSON.parse(readFileSync(new URL('../deployment-week12.json', here)));
const idl = JSON.parse(readFileSync(new URL('../idl/vault.json', here)));
const rayIdl = JSON.parse(readFileSync(new URL('../idls/raydium_clmm.json', here)));
const connection = new Connection(process.env.RPC_URL ?? clusterApiUrl('devnet'), 'confirmed');
const local = ['localhost', '127.0.0.1'].includes(new URL(connection.rpcEndpoint).hostname);
assert(local || await connection.getGenesisHash() === 'EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG', 'Только devnet или localhost');
const signer = command === 'show' ? null : Keypair.fromSecretKey(Uint8Array.from(JSON.parse(readFileSync(process.env.WALLET ?? new URL('../keys/wallet.json', here)))));
const provider = signer ? new AnchorProvider(connection, new Wallet(signer), { commitment: 'confirmed' }) : { connection };
const program = new Program(idl, provider);
const raydium = new Program(rayIdl, provider);
const key = (s) => new PublicKey(s);
const admin = key(week12.admin);
if (signer) assert(signer.publicKey.equals(admin), 'Требуется кошелёк администратора существующего vault');
const vault = key(week12.vault), market = key(week12.market), tokenVault = key(week12.tokenVault), quoteVault = key(week12.quoteVault);
const assetMint = key(week12.assetMint), quoteMint = key(week12.quoteMint), poolState = key(clmm.pool.id), clmmProgram = key(clmm.programId);
const pda = (...s) => PublicKey.findProgramAddressSync(s, program.programId)[0];
const rayPda = (...s) => PublicKey.findProgramAddressSync(s, clmmProgram)[0];
const positionState = pda(Buffer.from('vault_position'), vault.toBuffer());
const liquidityPayer = pda(Buffer.from('liquidity_payer'), vault.toBuffer());
const bitmap = key(clmm.pool.exBitmapAccount);
const memoProgram = key('MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr');
const stateFile = process.env.POSITION_STATE ?? new URL('../deployment-week13.json', here);
assert(!local || process.env.POSITION_STATE, 'Для localhost задайте отдельный POSITION_STATE');
let state = existsSync(stateFile) ? JSON.parse(readFileSync(stateFile)) : {
  week: 13, cluster: local ? 'localnet' : 'devnet', program: program.programId.toBase58(), admin: admin.toBase58(),
  vault: vault.toBase58(), market: market.toBase58(), pool: poolState.toBase58(), assetMint: assetMint.toBase58(), quoteMint: quoteMint.toBase58(),
  tokenVault: tokenVault.toBase58(), quoteVault: quoteVault.toBase58(), positionState: positionState.toBase58(),
  liquidityPayer: liquidityPayer.toBase58(), transactions: {}, operations: {} };
assert.equal(state.cluster, local ? 'localnet' : 'devnet');
assert.equal(state.vault, vault.toBase58());
const save = () => writeFileSync(stateFile, `${JSON.stringify(state, null, 2)}\n`);
const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const explorer = (sig) => `https://explorer.solana.com/tx/${sig}?${local ? `cluster=custom&customUrl=${encodeURIComponent(connection.rpcEndpoint)}` : 'cluster=devnet'}`;

async function confirm(label, finalized = false) {
  const tx = state.transactions[label];
  if (tx.confirmationStatus === 'finalized') return;
  for (let i = 0; i < 90; i++) {
    const status = (await connection.getSignatureStatuses([tx.signature], { searchTransactionHistory: true })).value[0];
    if (status && (status.confirmationStatus === 'finalized' || !finalized && status.confirmationStatus === 'confirmed')) {
      assert.equal(status.err, null, JSON.stringify(status.err));
      Object.assign(tx, { slot: status.slot, confirmationStatus: status.confirmationStatus }); save(); return;
    }
    if (!status) assert(await connection.getBlockHeight() <= tx.lastValidBlockHeight, `Истекла транзакция ${label}; проверьте Explorer`);
    await delay(1000);
  }
  throw new Error(`Нет подтверждения ${label}; подпись сохранена, повторите команду`);
}
async function send(label, instructions, signers = [signer]) {
  if (state.transactions[label]) return confirm(label);
  const block = await connection.getLatestBlockhash();
  const message = new TransactionMessage({ payerKey: signers[0].publicKey, recentBlockhash: block.blockhash,
    instructions: [ComputeBudgetProgram.setComputeUnitLimit({ units: 900000 }), ...instructions] }).compileToV0Message();
  const tx = new VersionedTransaction(message); tx.sign(signers);
  const serialized = tx.serialize(); assert(serialized.length <= 1232, `Размер транзакции ${serialized.length} > 1232`);
  const simulation = await connection.simulateTransaction(tx, { sigVerify: true });
  assert.equal(simulation.value.err, null, simulation.value.logs?.join('\n'));
  const signature = bs58.encode(tx.signatures[0]);
  state.transactions[label] = { signature, lastValidBlockHeight: block.lastValidBlockHeight, serializedBytes: serialized.length,
    unitsConsumed: simulation.value.unitsConsumed, explorer: explorer(signature) }; save();
  await connection.sendRawTransaction(serialized, { preflightCommitment: 'confirmed', maxRetries: 3 });
  await confirm(label); console.log(label, explorer(signature));
}
function privateKey(name) {
  const file = new URL(`../keys/week13/${local ? 'local' : 'devnet'}-${name}.json`, here);
  if (existsSync(file)) return Keypair.fromSecretKey(Uint8Array.from(JSON.parse(readFileSync(file))));
  mkdirSync(new URL('.', file), { recursive: true });
  const result = Keypair.generate(); writeFileSync(file, JSON.stringify(Array.from(result.secretKey)), { mode: 0o600 }); return result;
}
const common = { admin, vault, market, positionState, assetMint, quoteMint, tokenVault, quoteVault, poolState,
  poolVaultA: key(clmm.pool.vaultA), poolVaultB: key(clmm.pool.vaultB), clmmProgram,
  tokenProgram: TOKEN_PROGRAM_ID, tokenProgram2022: TOKEN_2022_PROGRAM_ID, systemProgram: SystemProgram.programId };
function lpAccounts(nftMint, lower, upper, spacing) {
  const i32 = (v) => { const b = Buffer.alloc(4); b.writeInt32BE(v); return b; };
  const array = (tick) => rayPda(Buffer.from('tick_array'), poolState.toBuffer(), i32(Math.floor(tick / (spacing * 60)) * spacing * 60));
  return { nftMint, nftAccount: getAssociatedTokenAddressSync(nftMint, vault, true, TOKEN_2022_PROGRAM_ID),
    personalPosition: rayPda(Buffer.from('position'), nftMint.toBuffer()),
    protocolPosition: rayPda(Buffer.from('position'), poolState.toBuffer(), i32(lower), i32(upper)),
    tickArrayLower: array(lower), tickArrayUpper: array(upper) };
}
const remaining = [{ pubkey: bitmap, isWritable: true, isSigner: false }];
async function snapshot() {
  const addresses = [tokenVault, quoteVault, poolState, positionState, liquidityPayer];
  const result = await connection.getMultipleAccountsInfoAndContext(addresses);
  const [a, b, poolInfo, marker, payer] = result.value;
  const tokenA = unpackAccount(tokenVault, a), tokenB = unpackAccount(quoteVault, b);
  assert(tokenA.owner.equals(vault) && tokenB.owner.equals(vault), 'Владелец активов должен быть vault');
  assert.equal(payer?.lamports ?? 0, 0, 'Неиспользованный rent должен быть возвращён');
  const pool = PoolInfoLayout.decode(poolInfo.data);
  const out = { slot: result.context.slot, storageA: tokenA.amount.toString(), storageB: tokenB.amount.toString(),
    price: TickUtil.sqrtPriceX64ToPrice(pool.sqrtPriceX64, pool.mintDecimalsA, pool.mintDecimalsB).toString(),
    sqrtPriceX64: pool.sqrtPriceX64.toString(), tick: pool.tickCurrent, tickSpacing: pool.tickSpacing, position: null };
  if (marker?.data.length) {
    assert(marker.owner.equals(program.programId));
    const stored = program.coder.accounts.decode('vaultPosition', marker.data);
    assert(stored.vault.equals(vault));
    const personal = await raydium.account.personalPositionState.fetch(stored.personalPosition);
    assert(personal.poolId.equals(poolState) && personal.nftMint.equals(stored.nftMint));
    const acc = lpAccounts(stored.nftMint, personal.tickLowerIndex, personal.tickUpperIndex, pool.tickSpacing);
    const nft = unpackAccount(acc.nftAccount, await connection.getAccountInfo(acc.nftAccount), TOKEN_2022_PROGRAM_ID);
    assert(nft.owner.equals(vault) && nft.mint.equals(stored.nftMint) && nft.amount === 1n);
    out.position = { nftMint: stored.nftMint.toBase58(), nftAccount: acc.nftAccount.toBase58(), nftOwner: vault.toBase58(),
      personalPosition: stored.personalPosition.toBase58(), liquidity: personal.liquidity.toString(),
      tickLower: personal.tickLowerIndex, tickUpper: personal.tickUpperIndex,
      priceLower: TickUtil.tickToPrice(personal.tickLowerIndex, 6, 6).toString(),
      priceUpper: TickUtil.tickToPrice(personal.tickUpperIndex, 6, 6).toString(),
      inRange: pool.tickCurrent >= personal.tickLowerIndex && pool.tickCurrent < personal.tickUpperIndex };
  }
  return out;
}
async function open(label, rawA, width) {
  const op = state.operations[label] ??= {}; if (op.after) return op;
  op.before ??= await snapshot(); assert(!op.before.position, 'Сначала закройте текущую позицию');
  if (!op.quote) {
    assert(/^\d+$/.test(rawA));
    const q = quotePosition({ price: op.before.price, decimalsA: 6, decimalsB: 6,
      tickSpacing: op.before.tickSpacing, amountA: new BN(rawA),
      ...(width.includes(':') ? { lowerPrice: width.split(':')[0], upperPrice: width.split(':')[1] } : { range: width }) });
    op.quote = Object.fromEntries(Object.entries(q).map(([k,v]) => [k, BN.isBN(v) ? v.toString() : v]));
    op.quote.maximumA = new BN(rawA).muln(101).addn(99).divn(100).toString(); save();
  }
  const nft = privateKey(label), q = op.quote;
  const accounts = lpAccounts(nft.publicKey, q.tickLower, q.tickUpper, op.before.tickSpacing);
  const ix = await program.methods.openClmmPosition(q.tickLower, q.tickUpper, new BN(q.liquidity), new BN(q.maximumA),
    new BN(q.amountBMax), new BN(200000000)).accountsStrict({ ...common, liquidityPayer, ...accounts,
    associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID, rent: SYSVAR_RENT_PUBKEY }).remainingAccounts(remaining).instruction();
  await send(label, [ix], [signer,nft]);
  op.after = await snapshot(); assert.equal(op.after.position?.nftMint, nft.publicKey.toBase58());
  assert.equal(op.after.position.liquidity, q.liquidity);
  op.spentA = (BigInt(op.before.storageA)-BigInt(op.after.storageA)).toString();
  op.spentB = (BigInt(op.before.storageB)-BigInt(op.after.storageB)).toString();
  assert(BigInt(op.spentA) <= BigInt(q.maximumA) && BigInt(op.spentB) <= BigInt(q.amountBMax)); save(); return op;
}
async function manage(label, close) {
  const op = state.operations[label] ??= {}; if (op.after) return op;
  op.before ??= await snapshot(); const pos = op.before.position; assert(pos, 'Нет позиции vault');
  const accounts = { ...common, ...lpAccounts(key(pos.nftMint), pos.tickLower, pos.tickUpper, op.before.tickSpacing), memoProgram };
  let method;
  if (close) {
    const amounts = LiquidityMathUtil.getAmountsForLiquidity(new BN(op.before.sqrtPriceX64), TickUtil.getSqrtPriceAtTick(pos.tickLower),
      TickUtil.getSqrtPriceAtTick(pos.tickUpper), new BN(pos.liquidity), false);
    op.minimumA ??= amounts.amountA.muln(99).divn(100).toString();
    op.minimumB ??= amounts.amountB.muln(99).divn(100).toString();
    method = program.methods.closeClmmPosition(new BN(op.minimumA), new BN(op.minimumB));
  } else method = program.methods.collectFees();
  save(); await send(label, [await method.accountsStrict(accounts).remainingAccounts(remaining).instruction()]);
  op.after = await snapshot(); op.returnedA = (BigInt(op.after.storageA)-BigInt(op.before.storageA)).toString();
  op.returnedB = (BigInt(op.after.storageB)-BigInt(op.before.storageB)).toString();
  if (close) {
    assert(!op.after.position);
    for (const address of [pos.nftMint,pos.nftAccount,pos.personalPosition]) assert(!(await connection.getAccountInfo(key(address)))?.data.length, 'NFT и personal position должны быть закрыты');
    assert(BigInt(op.returnedA) >= BigInt(op.minimumA) && BigInt(op.returnedB) >= BigInt(op.minimumB));
  } else assert.equal(op.after.position.liquidity, pos.liquidity);
  save(); return op;
}
async function swapVault(label, back, raw) {
  const op = state.operations[label] ??= {}; if (op.after) return op;
  op.before ??= await snapshot();
  op.quote ??= await quoteSwap(connection, poolState.toBase58(), raw ?? op.before.storageB, !back); save();
  const ix = await program.methods[back ? 'swapBack' : 'swap'](new BN(op.quote.amountIn), new BN(op.quote.minimumOut)).accountsStrict({
    admin,vault,market,assetMint,quoteMint,tokenVault,quoteVault,poolState,ammConfig:key(clmm.config.id),
    inputVault:key(clmm.pool.vaultA),outputVault:key(clmm.pool.vaultB),observationState:key(clmm.pool.observationId),
    clmmProgram,tokenProgram:TOKEN_PROGRAM_ID,tokenProgram2022:TOKEN_2022_PROGRAM_ID,memoProgram,
  }).remainingAccounts(op.quote.remainingAccounts.map((v)=>({pubkey:key(v),isWritable:true,isSigner:false}))).instruction();
  await send(label,[ix]); op.after = await snapshot(); save(); return op;
}
async function fundTrader(trader) {
  const ataA = getAssociatedTokenAddressSync(assetMint,trader.publicKey), ataB = getAssociatedTokenAddressSync(quoteMint,trader.publicKey);
  state.trader = trader.publicKey.toBase58(); assert(!trader.publicKey.equals(admin)); save();
  await send('fundTrader', [SystemProgram.transfer({fromPubkey:admin,toPubkey:trader.publicKey,lamports:100000000}),
    ...[[assetMint,ataA,500000000n],[quoteMint,ataB,100000000n]].flatMap(([mint,ata,value])=>[
      createAssociatedTokenAccountIdempotentInstruction(admin,ata,trader.publicKey,mint),
      createTransferCheckedInstruction(getAssociatedTokenAddressSync(mint,admin),mint,ata,admin,value,6),
    ])]);
}
async function trade(label, trader, raw, aToB = true) {
  const op = state.operations[label] ??= {}; if (op.after) return op;
  op.before ??= await snapshot(); op.quote ??= await quoteSwap(connection,poolState.toBase58(),raw,aToB); save();
  const ix = await raydium.methods.swapV2(new BN(raw),new BN(op.quote.minimumOut),new BN(0),true).accountsStrict({
    payer:trader.publicKey,ammConfig:key(clmm.config.id),poolState,
    inputTokenAccount:getAssociatedTokenAddressSync(aToB?assetMint:quoteMint,trader.publicKey),
    outputTokenAccount:getAssociatedTokenAddressSync(aToB?quoteMint:assetMint,trader.publicKey),
    inputVault:key(aToB?clmm.pool.vaultA:clmm.pool.vaultB),outputVault:key(aToB?clmm.pool.vaultB:clmm.pool.vaultA),
    observationState:key(clmm.pool.observationId),tokenProgram:TOKEN_PROGRAM_ID,tokenProgram2022:TOKEN_2022_PROGRAM_ID,memoProgram,
    inputVaultMint:aToB?assetMint:quoteMint,outputVaultMint:aToB?quoteMint:assetMint,
  }).remainingAccounts(op.quote.remainingAccounts.map((v)=>({pubkey:key(v),isWritable:true,isSigner:false}))).instruction();
  await send(label,[ix],[trader]); op.after=await snapshot(); save(); return op;
}
async function demo() {
  await open('openInitial','8000000','0.05');
  const trader = privateKey('trader'); await fundTrader(trader);
  await trade('traderSwapA1',trader,'25000000'); await trade('traderSwapA2',trader,'20000000'); await trade('traderSwapB',trader,'10000000',false);
  const fees = await manage('harvestInitial',false); assert(BigInt(fees.returnedA)>0n && BigInt(fees.returnedB)>0n, 'Нужны комиссии обоих активов');
  await manage('closeInitial',true);
  const back = await swapVault('convertAllB',true); assert.equal(back.after.storageB,'0');
  await swapVault('prepareRebalance',false,'10000000');
  await open('openNarrow','5000000','0.005');
  const moved = await trade('moveOutsideRange',trader,'150000000'); assert.equal(moved.after.position.inRange,false);
  await manage('closeOutsideRange',true);
  await open('reopenNewRange','3000000','0.05');
  for (const label of Object.keys(state.transactions)) await confirm(label,true);
  state.final = await snapshot(); state.verification = { ...state.verification, fullCycle:true, thirdPartyTrader:true, earnedBothFees:true,
    fullCloseAndBurn:true, allBConvertedToA:true, outOfRangeAndRebalance:true, tokenAuthoritiesRestored:true };
  save(); console.log(JSON.stringify({feesA:fees.returnedA,feesB:fees.returnedB,final:state.final},null,2));
}
try {
  if (command === 'show') console.log(JSON.stringify(await snapshot(),null,2));
  else if (command === 'demo') await demo();
  else {
    const label = `${command}-${Date.now()}`;
    if (command === 'open') await open(label,amount,range);
    else if (command === 'back') await swapVault(label,true);
    else await manage(label,command === 'close');
    await confirm(label,true); state.final=await snapshot(); save(); console.log(JSON.stringify(state.final,null,2));
  }
} catch (error) { console.error(error.stack); process.exitCode=1; }
