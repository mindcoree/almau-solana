import assert from 'node:assert/strict';
import { PublicKey } from '@solana/web3.js';
import { Raydium, DEV_API_URLS, DEVNET_PROGRAM_ID, swapInternal, getPdaExBitmapAccount,
  TickArrayBitmapExtensionLayout, TickUtil } from '@raydium-io/raydium-sdk-v2';
import BN from 'bn.js';
import { minimumOutput } from './clmm-math.mjs';

export async function quoteSwap(connection, poolAddress, amountIn) {
  const raydium = await Raydium.load({ connection, cluster: 'devnet', disableFeatureCheck: true,
    disableLoadToken: true, urlConfigs: DEV_API_URLS });
  const poolId = new PublicKey(poolAddress);
  const { poolInfo, rpcData, configInfo, tickArrays } = await raydium.clmm.getSwapPoolInfo(poolAddress, true);
  const programId = new PublicKey(poolInfo.programId);
  assert(programId.equals(DEVNET_PROGRAM_ID.CLMM_PROGRAM_ID));
  const bitmap = getPdaExBitmapAccount(programId, poolId).publicKey;
  const bitmapInfo = await connection.getAccountInfo(bitmap);
  assert(bitmapInfo?.owner.equals(programId));
  const simulated = swapInternal({ programId, poolId, poolInfo: rpcData, tickArrays, configInfo,
    tickarrayBitmapExtension: TickArrayBitmapExtensionLayout.decode(bitmapInfo.data),
    amountSpecified: new BN(amountIn), sqrtPriceLimitX64: new BN(0), zeroForOne: true,
    isBaseInput: true, blockTimestamp: Math.floor(Date.now() / 1000), includeExtraTickArrays: true });
  assert(simulated.allTrade && simulated.amountSpecifiedRemaining.isZero(), 'Недостаточно ликвидности для полного обмена');
  return { amountIn, expectedOut: simulated.amountCalculated.toString(),
    minimumOut: minimumOutput(simulated.amountCalculated).toString(), feeAmount: simulated.feeAmount.toString(),
    priceBefore: TickUtil.sqrtPriceX64ToPrice(rpcData.sqrtPriceX64, rpcData.mintDecimalsA, rpcData.mintDecimalsB).toString(),
    priceAfter: TickUtil.sqrtPriceX64ToPrice(simulated.sqrtPriceX64, rpcData.mintDecimalsA, rpcData.mintDecimalsB).toString(),
    sqrtPriceAfterX64: simulated.sqrtPriceX64.toString(),
    remainingAccounts: [bitmap, ...simulated.accounts].map((address) => address.toBase58()) };
}
