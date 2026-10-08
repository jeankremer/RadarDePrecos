// Só em desenvolvimento no navegador (npm run dev, fora do Tauri): simula o backend em memória com dados
// de exemplo, para mexer na interface sem tocar no banco real nem no Mercado Livre.
import { mockIPC } from '@tauri-apps/api/mocks';
import { emit } from '@tauri-apps/api/event';

const pad = (n) => String(n).padStart(2, '0');
const at = (daysAgo, h = 9) => {
  const d = new Date();
  d.setDate(d.getDate() - daysAgo);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(h)}:00:00`;
};
const R = (price, extra = {}) => ({ price, listPrice: null, inStock: true, freeShipping: true, ...extra });
const url = (id) => `https://www.mercadolivre.com.br/p/${id}`;

const offers = [
  { store: 'ml', code: 'MLB39766120', title: 'SSD Kingston NV3 1TB M.2 2280 PCIe 4.0 NVMe 6000 MB/s', url: url('MLB39766120'), image: '', price: 99700, listPrice: 144900, freeShipping: true, full: false, sellers: 198 },
  { store: 'ml', code: 'MLB38550244', title: 'SSD Crucial BX500 1TB SATA', url: url('MLB38550244'), image: '', price: 45900, listPrice: null, freeShipping: true, full: true, sellers: 37 },
  { store: 'ml', code: 'MLB22026951', title: 'SSD Kazuk 1TB SATA Preto', url: url('MLB22026951'), image: '', price: 31990, listPrice: 39990, freeShipping: false, full: false, sellers: 1 },
];

const history = {
  1: [[at(20), R(119900)], [at(12), R(109900)], [at(5), R(104900)], [at(1), R(99700, { listPrice: 144900 })]],
  2: [[at(18), R(112000)], [at(3), R(101900)], [at(0), R(101900, { inStock: false })]],
};
const link = (id, code, title) => ({ id, store: 'ml', code, title, url: url(code), image: '', lastCheck: at(0, new Date().getHours()), lastError: null, failures: 0 });
let products = [{ id: 1, name: 'SSD Kingston NV3 1TB', targetPrice: 95000, minDropPct: 5, notifyLowest: true, links: [link(1, 'MLB39766120', 'SSD Kingston NV3 1TB M.2 2280'), link(2, 'MLB50970810', 'Kingston NV3 1TB - Preto')] }];

const priceOn = (h, day) => {
  const r = h.filter(([a]) => a.slice(0, 10) <= day).at(-1)?.[1];
  return r?.inStock ? r.price : null;
};
function summary(p) {
  const hist = p.links.map((l) => history[l.id] || []);
  const cur = hist.map((h) => { const r = h.at(-1)?.[1]; return r?.inStock ? r.price : null; });
  const valid = cur.filter((v) => v != null);
  const best = valid.length ? Math.min(...valid) : null;
  const all = hist.flat().map(([, r]) => r).filter((r) => r.inStock).map((r) => r.price);
  const first = hist.map((h) => h[0]?.[0]).filter(Boolean).sort()[0];
  const bestOn = (day) => { const v = hist.map((h) => priceOn(h, day)).filter((x) => x != null); return v.length ? Math.min(...v) : null; };
  return {
    id: p.id, name: p.name, targetPrice: p.targetPrice ?? null, minDropPct: p.minDropPct ?? 5, notifyLowest: p.notifyLowest ?? true,
    max30d: 104900, inflated: p.id === 1,
    links: p.links.map((l, i) => ({ ...l, current: hist[i].at(-1)?.[1] ?? null })),
    bestPrice: best, bestStore: best == null ? null : p.links[cur.indexOf(best)].store,
    lowestEver: all.length ? Math.min(...all) : null,
    firstPrice: first ? bestOn(first.slice(0, 10)) : null,
    spark: Array.from({ length: 30 }, (_, i) => bestOn(at(29 - i).slice(0, 10))),
    lastCheck: p.links[0]?.lastCheck ?? null,
  };
}

let alerts = [
  { id: 2, productId: 1, productName: 'SSD Kingston NV3 1TB', kind: 'lowest', priceBefore: 104900, priceAfter: 99700, at: at(1, 14), read: false },
  { id: 1, productId: 1, productName: 'SSD Kingston NV3 1TB', kind: 'drop', priceBefore: 109900, priceAfter: 104900, at: at(5, 10), read: true },
];
let checks = { intervalHours: 3, notify: true, autostart: false, lastAutoCheck: at(0, 8), checking: false };
let ml = { clientId: '417769415941125', redirectUri: 'https://gocomercio.com.br/oauth/mercadolivre/callback', hasSecret: true, connected: true, nickname: 'JEANK' };
let backup = { dir: null, last: null, count: 0, error: null, suggestion: 'C:\\Users\\voce\\OneDrive\\Backups\\Radar de Precos' };
const later = (v, ms = 500) => new Promise((r) => setTimeout(() => r(v), ms));

mockIPC((cmd, args) => {
  switch (cmd) {
    case 'search': return later(offers);
    case 'list_products': return products.map(summary);
    case 'product_detail': {
      const p = products.find((x) => x.id === args.id);
      if (!p) throw 'Produto não encontrado';
      return { product: summary(p), history: p.links.map((l) => ({ linkId: l.id, store: l.store, points: (history[l.id] || []).map(([a, r]) => ({ at: a, ...r })) })) };
    }
    case 'track': case 'track_url': return 1;
    case 'rename_product': products.find((x) => x.id === args.id).name = args.name; return null;
    case 'delete_product': products = products.filter((x) => x.id !== args.id); return null;
    case 'delete_link':
      products.forEach((p) => (p.links = p.links.filter((l) => l.id !== args.id)));
      products = products.filter((p) => p.links.length);
      return null;
    case 'check_now': return later({ checked: 2, changed: 1, failed: 0, alerts: 0, suspicious: 0 }, 800);
    case 'update_rules': Object.assign(products.find((x) => x.id === args.id), { targetPrice: args.target, minDropPct: args.minDropPct, notifyLowest: args.notifyLowest }); return null;
    case 'list_alerts': return alerts;
    case 'unread_alerts': return alerts.filter((a) => !a.read).length;
    case 'mark_alerts_read': alerts = alerts.map((a) => ({ ...a, read: true })); emit('alerts', 0); return null;
    case 'check_settings': return checks;
    case 'set_check_settings': checks = { ...checks, intervalHours: args.intervalHours, notify: args.notify, autostart: args.autostart }; return checks;
    case 'test_notification': return null;
    case 'ml_status': case 'ml_save_settings': return ml;
    case 'ml_disconnect': ml = { ...ml, connected: false, nickname: null }; return ml;
    case 'ml_connect':
      setTimeout(() => { ml = { ...ml, connected: true, nickname: 'JEANK' }; emit('ml-login', { ok: true, nickname: 'JEANK' }); }, 1500);
      return null;
    case 'ml_dump_fixtures': return 'Simulado: nada foi salvo';
    case 'backup_status': return backup;
    case 'set_backup_dir': backup = { ...backup, dir: args.dir, last: args.dir ? at(0).slice(0, 10) : null, count: args.dir ? 1 : 0 }; return backup;
    case 'backup_now': return backup;
    case 'restore_backup': return null;
    case 'plugin:app|version': return '0.0.0-dev';
    case 'plugin:dialog|open': return args.options?.directory ? 'D:\\Backups\\Radar' : 'D:\\Backups\\Radar\\radar-2026-10-05.db';
    case 'plugin:opener|open_url': window.open(args.url, '_blank'); return null;
    default: console.log('[mock]', cmd, args); return null;
  }
}, { shouldMockEvents: true });
console.info('[dev-mock] backend simulado ativo: dados de exemplo, nada é gravado');
