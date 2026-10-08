// Formatação para a interface. Funções puras, sem Tauri (testadas em tests/format.test.js).

export const esc = (s) =>
  String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);

export const STORES = {
  ml: { name: 'Mercado Livre', short: 'ML' },
  amazon: { name: 'Amazon', short: 'AMZ' },
  shopee: { name: 'Shopee', short: 'SHP' },
};

/** Centavos → "R$ 1.234,50". Sem preço → "—". */
export function brl(cents) {
  if (cents == null) return '—';
  return (cents / 100).toLocaleString('pt-BR', { style: 'currency', currency: 'BRL' });
}

/** Variação percentual com uma casa: pct(500, 450) = -10. */
export function pct(from, to) {
  if (!from || to == null) return null;
  return Math.round(((to - from) / from) * 1000) / 10;
}

/** Desconto anunciado em % inteiro (0 quando não há preço "de" maior). */
export function discount(price, listPrice) {
  if (!listPrice || listPrice <= price) return 0;
  return Math.round((1 - price / listPrice) * 100);
}

const dm = (d) => `${String(d.getDate()).padStart(2, '0')}/${String(d.getMonth() + 1).padStart(2, '0')}`;

/** "agora", "há 5 min", "há 3 h", "ontem" ou "05/10". `iso` é AAAA-MM-DDTHH:MM:SS no horário local. */
export function ago(iso, now = new Date()) {
  if (!iso) return 'nunca';
  const t = new Date(iso);
  const min = Math.floor((now - t) / 60000);
  if (min < 1) return 'agora';
  if (min < 60) return `há ${min} min`;
  if (t.toDateString() === now.toDateString()) return `há ${Math.floor(min / 60)} h`;
  const yesterday = new Date(now);
  yesterday.setDate(yesterday.getDate() - 1);
  if (t.toDateString() === yesterday.toDateString()) return 'ontem';
  return dm(t);
}

/** dd/mm de um timestamp em ms. */
export const dayMonth = (ms) => dm(new Date(ms));
