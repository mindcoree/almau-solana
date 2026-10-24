import assert from 'node:assert/strict';
import test from 'node:test';
import BN from 'bn.js';
import Decimal from 'decimal.js';
import { quotePosition } from './clmm-math.mjs';

const input = { price: '1', decimalsA: 6, decimalsB: 6, tickSpacing: 10, amountA: new BN('1000000000') };

test('Диапазон вокруг 1:1 округляется наружу по шагу тиков', () => {
  const result = quotePosition(input);
  assert.equal(result.tickLower, -1060);
  assert.equal(result.tickUpper, 960);
  assert(new Decimal(result.priceLower).lte('0.9'));
  assert(new Decimal(result.priceUpper).gte('1.1'));
});

test('Учитываются разные decimals, положительные и отрицательные тики', () => {
  for (const price of ['0.01', '1', '100']) {
    const result = quotePosition({ ...input, price, decimalsA: 9, tickSpacing: 60 });
    assert.equal(Math.abs(result.tickLower % 60), 0);
    assert.equal(Math.abs(result.tickUpper % 60), 0);
    assert(new Decimal(result.priceLower).lte(new Decimal(price).mul('0.9')));
    assert(new Decimal(result.priceUpper).gte(new Decimal(price).mul('1.1')));
  }
});

test('Положительная ликвидность и лимит token B с округлением вверх на 1%', () => {
  const result = quotePosition(input);
  assert(result.liquidity.gt(new BN(0)));
  assert.equal(result.liquidity.toString(), '21338374629');
  assert.equal(result.amountB.toString(), '1101433008');
  assert(result.amountBMax.muln(10000).gte(result.amountB.muln(10100)));
  assert(result.amountBMax.subn(1).muln(10000).lt(result.amountB.muln(10100)));
});

test('Расчёт крупных сумм сохраняет целые числа за пределами Number.MAX_SAFE_INTEGER', () => {
  const result = quotePosition({ ...input, amountA: new BN('1000000000000000000') });
  assert(result.amountB.gt(new BN(Number.MAX_SAFE_INTEGER.toString())));
  assert(result.amountBMax.gt(result.amountB));
});

test('Нулевая, отрицательная, бесконечная цена и неверный шаг отклоняются', () => {
  for (const price of ['0', '-1', 'Infinity', 'NaN']) assert.throws(() => quotePosition({ ...input, price }));
  for (const tickSpacing of [0, -10, 1.5]) assert.throws(() => quotePosition({ ...input, tickSpacing }));
  assert.throws(() => quotePosition({ ...input, amountA: new BN(0) }));
});
