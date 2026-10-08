import '@tabler/icons-webfont/dist/tabler-icons.min.css';
import './style.css';
import { esc } from './core.js';
import { searchModule } from './search.js';
import { productsModule } from './products.js';
import { settingsModule } from './settings.js';

const LOGO = `<svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
  <circle cx="12" cy="12" r="9"/><circle cx="12" cy="12" r="5"/><path d="M12 12 18.5 5.5"/><circle cx="12" cy="12" r="1" fill="currentColor"/>
</svg>`;

const VIEWS = {
  search: { label: 'Buscar', icon: 'search', hint: 'Buscar e comparar preços', module: searchModule },
  products: { label: 'Acompanhando', icon: 'chart-line', hint: 'Produtos acompanhados e histórico', module: productsModule },
  settings: { label: 'Ajustes', icon: 'settings', module: settingsModule, bottom: true },
};
// Telas principais, na ordem dos atalhos Ctrl+1..
const VIEW_KEYS = Object.keys(VIEWS).filter((k) => !VIEWS[k].bottom);

const app = document.getElementById('app');
const $ = (sel) => app.querySelector(sel);

let view = loadView();
let cleanup = null;

function loadView() {
  try {
    const v = localStorage.getItem('view');
    return VIEWS[v] ? v : 'search';
  } catch {
    return 'search';
  }
}

function renderShell() {
  app.innerHTML = `
    <div class="shell">
      <nav class="rail">
        <div class="brand"><div class="logo">${LOGO}</div><span>RADAR</span></div>
        ${VIEW_KEYS.map((k, i) => `
          <button class="rail-btn" data-v="${k}" title="${VIEWS[k].hint} (Ctrl+${i + 1})">
            <i class="ti ti-${VIEWS[k].icon}"></i><span>${VIEWS[k].label}</span>
          </button>`).join('')}
        <div class="spacer"></div>
        <button class="rail-btn" data-v="settings" title="Mercado Livre e backup">
          <i class="ti ti-settings"></i><span>Ajustes</span>
        </button>
      </nav>
      <div class="module" id="module"></div>
    </div>`;
  app.querySelectorAll('.rail-btn[data-v]').forEach((b) => (b.onclick = () => navigate(b.dataset.v)));
  showView();
}

/** `arg` é repassado ao render da tela (ex.: { open: idDoProduto }). */
function navigate(v, arg) {
  view = v;
  try { localStorage.setItem('view', v); } catch { /* sem armazenamento: só nesta sessão */ }
  showView(arg);
}

function showView(arg) {
  cleanup?.();
  app.querySelectorAll('.rail-btn[data-v]').forEach((b) => b.classList.toggle('on', b.dataset.v === view));
  cleanup = VIEWS[view].module.render($('#module'), { navigate }, arg) || null;
}

document.addEventListener('keydown', (ev) => {
  if (document.querySelector('.overlay') || !ev.ctrlKey) return;
  const k = ev.key.toLowerCase();
  const mod = VIEWS[view].module;
  if (k === 'f' && mod.onFind) { ev.preventDefault(); mod.onFind(); }
  else if (/^[1-9]$/.test(k) && VIEW_KEYS[+k - 1]) { ev.preventDefault(); navigate(VIEW_KEYS[+k - 1]); }
});

// Sem menu de contexto do navegador fora dos campos de texto.
document.addEventListener('contextmenu', (ev) => {
  if (!ev.target.closest('input, textarea')) ev.preventDefault();
});

(async () => {
  try {
    if (import.meta.env.DEV && !window.__TAURI_INTERNALS__) await import('./dev-mock.js');
    renderShell();
  } catch (e) {
    app.innerHTML = `<div class="center"><p class="error">Erro ao iniciar: ${esc(e)}</p></div>`;
  }
})();
