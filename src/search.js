// Tela "Buscar": produtos do catálogo do Mercado Livre pelo menor preço, com "Acompanhar" em cada um.
import { openUrl } from '@tauri-apps/plugin-opener';
import { call, esc, toast, setBusy } from './core.js';
import { brl, discount, STORES } from './format.js';
import { chooseProduct } from './track.js';

let root, ctx;
let results = [];
let lastQuery = '';
const $ = (s) => root.querySelector(s);

export const searchModule = {
  render(el, context) {
    root = el;
    ctx = context;
    draw();
  },
  onFind() { $('#q')?.focus(); },
};

function draw() {
  root.innerHTML = `
    <div class="page">
      <h2 class="pg-title">Buscar</h2>
      <form id="sf" class="search-bar" autocomplete="off">
        <div class="search"><i class="ti ti-search"></i>
          <input id="q" placeholder="O que você procura? (ex.: SSD Kingston 1TB)" value="${esc(lastQuery)}" autofocus></div>
        <button class="primary" id="go">Buscar</button>
      </form>
      <p class="muted" id="status"></p>
      <div class="offers" id="results"></div>
    </div>`;
  $('#sf').onsubmit = (e) => { e.preventDefault(); run(); };
  paint();
}

async function run() {
  const q = $('#q').value.trim();
  if (!q) return;
  lastQuery = q;
  const btn = $('#go');
  setBusy(btn, true, 'Buscando…');
  $('#status').textContent = 'Buscando no catálogo do Mercado Livre e consultando as ofertas…';
  try {
    results = await call('search', { query: q });
  } catch {
    results = [];
  }
  if (!root.isConnected) return;
  setBusy(btn, false, 'Buscar');
  paint();
}

function paint() {
  const order = results.map((o, i) => i).sort((a, b) => results[a].price - results[b].price);
  $('#status').textContent = lastQuery
    ? `${results.length} produtos do catálogo com oferta para "${lastQuery}", do menor preço para o maior`
    : 'A busca usa o catálogo do Mercado Livre: cada cartão é um produto com o menor preço entre os vendedores.';
  $('#results').innerHTML = order.map((i) => card(results[i], i)).join('') || (lastQuery ? '<p class="muted">Nada encontrado com oferta ativa.</p>' : '');
  root.querySelectorAll('[data-track]').forEach((b) => (b.onclick = () => track(results[+b.dataset.track])));
  root.querySelectorAll('[data-open]').forEach((b) => (b.onclick = () => openUrl(results[+b.dataset.open].url)));
}

function card(o, i) {
  const d = discount(o.price, o.listPrice);
  return `
    <article class="offer">
      <img src="${esc(o.image || '')}" alt="" loading="lazy">
      <div class="offer-body">
        <span class="store-tag store-${o.store}">${STORES[o.store].short}</span>
        <h4 title="${esc(o.title)}">${esc(o.title)}</h4>
        <div class="price-line"><b>${brl(o.price)}</b>${d ? `<s>${brl(o.listPrice)}</s><span class="off">-${d}%</span>` : ''}</div>
        <div class="tags">
          ${o.freeShipping ? '<span class="tag ok">Frete grátis</span>' : ''}
          ${o.full ? '<span class="tag">Full</span>' : ''}
          <span class="muted">${o.sellers} ${o.sellers === 1 ? 'vendedor' : 'vendedores'}</span>
        </div>
      </div>
      <div class="offer-actions">
        <button class="ghost small" data-open="${i}" title="Abrir na loja"><i class="ti ti-external-link"></i></button>
        <button class="primary small" data-track="${i}"><i class="ti ti-eye-plus"></i> Acompanhar</button>
      </div>
    </article>`;
}

async function track(offer) {
  const target = await chooseProduct(offer.title);
  if (!target) return;
  try {
    const id = await call('track', { offer, target });
    toast('Produto acompanhado');
    ctx.navigate('products', { open: id });
  } catch { /* call já mostrou o erro */ }
}
