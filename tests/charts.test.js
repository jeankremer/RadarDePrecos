import { test } from 'node:test';
import assert from 'node:assert/strict';
import { stepPath, sparkline } from '../src/charts.js';

test('stepPath desenha degraus: o preço vale até a próxima mudança', () => {
  assert.equal(stepPath([{ x: 0, y: 10 }, { x: 50, y: 20 }, { x: 100, y: 5 }]), 'M0.0,10.0H50.0V20.0H100.0V5.0');
});

test('stepPath interrompe a linha quando fica sem estoque', () => {
  assert.equal(stepPath([{ x: 0, y: 10 }, { x: 50, y: null }, { x: 80, y: 20 }]), 'M0.0,10.0H50.0M80.0,20.0');
  assert.equal(stepPath([{ x: 0, y: null }, { x: 10, y: 5 }]), 'M10.0,5.0');
});

test('sparkline precisa de pelo menos dois preços', () => {
  assert.equal(sparkline([null, 500]), '');
  assert.match(sparkline([500, null, 450]), /^<svg class="spark down"/);
});
