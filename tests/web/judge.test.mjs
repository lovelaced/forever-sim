// A difference earns an arrow only when it's real and big enough to matter.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import './harness.mjs';
import { judge, smdOf } from '../../web/results.js';
import { METRICS } from '../../web/meta.js';

test('the smallest meaningful difference by kind of number', () => {
  assert.equal(smdOf(METRICS.dps, 300), 1.5);
  assert.equal(smdOf(METRICS.score, 0.5), 0.01);
  assert.equal(smdOf(METRICS.rest_share, 0.2), 0.005);
});

test('clear, small, same and unsure', () => {
  const dps = METRICS.dps;
  const base = 300;
  assert.equal(judge({ diff: 6, lo: 4, hi: 8, significant: true }, dps, base), 'clear');
  assert.equal(judge({ diff: 1, lo: 0.5, hi: 1.4, significant: true }, dps, base), 'small');
  assert.equal(judge({ diff: 0.2, lo: -1, hi: 1.2, significant: false }, dps, base), 'same');
  assert.equal(judge({ diff: 0.2, lo: -4, hi: 4, significant: false }, dps, base), 'unsure');
  // the long list asks 2.81 standard errors
  assert.equal(judge({ diff: 3, lo: 0.2, hi: 5.8, se: 1.4, significant: true }, dps, base, true), 'unsure');
  assert.equal(judge({ diff: 3, lo: 0.2, hi: 5.8, se: 1.4, significant: true }, dps, base), 'clear');
  assert.equal(judge(null, dps, base), 'unsure');
});
