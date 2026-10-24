// Проверка опубликованного кода, сохранённых devnet-транзакций и совместимости старых данных.
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { Connection, PublicKey } from '@solana/web3.js';
const here = new URL('.',import.meta.url);
const file = process.env.POSITION_STATE ?? new URL('../deployment-week13.json',here);
const state = JSON.parse(readFileSync(file));
const previous = JSON.parse(readFileSync(new URL('../deployment-week12.json',here)));
assert.equal(state.cluster,'devnet');
const connection = new Connection(process.env.RPC_URL ?? 'https://api.devnet.solana.com','finalized');
assert.equal(await connection.getGenesisHash(),'EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG');
const key = x => new PublicKey(x);
const hash = d => createHash('sha256').update(d).digest('hex');
const binary = readFileSync(new URL('../target/deploy/vault.so',here));
const program = await connection.getAccountInfo(key(state.program));
assert(program.executable);
const programData = new PublicKey(program.data.subarray(4,36));
const deployed = await connection.getAccountInfo(programData);
assert(deployed.data.subarray(45,45+binary.length).equals(binary),'Другая версия программы');
assert(deployed.data.subarray(45+binary.length).every(x=>x===0),'Ненулевой остаток bytecode');
assert.equal(new PublicKey(deployed.data.subarray(13,45)).toBase58(),state.admin);
const legacy = previous.deployment.legacy;
const infos = await connection.getMultipleAccountsInfo(Object.values(legacy).map(x=>key(x.address)));
Object.entries(legacy).forEach(([name,expected],i)=>{
  assert.equal(infos[i].owner.toBase58(),expected.owner,name);
  assert.equal(infos[i].data.length,expected.dataSize,name);
  assert.equal(hash(infos[i].data),expected.dataSha256,name);
});
const entries = Object.entries(state.transactions);
const statuses = (await connection.getSignatureStatuses(entries.map(([,tx])=>tx.signature),{searchTransactionHistory:true})).value;
for (let offset=0;offset<entries.length;offset+=3) {
  const batch = entries.slice(offset,offset+3);
  let transactions;
  for (let retry=0;retry<6;retry++) {
    try {
      transactions = await connection.getParsedTransactions(batch.map(([,t])=>t.signature),{commitment:'finalized',maxSupportedTransactionVersion:0});
      break;
    } catch (error) {
      if (error.code !== 429 || retry === 5) throw error;
      await new Promise(resolve=>setTimeout(resolve,5000*(retry+1)));
    }
  }
  // Некоторые RPC возвращают batch в другом порядке: сопоставляем именно подписи.
  for (const [label,saved] of batch) {
    const tx = transactions.find(t=>t?.transaction.signatures[0]===saved.signature);
    assert(tx,`Нет ${label}`); assert.equal(tx.meta.err,null,label); assert.equal(tx.slot,saved.slot,label);
    const index = entries.findIndex(([name])=>name===label);
    assert.equal(statuses[index]?.confirmationStatus,'finalized',label);
    const trader = ['traderSwapA1','traderSwapA2','traderSwapB','moveOutsideRange'].includes(label);
    assert.equal(tx.transaction.message.accountKeys[0].pubkey.toBase58(),trader?state.trader:state.admin,label);
    const op = state.operations[label];
    const delta = address => {
      const idx=tx.transaction.message.accountKeys.findIndex(a=>a.pubkey.toBase58()===address);
      const balance = list => BigInt(list?.find(b=>b.accountIndex===idx)?.uiTokenAmount.amount??'0');
      return balance(tx.meta.postTokenBalances)-balance(tx.meta.preTokenBalances);
    };
    if(op?.spentA!==undefined) {
      assert.equal((-delta(state.tokenVault)).toString(),op.spentA,label);
      assert.equal((-delta(state.quoteVault)).toString(),op.spentB,label);
    }
    if(op?.returnedA!==undefined) {
      assert.equal(delta(state.tokenVault).toString(),op.returnedA,label);
      assert.equal(delta(state.quoteVault).toString(),op.returnedB,label);
    }
    if(trader) assert(tx.transaction.message.instructions.some(i=>i.programId.toBase58()===previous.clmmProgram));
    else if(label!=='fundTrader') assert(tx.transaction.message.instructions.some(i=>i.programId.toBase58()===state.program));
    saved.confirmationStatus='finalized';
  }
  await new Promise(resolve=>setTimeout(resolve,2000));
}
state.deployment = { ...state.deployment, programData:programData.toBase58(),deployedSlot:Number(deployed.data.readBigUInt64LE(4)),
  bytecodeSize:binary.length,allocatedBytecodeSize:deployed.data.length-45,bytecodeSha256:hash(binary),legacy,
  legacyTmrAccountsUnchanged:true,upgradeAuthority:state.admin };
state.verification = { ...state.verification, transactionSignersAndTokenDeltasVerified:true,
  deployedBytecodeMatches:true, legacyTmrAccountsUnchanged:true };
state.verifiedAt=new Date().toISOString();
writeFileSync(file,`${JSON.stringify(state,null,2)}\n`);
console.log({transactionsVerified:entries.length,bytecodeSha256:hash(binary),legacyAccountsUnchanged:true});
