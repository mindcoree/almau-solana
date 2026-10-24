import assert from 'node:assert/strict';
import BN from 'bn.js';
import Decimal from 'decimal.js';
import { LiquidityMathUtil, TickUtil } from '@raydium-io/raydium-sdk-v2';

export function minimumOutput(expected, slippageBps = 100) {
  assert(BN.isBN(expected) && expected.gt(new BN(0)), 'Ожидаемый выход должен быть положительным');
  assert(Number.isInteger(slippageBps) && slippageBps >= 0 && slippageBps < 10000, 'Неверный slippage');
  const minimum = expected.muln(10000 - slippageBps).divn(10000);
  assert(minimum.gt(new BN(0)), 'Минимальный выход округлился до нуля');
  return minimum;
}

export function quotePosition({ price, decimalsA, decimalsB, tickSpacing, amountA }) {
  const current = new Decimal(price);
  assert(current.isFinite() && current.gt(0), 'Цена должна быть положительной');
  assert(Number.isInteger(tickSpacing) && tickSpacing > 0, 'Неверный шаг тиков');
  assert(BN.isBN(amountA) && amountA.gt(new BN(0)), 'Нужна положительная сумма token A');
  const lower = current.mul('0.9');
  const upper = current.mul('1.1');
  const tickLower = TickUtil.toTickIndex(TickUtil.priceToTick(lower, decimalsA, decimalsB), tickSpacing);
  let tickUpper = TickUtil.toTickIndex(TickUtil.priceToTick(upper, decimalsA, decimalsB), tickSpacing);
  if (TickUtil.tickToPrice(tickUpper, decimalsA, decimalsB).lt(upper)) tickUpper += tickSpacing;
  const priceLower = TickUtil.tickToPrice(tickLower, decimalsA, decimalsB);
  const priceUpper = TickUtil.tickToPrice(tickUpper, decimalsA, decimalsB);
  assert(priceLower.lte(lower) && priceUpper.gte(upper), 'Диапазон выходит за допустимые тики Raydium');
  const sqrtCurrent = TickUtil.priceToSqrtPriceX64(current, decimalsA, decimalsB);
  const sqrtLower = TickUtil.getSqrtPriceAtTick(tickLower);
  const sqrtUpper = TickUtil.getSqrtPriceAtTick(tickUpper);
  const liquidity = LiquidityMathUtil.getLiquidityFromAmountA(sqrtCurrent, sqrtUpper, amountA);
  const { amountB } = LiquidityMathUtil.getAmountsForLiquidity(sqrtCurrent, sqrtLower, sqrtUpper, liquidity, true);
  assert(liquidity.gt(new BN(0)) && amountB.gt(new BN(0)), 'Слишком малая ликвидность');
  // Верхний лимит token B с запасом 1%; округление вверх в минимальных единицах.
  const amountBMax = amountB.muln(10100).addn(9999).divn(10000);
  return { tickLower, tickUpper, priceLower: priceLower.toString(), priceUpper: priceUpper.toString(),
    amountA, amountB, amountBMax, liquidity };
}
