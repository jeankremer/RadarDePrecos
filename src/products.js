// Tela "Acompanhando": lista de produtos, adicionar por link, checar agora e detalhe com histórico.
import { openUrl } from '@tauri-apps/plugin-opener';
import { call, esc, toast, setBusy } from './core.js';
import { brl, pct, ago, STORES, parseCents, discount, salesLabel, powerSellerLabel } from './format.js';
import { sparkline, stepChart } from './charts.js';
import { chooseProduct } from './track.js';

const SERIES_COLORS = ['#a78bfa', '#60a5fa', '#34d399', '#fbbf24', '#f472b6'];

let root;
let openId = null;
const $ = (s) => root.querySelector(s);

export const productsModule = {
  render(el, context, arg) {
    root = el;
    if (arg?.open) openId = arg.open;
    openId ? showDetail(openId) : showList();
  },
  onFind() { $('#addurl')?.focus(); },
  /** Chamado quando uma checagem termina. Não redesenha enquanto o usuário digita. */
  refresh() {
    if (root?.contains(document.activeElement) && document.activeElement.matches('input, select')) return;
    openId ? showDetail(openId) : showList();
  },
};

const INFLATED = (p) => `<span class="tag warn" title="O preço &quot;de&quot; está acima do maior preço visto nos últimos 30 dias (${brl(p.max30d)}). O desconto anunciado parece inflado.">Desconto inflado</span>`;

const fmtPct = (v) => `${v > 0 ? '+' : ''}${String(v).replace('.', ',')}%`;

const lastCheck = (products) => {
  const t = products.map((p) => p.lastCheck).filter(Boolean).sort().pop();
  return t ? ` · última checagem ${ago(t)}` : '';
};

// ---------- lista ----------

async function showList() {
  openId = null;
  let products;
  try {
    products = await call('list_products');
  } catch {
    return;
  }
  if (!root.isConnected || openId) return;
  root.innerHTML = `
    <div class="page">
      <div class="pg-head">
        <div>
          <h2 class="pg-title">Acompanhando</h2>
          <p class="muted">${products.length} ${products.length === 1 ? 'produto' : 'produtos'}${lastCheck(products)}</p>
        </div>
        <button class="primary" id="check"><i class="ti ti-refresh"></i> Checar agora</button>
      </div>
      <form class="add-url" id="addf" autocomplete="off">
        <input id="addurl" placeholder="Cole o link de um produto do Mercado Livre (com /p/MLB… no endereço)">
        <button id="addgo"><i class="ti ti-plus"></i> Acompanhar link</button>
      </form>
      ${products.length ? `<div class="plist">${products.map(row).join('')}</div>` : `
        <div class="empty"><i class="ti ti-radar"></i><p>Nenhum produto ainda.</p>
          <p class="muted">Busque em <b>Buscar</b> e clique em Acompanhar, ou cole um link acima.</p></div>`}
    </div>`;
  $('#check').onclick = (e) => checkNow(e.currentTarget);
  $('#addf').onsubmit = (e) => { e.preventDefault(); addByUrl(); };
  root.querySelectorAll('.prow').forEach((r) => {
    r.onclick = () => showDetail(Number(r.dataset.id));
    r.onkeydown = (e) => { if (e.key === 'Enter') showDetail(Number(r.dataset.id)); };
  });
}

function row(p) {
  const change = pct(p.firstPrice, p.bestPrice);
  const problems = p.links.filter((l) => l.failures >= 3).length;
  return `
    <div class="prow" data-id="${p.id}" role="button" tabindex="0">
      <img src="${esc(p.links[0]?.image || '')}" alt="">
      <div class="prow-main">
        <b title="${esc(p.name)}">${esc(p.name)}</b>
        <span class="muted">${p.links.map((l) => STORES[l.store].short).join(' · ')}
          ${problems ? ` · <span class="warn-text">${problems} ${problems === 1 ? 'link com problema' : 'links com problema'}</span>` : ''}</span>
      </div>
      <div class="prow-spark">${sparkline(p.spark)}</div>
      <div class="prow-price">
        <b>${brl(p.bestPrice)}</b>
        <span class="muted">${p.bestStore ? STORES[p.bestStore].name : 'sem estoque'}</span>
        ${p.inflated ? INFLATED(p) : ''}
      </div>
      <div class="prow-meta">
        ${change ? `<span class="chg ${change < 0 ? 'down' : 'up'}">${fmtPct(change)}</span>` : '<span class="muted">—</span>'}
        <span class="muted">menor: ${brl(p.lowestEver)}</span>
      </div>
    </div>`;
}

async function checkNow(btn) {
  setBusy(btn, true, 'Checando…');
  try {
    const s = await call('check_now');
    toast(`${s.checked} checados · ${s.changed} com mudança${s.failed ? ` · ${s.failed} com erro` : ''}`, s.failed ? 'err' : 'ok');
  } catch { /* call já mostrou o erro */ }
  if (root.isConnected) (openId ? showDetail(openId) : showList());
}

async function addByUrl() {
  const url = $('#addurl').value.trim();
  if (!url) return;
  const target = await chooseProduct('');
  if (!target) return;
  const btn = $('#addgo');
  setBusy(btn, true, 'Buscando produto…');
  try {
    const id = await call('track_url', { url, target });
    toast('Produto acompanhado');
    showDetail(id);
  } catch {
    if (root.isConnected) setBusy(btn, false, 'Acompanhar link');
  }
}

// ---------- detalhe ----------

async function showDetail(id) {
  openId = id;
  let d;
  try {
    d = await call('product_detail', { id });
  } catch {
    return showList();
  }
  if (!root.isConnected || openId !== id) return;
  const p = d.product;
  const titleOf = (linkId) => p.links.find((l) => l.id === linkId)?.title ?? '';
  const series = d.history.map((h, i) => ({
    label: `${STORES[h.store].short} · ${titleOf(h.linkId)}`,
    color: SERIES_COLORS[i % SERIES_COLORS.length],
    points: h.points.map((pt) => ({ t: new Date(pt.at).getTime(), v: pt.inStock ? pt.price : null })),
  }));
  const change = pct(p.firstPrice, p.bestPrice);
  root.innerHTML = `
    <div class="page">
      <button class="ghost small" id="back"><i class="ti ti-arrow-left"></i> Acompanhando</button>
      <div class="pg-head">
        <div class="detail-title">
          <input id="pname" class="title-input" value="${esc(p.name)}" maxlength="120" title="Edite e aperte Enter para renomear">
          <p class="muted">Menor agora: <b>${brl(p.bestPrice)}</b>${p.bestStore ? ` no ${STORES[p.bestStore].name}` : ''}
            · menor já visto: <b>${brl(p.lowestEver)}</b>
            ${change ? ` · <span class="chg ${change < 0 ? 'down' : 'up'}">${fmtPct(change)} desde que começou</span>` : ''}
            ${p.inflated ? ` ${INFLATED(p)}` : ''}</p>
        </div>
        <button class="primary" id="check"><i class="ti ti-refresh"></i> Checar agora</button>
      </div>
      <section class="card">${stepChart(series)}</section>
      <section class="card" id="offers"><h3><i class="ti ti-list-details"></i> Ofertas agora</h3><p class="muted">Consultando as ofertas no Mercado Livre…</p></section>
      <section class="card">
        <h3>Links</h3>
        <table class="links">
          <thead><tr><th>Loja</th><th>Produto</th><th>Menor preço</th><th>Checado</th><th></th></tr></thead>
          <tbody>${p.links.map(linkRow).join('')}</tbody>
        </table>
      </section>
      <section class="card">
        <h3><i class="ti ti-bell"></i> Avisos</h3>
        <form id="rulesf" class="rules-form" autocomplete="off">
          <label>Preço-alvo <span class="pre">R$</span><input id="rtarget" value="${p.targetPrice != null ? (p.targetPrice / 100).toFixed(2).replace('.', ',') : ''}" placeholder="opcional"></label>
          <label>Avisar quando cair pelo menos <input id="rdrop" type="number" min="0" max="90" step="1" value="${p.minDropPct}"><span class="pos">%</span></label>
          <label class="check"><input id="rlowest" type="checkbox" ${p.notifyLowest ? 'checked' : ''}> Avisar no menor preço já visto</label>
          <button class="small" id="rsave">Salvar</button>
        </form>
        <p class="error" id="rerr"></p>
        <p class="muted">Os avisos aparecem em Alertas e como notificação do Windows. A primeira checagem nunca avisa,
          e uma queda de mais de 60% só vira aviso depois de confirmada numa segunda leitura.</p>
      </section>
      <div class="danger-zone"><button class="ghost danger" id="del"><i class="ti ti-trash"></i> Parar de acompanhar</button></div>
    </div>`;

  $('#back').onclick = () => showList();
  loadOffers(id);
  $('#check').onclick = (e) => checkNow(e.currentTarget);
  const name = $('#pname');
  name.onkeydown = (e) => { if (e.key === 'Enter') { e.preventDefault(); name.blur(); } };
  name.onchange = async () => {
    try {
      await call('rename_product', { id, name: name.value });
      toast('Nome salvo');
    } catch {
      name.value = p.name;
    }
  };
  root.querySelectorAll('[data-open]').forEach((b) => (b.onclick = () => openUrl(b.dataset.open)));
  root.querySelectorAll('[data-unlink]').forEach((b) => (b.onclick = async () => {
    const last = p.links.length === 1;
    if (!confirm(last ? 'Este é o único link. Remover também tira o produto e o histórico. Continuar?' : 'Remover este link e o histórico dele?')) return;
    try {
      await call('delete_link', { id: Number(b.dataset.unlink) });
      last ? showList() : showDetail(id);
    } catch { /* call já mostrou o erro */ }
  }));
  $('#rulesf').oninput = () => { $('#rerr').textContent = ''; };
  $('#rulesf').onsubmit = async (e) => {
    e.preventDefault();
    const target = parseCents($('#rtarget').value);
    const minDropPct = Number($('#rdrop').value);
    if (target === undefined) return ($('#rerr').textContent = 'Preço-alvo inválido. Use 89,90');
    if (!Number.isFinite(minDropPct) || minDropPct < 0 || minDropPct > 90) return ($('#rerr').textContent = 'A queda mínima vai de 0% a 90%');
    try {
      await call('update_rules', { id, target, minDropPct, notifyLowest: $('#rlowest').checked });
      toast('Avisos salvos');
      $('#rsave').blur();
    } catch { /* call já mostrou o erro */ }
  };
  $('#del').onclick = async () => {
    if (!confirm(`Parar de acompanhar "${p.name}"? O histórico de preços será apagado.`)) return;
    try {
      await call('delete_product', { id });
      toast('Produto removido');
      showList();
    } catch { /* call já mostrou o erro */ }
  };
}

function linkRow(l) {
  const c = l.current;
  const price = !c ? '—'
    : !c.inStock ? '<span class="muted">sem oferta</span>'
    : `${brl(c.price)}${c.listPrice ? ` <s class="muted">${brl(c.listPrice)}</s>` : ''}`;
  return `
    <tr>
      <td><span class="store-tag store-${l.store}">${STORES[l.store].short}</span></td>
      <td class="ltitle" title="${esc(l.title)}">${esc(l.title)}</td>
      <td>${price}</td>
      <td>${ago(l.lastCheck)}${l.lastError ? `<div class="error small" title="${esc(l.lastError)}">${l.failures >= 3 ? 'Link com problema: ' : ''}${esc(l.lastError)}</div>` : ''}</td>
      <td class="row-actions">
        <button class="icon ghost small" data-open="${esc(l.url)}" title="Abrir na loja"><i class="ti ti-external-link"></i></button>
        <button class="icon ghost small danger" data-unlink="${l.id}" title="Remover este link"><i class="ti ti-unlink"></i></button>
      </td>
    </tr>`;
}

// ---------- ofertas agora ----------

let freeOnly = false;
let offersData = [];

/** Busca as ofertas de cada link na hora (não ficam no banco) e desenha a tabela. */
async function loadOffers(id) {
  try {
    offersData = await call('current_offers', { id });
  } catch {
    offersData = [];
  }
  if (openId === id && root.isConnected) paintOffers();
}

const LEVELS = { '5_green': 'Reputação ótima', '4_light_green': 'Reputação boa', '3_yellow': 'Reputação regular', '2_orange': 'Reputação ruim', '1_red': 'Reputação muito ruim' };

function sellerCell(o) {
  const s = o.seller;
  const name = s ? esc(s.nickname) : `Vendedor nº ${o.sellerId}`;
  const lines = [
    o.officialStore ? '<span class="tag">Loja oficial</span>' : '',
    s?.powerSeller ? `<span class="ok-text">${powerSellerLabel(s.powerSeller)}</span>` : '',
    s?.sales != null ? `<span class="muted">${salesLabel(s.sales)}</span>` : '',
    o.sellerPlace ? `<span class="muted">${esc(o.sellerPlace)}</span>` : '',
  ].filter(Boolean);
  const level = s?.level ? `<i class="rep rep-${esc(s.level)}" title="${LEVELS[s.level] || ''}"></i>` : '';
  return `<b>${level}${name}</b>${lines.length ? `<div class="sub">${lines.join(' · ')}</div>` : ''}`;
}

function offerRow(o) {
  const d = discount(o.price, o.listPrice);
  const delivery = [
    o.freeShipping ? '<span class="ok-text">Frete grátis</span>' : '<span class="muted">Frete pago</span>',
    o.full ? '<span class="tag">Full</span>' : '',
    o.flex ? '<span class="tag" title="Entrega no mesmo dia em algumas cidades">Flex</span>' : '',
  ].filter(Boolean).join(' ');
  return `
    <tr>
      <td class="oprice"><b>${brl(o.price)}</b>${d ? `<div class="sub"><s>${brl(o.listPrice)}</s> <span class="off">-${d}%</span></div>` : ''}</td>
      <td>${o.condition === 'new' ? 'Novo' : o.condition === 'used' ? 'Usado' : 'Recondicionado'}</td>
      <td>${delivery}</td>
      <td>${sellerCell(o)}</td>
      <td class="row-actions"><button class="icon ghost small" data-open="${esc(o.url)}" title="Abrir esta oferta no Mercado Livre"><i class="ti ti-external-link"></i></button></td>
    </tr>`;
}

function paintOffers() {
  const card = $('#offers');
  if (!card) return;
  const many = offersData.length > 1;
  const total = offersData.reduce((n, l) => n + l.offers.length, 0);
  const groups = offersData.map((l) => {
    const rows = l.offers.filter((o) => !freeOnly || o.freeShipping);
    return `
      ${many ? `<h4 class="ogroup">${esc(l.title)}</h4>` : ''}
      ${l.error ? `<p class="error">${esc(l.error)}</p>` : ''}
      ${rows.length ? `
        <table class="links offers-table">
          <thead><tr><th>Preço</th><th>Condição</th><th>Entrega</th><th>Vendedor</th><th></th></tr></thead>
          <tbody>${rows.map(offerRow).join('')}</tbody>
        </table>` : `<p class="muted">${l.offers.length ? 'Nenhuma oferta com frete grátis.' : 'Ninguém está vendendo este produto agora.'}</p>`}
      ${l.sellersNote ? `<p class="muted small">${esc(l.sellersNote)}</p>` : ''}`;
  }).join('');
  card.innerHTML = `
    <div class="ohead">
      <h3><i class="ti ti-list-details"></i> Ofertas agora <span class="muted">(${total})</span></h3>
      <label class="check"><input type="checkbox" id="ofree" ${freeOnly ? 'checked' : ''}> Só frete grátis</label>
    </div>
    ${groups || '<p class="muted">Nenhum link do Mercado Livre neste produto.</p>'}
    <p class="muted small">Parcelas e datas de entrega não vêm pela API do Mercado Livre; abra a oferta para ver.</p>`;
  $('#ofree').onchange = (e) => { freeOnly = e.target.checked; paintOffers(); };
  card.querySelectorAll('[data-open]').forEach((b) => (b.onclick = () => openUrl(b.dataset.open)));
}
