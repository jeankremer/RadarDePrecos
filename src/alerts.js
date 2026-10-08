// Tela "Alertas": o que mudou nas checagens. Abrir a tela marca tudo como lido.
import { call, esc } from './core.js';
import { brl, ago, alertText } from './format.js';

const ICONS = { target: 'target-arrow', lowest: 'trophy', drop: 'trending-down', back_in_stock: 'package' };

let root, ctx;

export const alertsModule = {
  render(el, context) {
    root = el;
    ctx = context;
    show();
  },
  refresh() { show(); },
};

async function show() {
  let list;
  try {
    list = await call('list_alerts');
  } catch {
    return;
  }
  if (!root.isConnected) return;
  root.innerHTML = `
    <div class="page">
      <h2 class="pg-title">Alertas</h2>
      <p class="muted">Quedas de preço, preço-alvo, menor preço já visto e produtos que voltaram a ter oferta.</p>
      ${list.length ? `<div class="alist">${list.map(row).join('')}</div>` : `
        <div class="empty"><i class="ti ti-bell"></i><p>Nenhum alerta ainda.</p>
          <p class="muted">Os avisos aparecem aqui e como notificação do Windows quando uma checagem encontrar algo.</p></div>`}
    </div>`;
  root.querySelectorAll('.arow').forEach((r) => (r.onclick = () => ctx.navigate('products', { open: Number(r.dataset.product) })));
  if (list.some((a) => !a.read)) call('mark_alerts_read').catch(() => {});
}

function row(a) {
  return `
    <div class="arow ${a.read ? '' : 'unread'} kind-${a.kind}" data-product="${a.productId}" role="button" tabindex="0">
      <div class="aicon"><i class="ti ti-${ICONS[a.kind] || 'bell'}"></i></div>
      <div class="amain">
        <b>${esc(a.productName)}</b>
        <span>${alertText(a)}</span>
      </div>
      <div class="aprice">
        ${a.priceBefore != null ? `<s class="muted">${brl(a.priceBefore)}</s>` : ''}
        <b>${brl(a.priceAfter)}</b>
      </div>
      <span class="muted atime">${ago(a.at)}</span>
    </div>`;
}
