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

/** Rótulo curto de um alerta (`kind` vem do Rust em snake_case). */
export function alertText(a) {
  switch (a.kind) {
    case 'target': return 'Chegou ao preço-alvo';
    case 'lowest': return 'Menor preço já visto';
    case 'drop': return `Caiu ${Math.round(((a.priceBefore - a.priceAfter) / a.priceBefore) * 100)}%`;
    case 'back_in_stock': return 'Voltou a ter oferta';
    default: return a.kind;
  }
}

/**
 * "R$ 1.234,56" → 123456 centavos. Vazio → null. Inválido, zero ou negativo → undefined.
 * Ponto é separador de milhar e vírgula é decimal (formato brasileiro).
 */
export function parseCents(text) {
  const s = String(text ?? '').replace(/R\$|\s/g, '');
  if (!s) return null;
  if (!/^\d{1,3}(\.\d{3})*(,\d{1,2})?$|^\d+(,\d{1,2})?$/.test(s)) return undefined;
  const [int, dec = ''] = s.replace(/\./g, '').split(',');
  const cents = Number(int) * 100 + Number(dec.padEnd(2, '0'));
  return cents > 0 ? cents : undefined;
}

/** Vendas no formato do Mercado Livre: "+10 mil vendas", "+1000 vendas", "+500 vendas", "2 vendas". */
export function salesLabel(n) {
  if (n == null) return '';
  if (n >= 10000) return `+${Math.floor(n / 1000)} mil vendas`;
  if (n >= 1000) return `+${Math.floor(n / 1000) * 1000} vendas`;
  if (n >= 100) return `+${Math.floor(n / 100) * 100} vendas`;
  return `${n} ${n === 1 ? 'venda' : 'vendas'}`;
}

/** "platinum" → "MercadoLíder Platinum". */
export function powerSellerLabel(status) {
  return { platinum: 'MercadoLíder Platinum', gold: 'MercadoLíder Gold', silver: 'MercadoLíder' }[status] || '';
}
