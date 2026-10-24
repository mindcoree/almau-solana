import assert from 'node:assert/strict';
import test from 'node:test';
import BN from 'bn.js';
import { minimumOutput } from './clmm-math.mjs';

test('Минимум на 1% ниже SDK quote округляется вниз', () => {
  assert.equal(minimumOutput(new BN('9990320')).toString(), '9890416');
  assert.equal(minimumOutput(new BN('101'), 100).toString(), '99');
  assert.equal(minimumOutput(new BN('101'), 0).toString(), '101');
});

test('Защита от slippage сохраняет точность для u64', () => {
  const input = new BN('18446744073709551615');
  assert.equal(minimumOutput(input).toString(), (18446744073709551615n * 99n / 100n).toString());
});

test('Неверный процент, нулевой и полностью округлённый выход запрещены', () => {
  for (const bps of [-1, 10000, 1.5, NaN]) assert.throws(() => minimumOutput(new BN('10000'), bps));
  assert.throws(() => minimumOutput(new BN(0)));
  assert.throws(() => minimumOutput(new BN(-1)));
  assert.throws(() => minimumOutput(new BN(1)));
});
