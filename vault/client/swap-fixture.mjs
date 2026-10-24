// Публичный снимок Raydium для настоящих CPI-тестов LiteSVM; вывод только в target/.
import assert from 'node:assert/strict';
import { readFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { Connection, PublicKey, clusterApiUrl } from '@solana/web3.js';
import { quoteSwap } from './swap-quote.mjs';

const here = new URL('.', import.meta.url);
const clmm = JSON.parse(readFileSync(new URL('../deployment-week11.json', here)));
const connection = new Connection(process.env.RPC_URL ?? clusterApiUrl('devnet'), 'confirmed');
assert.equal(await connection.getGenesisHash(), 'EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG');
const programId = new PublicKey(clmm.programId);
const program = await connection.getAccountInfo(programId);
assert(program?.executable);
const programData = new PublicKey(program.data.subarray(4, 36));
const bytecode = (await connection.getAccountInfo(programData)).data.subarray(45);
const quote = await quoteSwap(connection, clmm.pool.id, '10000000');
const addresses = { assetMint: clmm.tokens[0].mint, quoteMint: clmm.tokens[1].mint,
  poolState: clmm.pool.id, ammConfig: clmm.config.id, inputVault: clmm.pool.vaultA,
  outputVault: clmm.pool.vaultB, observation: clmm.pool.observationId,
  ...Object.fromEntries(quote.remainingAccounts.map((address, index) => [`remaining${index}`, address])) };
const result = await connection.getMultipleAccountsInfoAndContext(Object.values(addresses).map((address) => new PublicKey(address)));
const accounts = Object.fromEntries(Object.entries(addresses).map(([name, address], index) => {
  const account = result.value[index];
  assert(account, `Missing ${name}`);
  return [name, { address, owner: account.owner.toBase58(), lamports: account.lamports,
    executable: account.executable, data: Array.from(account.data) }];
}));
const directory = new URL('../target/fixtures/', here);
mkdirSync(directory, { recursive: true });
writeFileSync(new URL('raydium-clmm.so', directory), bytecode);
writeFileSync(new URL('clmm.json', directory), JSON.stringify({ slot: result.context.slot, blockTimestamp: Math.floor(Date.now() / 1000), programId: clmm.programId,
  programData: programData.toBase58(), bytecodeSha256: createHash('sha256').update(bytecode).digest('hex'),
  quote, accounts }));
console.log('Raydium fixture:', { slot: result.context.slot, programId: clmm.programId, bytes: bytecode.length,
  expectedOut: quote.expectedOut, remainingAccounts: quote.remainingAccounts });
