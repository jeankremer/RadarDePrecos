import { test } from 'node:test';
import assert from 'node:assert/strict';
import { brl, pct, discount, ago, esc, alertText, parseCents } from '../src/format.js';

test('brl formata centavos em reais', () => {
  assert.equal(brl(123450).replace(/\s/g, ' '), 'R$ 1.234,50');
  assert.equal(brl(990).replace(/\s/g, ' '), 'R$ 9,90');
  assert.equal(brl(null), '—');
});

test('pct: variação com uma casa decimal', () => {
  assert.equal(pct(500, 450), -10);
  assert.equal(pct(300, 310), 3.3);
  assert.equal(pct(0, 10), null);
  assert.equal(pct(500, null), null);
});

test('discount: só quando o preço "de" é maior', () => {
  assert.equal(discount(38990, 49990), 22);
  assert.equal(discount(500, null), 0);
  assert.equal(discount(500, 400), 0);
});

test('ago: tempo relativo em português', () => {
  const now = new Date('2026-10-07T15:00:00');
  assert.equal(ago('2026-10-07T14:59:30', now), 'agora');
  assert.equal(ago('2026-10-07T14:55:00', now), 'há 5 min');
  assert.equal(ago('2026-10-07T12:00:00', now), 'há 3 h');
  assert.equal(ago('2026-10-06T20:00:00', now), 'ontem');
  assert.equal(ago('2026-10-05T10:00:00', now), '05/10');
  assert.equal(ago(null, now), 'nunca');
});

test('esc escapa HTML', () => {
  assert.equal(esc('<b>"a" & \'b\'</b>'), '&lt;b&gt;&quot;a&quot; &amp; &#39;b&#39;&lt;/b&gt;');
});

test('alertText descreve cada tipo de alerta', () => {
  assert.equal(alertText({ kind: 'target', priceBefore: 10000, priceAfter: 8990 }), 'Chegou ao preço-alvo');
  assert.equal(alertText({ kind: 'lowest', priceBefore: 10000, priceAfter: 8990 }), 'Menor preço já visto');
  assert.equal(alertText({ kind: 'drop', priceBefore: 10000, priceAfter: 9000 }), 'Caiu 10%');
  assert.equal(alertText({ kind: 'back_in_stock', priceBefore: null, priceAfter: 9000 }), 'Voltou a ter oferta');
});

test('parseCents lê valores em reais', () => {
  const cases = [['89,90', 8990], ['89,9', 8990], ['89', 8900], ['R$ 1.234,56', 123456], ['1.500', 150000], [' 0,99 ', 99]];
  for (const [input, expected] of cases) assert.equal(parseCents(input), expected, input);
  assert.equal(parseCents(''), null);
  assert.equal(parseCents('abc'), undefined);
  assert.equal(parseCents('-5'), undefined);
  assert.equal(parseCents('0'), undefined);
});
