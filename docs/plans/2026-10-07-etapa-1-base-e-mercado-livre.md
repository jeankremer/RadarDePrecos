# Etapa 1: base do app + Mercado Livre — plano de implementação

> **Para o Claude:** SUB-SKILL OBRIGATÓRIA: use `executing-plans` (ou `subagent-driven-development`) para implementar este plano tarefa por tarefa.

**Objetivo:** app Tauri que busca ofertas no Mercado Livre, deixa acompanhar anúncios agrupados em produtos, checa os preços
pelo botão "Checar agora" e mostra o histórico em gráfico. Tem backup diário como o do Cofre.

**Arquitetura:** a mesma base do Cofre de Credenciais (`D:\05 - PROJETOS PESSOAIS\Gerenciador de Credenciais`): interface em
HTML/CSS/JS puro com Vite e núcleo em Rust com Tauri 2. Os dados ficam num SQLite em `%APPDATA%\com.jeank.radar\radar.db`.
A chave secreta e o refresh token do ML ficam no Gerenciador de Credenciais do Windows (crate `keyring`). O login no ML é
OAuth com PKCE numa janela do app, que captura o redirect antes de ele carregar.

**Stack:** Tauri 2, Vite 6, `@tabler/icons-webfont`, Rust (`rusqlite` bundled, `reqwest` com rustls, `keyring`,
`sha2`, `rand`, `base64`, `chrono`), testes com `node --test` + `cargo test`.

**Fora desta etapa:** checagem automática em segundo plano, bandeja, notificações, alertas, promoção falsa, Amazon e
Shopee. Ficam para as etapas 2 a 4 do [desenho](2026-10-07-radar-de-precos-design.md).

---

## Fatos já validados (07/10/2026)

| Fato | Consequência |
|---|---|
| Sem token, a API do ML responde **403** em tudo (`/sites/MLB/search`, `/items/…`, `/sites/MLB`) | Não existe modo anônimo; o app só funciona conectado |
| `grant_type=client_credentials` → `unsupported_grant_type` | Precisa do login do usuário (authorization code + PKCE) |
| O DevCenter recusa `https://localhost/...` como redirect | Usar a URI já cadastrada: `https://gocomercio.com.br/oauth/mercadolivre/callback` (domínio do usuário). O app captura a navegação antes do carregamento |
| App no DevCenter: **Radar Ofertas JEV**, Client ID `417769415941125` | O Client ID é público e vai como padrão no código. A chave secreta **nunca** entra no código, no chat ou no git |
| **Ainda não confirmado:** se `/sites/MLB/search` responde com o token do usuário | **Tarefa 8 é um ponto de decisão.** Se a busca der 403 mesmo conectado, parar e rever a busca antes da Tarefa 9 |

## Mudança decidida na Tarefa 8 (07/10/2026)

O diagnóstico com o login real mostrou o que a API libera para o app "Radar Ofertas JEV" (não certificado):

| Endereço | Resultado |
|---|---|
| `/sites/MLB/search` (anúncios por texto ou categoria) | 403 |
| `/items?ids=`, `/items/{id}`, `/items/{id}/sale_price`, `/items/{id}/prices` | 403 |
| `/products/search?site_id=MLB&status=active&q=` (catálogo) | OK, mas sem preço |
| `/products/{id}` (nome, fotos) | OK |
| `/products/{id}/items` (todas as ofertas do produto, já do menor preço para o maior) | OK; 404 quando não há vendedor |
| `/highlights/MLB/category/{cat}` (mais vendidos) | OK |

**Decisão (opção C):** no Mercado Livre o app trabalha com **produtos do catálogo**, não com anúncios.
- **Buscar:** `/products/search` e depois `/products/{id}/items` de cada resultado, em paralelo. A tela mostra o menor preço entre as ofertas **novas** e quantos vendedores o produto tem. Produtos sem oferta ficam de fora.
- **Acompanhar e Checar agora:** o link guarda o id do produto (`MLB39766120`) e a URL `https://www.mercadolivre.com.br/p/{id}`. A leitura é a menor oferta nova; 404 conta como "sem estoque", não como falha.
- **Adicionar por link:** só links de catálogo (`…/p/MLB…`). Para link de anúncio, o app explica o motivo.
- Saem `parse_items`, `parse_sale_price`, `parse_buy_box_winner`, `ITEM_ATTRS` e `BATCH`. As fixtures escritas à mão dão lugar às respostas reais.
- **Depois:** anúncios fora do catálogo pelo navegador escondido, na etapa da Shopee.

As Tarefas 9 a 11 abaixo seguem valendo na estrutura. O que muda é o que está nesta seção.

## Convenções (iguais às do Cofre)

- Identificadores do código em inglês. Textos da interface, mensagens de erro, comentários e nomes de testes em português.
- Comandos Tauri retornam `Result<T, String>`, e a interface mostra a mensagem num toast.
- Dinheiro sempre em **centavos inteiros** (`i64` no Rust, `number` no JS).
- Datas como texto local `AAAA-MM-DDTHH:MM:SS`, que ordena como texto.
- Commits pequenos, mensagem em português no formato `Descrição (0.1.0)` só na versão final. Durante a etapa, mensagens curtas descritivas.
  Todo commit termina com `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Pasta do projeto: `D:\05 - PROJETOS PESSOAIS\Radar de Precos` (abaixo, caminhos relativos a ela). Cofre = pasta do Gerenciador de Credenciais.

---

### Tarefa 1: esqueleto do projeto

**Arquivos:**
- Criar: `package.json`, `vite.config.js`, `index.html`, `.gitignore`, `.gitattributes`, `.claude/launch.json`
- Criar: `src-tauri/Cargo.toml`, `src-tauri/build.rs`, `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json`
- Criar: `src-tauri/src/main.rs`, `src-tauri/src/lib.rs`
- Copiar do Cofre: `src-tauri/icons/` (provisório), `app-icon.png` (provisório), `src/style.css`
- Criar: `src/main.js`, `src/core.js`, `src/format.js` (só `esc` por enquanto), `src/search.js`, `src/products.js`, `src/settings.js` (stubs)

**Passo 1: arquivos de configuração**

`package.json`:
```json
{
  "name": "radar-de-precos",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "vite build",
    "tauri": "tauri",
    "test": "node --test \"tests/*.test.js\" && cargo test --manifest-path src-tauri/Cargo.toml"
  },
  "dependencies": {
    "@tabler/icons-webfont": "^3",
    "@tauri-apps/api": "^2",
    "@tauri-apps/plugin-dialog": "^2",
    "@tauri-apps/plugin-opener": "^2"
  },
  "devDependencies": {
    "@tauri-apps/cli": "^2",
    "vite": "^6"
  }
}
```

`vite.config.js`, `.gitattributes` e `.claude/launch.json`: **cópias idênticas** dos arquivos do Cofre.

`.gitignore`:
```
node_modules/
dist/
src-tauri/target/
src-tauri/gen/
```

`index.html`: igual ao do Cofre, trocando o `<title>` por `Radar de Preços`.

`src-tauri/Cargo.toml`:
```toml
[package]
name = "radar"
version = "0.1.0"
description = "Radar de Preços: busca e acompanhamento de preços"
edition = "2021"

[lib]
name = "radar_lib"
crate-type = ["staticlib", "cdylib", "rlib"]

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
tauri = { version = "2", features = [] }
tauri-plugin-opener = "2"
tauri-plugin-dialog = "2"
chrono = { version = "0.4", default-features = false, features = ["clock"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
rusqlite = { version = "0.32", features = ["bundled"] }
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
# Sem "windows-native" o keyring usa um armazenamento falso em memória.
keyring = { version = "3", features = ["windows-native"] }
rand = "0.8"
sha2 = "0.10"
base64 = "0.22"

[profile.release]
codegen-units = 1
lto = true
opt-level = "s"
strip = true
```

`src-tauri/build.rs` e `src-tauri/src/main.rs`: iguais aos do Cofre, trocando `cofre_lib::run()` por `radar_lib::run()`.

`src-tauri/tauri.conf.json`:
```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "Radar de Preços",
  "version": "0.1.0",
  "identifier": "com.jeank.radar",
  "build": {
    "beforeDevCommand": "npm run dev",
    "devUrl": "http://localhost:1420",
    "beforeBuildCommand": "npm run build",
    "frontendDist": "../dist"
  },
  "app": {
    "windows": [
      { "title": "Radar de Preços", "width": 1280, "height": 820, "minWidth": 820, "minHeight": 560, "center": true }
    ],
    "security": {
      "csp": "default-src 'self'; style-src 'self' 'unsafe-inline'; font-src 'self' data:; img-src 'self' data: https://*.mlstatic.com; connect-src ipc: http://ipc.localhost"
    }
  },
  "bundle": {
    "active": true,
    "targets": ["nsis"],
    "icon": ["icons/32x32.png", "icons/128x128.png", "icons/128x128@2x.png", "icons/icon.icns", "icons/icon.ico"],
    "shortDescription": "Busca e acompanhamento de preços",
    "longDescription": "Busca preços no Mercado Livre, Amazon e Shopee e acompanha o histórico dos produtos escolhidos.",
    "windows": { "nsis": { "languages": ["PortugueseBR"], "installMode": "currentUser" } }
  }
}
```

`src-tauri/capabilities/default.json`:
```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Permissões da janela principal",
  "windows": ["main"],
  "permissions": ["core:default", "opener:default", "dialog:allow-open"]
}
```
A janela de login do ML (`ml-login`) carrega um site externo e **não** recebe permissões.

**Passo 2: `src-tauri/src/lib.rs` mínimo**
```rust
//! Núcleo do Radar de Preços: banco local, Mercado Livre e comandos da interface.

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o aplicativo");
}
```

**Passo 3: interface base**

`src/format.js` (cresce na Tarefa 2):
```js
// Formatação para a interface. Funções puras, sem Tauri (testadas em tests/format.test.js).

export const esc = (s) =>
  String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);
```

`src/core.js`:
```js
// Utilitários de interface usados por todas as telas.
import { invoke } from '@tauri-apps/api/core';

export { esc } from './format.js';

export function toast(msg, kind = 'ok') {
  const t = document.getElementById('toast');
  t.textContent = msg;
  t.className = `toast show ${kind === 'err' ? 'err' : ''}`;
  clearTimeout(t._timer);
  t._timer = setTimeout(() => (t.className = 'toast'), 3200);
}

export function setBusy(btn, busy, label) {
  btn.disabled = busy;
  if (label) btn.textContent = label;
}

/** Chama um comando do Rust. Em caso de erro mostra o toast e repassa o erro. */
export async function call(cmd, args) {
  try {
    return await invoke(cmd, args);
  } catch (e) {
    toast(String(e), 'err');
    throw e;
  }
}

/** Abre um diálogo. Clicar fora ou apertar Esc fecha. Devolve { el, close }. */
export function modal(html, onClose) {
  const ov = document.createElement('div');
  ov.className = 'overlay';
  ov.innerHTML = `<div class="modal">${html}</div>`;
  document.body.appendChild(ov);
  const onKey = (e) => { if (e.key === 'Escape') close(); };
  const close = () => {
    ov.remove();
    document.removeEventListener('keydown', onKey);
    onClose?.();
  };
  document.addEventListener('keydown', onKey);
  ov.onclick = (e) => { if (e.target === ov) close(); };
  return { el: ov.querySelector('.modal'), close };
}
```

`src/main.js`:
```js
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
```

Stubs (substituídos nas próximas tarefas). `src/search.js`:
```js
export const searchModule = {
  render(el) { el.innerHTML = '<div class="page"><h2 class="pg-title">Buscar</h2></div>'; },
};
```
Faça o mesmo para `src/products.js` (`productsModule`, título "Acompanhando") e `src/settings.js` (`settingsModule`, título "Ajustes").

`src/dev-mock.js` provisório (completo na Tarefa 13):
```js
import { mockIPC } from '@tauri-apps/api/mocks';
mockIPC((cmd, args) => { console.log('[mock]', cmd, args); return null; });
```

`src/style.css`: copiar o do Cofre sem mudanças. As regras novas entram nas tarefas seguintes, sempre **no fim do arquivo**, depois de um
comentário `/* ---------- Radar de Preços ---------- */`.

**Passo 4: instalar e rodar**

Rode: `npm install`, depois `npm run tauri dev`.
Esperado: a janela "Radar de Preços" abre com a barra lateral (Buscar, Acompanhando, Ajustes) e o visual escuro do Cofre.
`Ctrl+1` e `Ctrl+2` trocam de tela. Feche a janela.

**Passo 5: commit**
```bash
git add -A
git commit -m "Esqueleto do app (Tauri 2 + Vite)"
```

---

### Tarefa 2: formatação e gráfico em degraus (JS, TDD)

**Arquivos:**
- Modificar: `src/format.js`
- Criar: `src/charts.js`, `tests/format.test.js`, `tests/charts.test.js`

**Passo 1: testes que falham**

`tests/format.test.js`:
```js
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { brl, pct, discount, ago, esc } from '../src/format.js';

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
```

`tests/charts.test.js`:
```js
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
```

**Passo 2: rodar e ver falhar**

Rode: `node --test "tests/*.test.js"`
Esperado: FAIL (`brl`, `stepPath`… não existem).

**Passo 3: implementação**

Acrescentar em `src/format.js`:
```js
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
```

`src/charts.js`:
```js
// Gráficos em SVG puro (o app funciona offline, sem bibliotecas externas). Sem dependência do Tauri.
import { brl, esc, dayMonth } from './format.js';

const n = (v) => v.toFixed(1);

/**
 * Caminho em degraus: cada preço vale até a próxima mudança.
 * `points` = [{ x, y }] em pixels; `y: null` (sem estoque) interrompe a linha.
 */
export function stepPath(points) {
  let d = '';
  let open = false;
  for (const p of points) {
    if (p.y == null) {
      if (open) d += `H${n(p.x)}`;
      open = false;
      continue;
    }
    d += open ? `H${n(p.x)}V${n(p.y)}` : `M${n(p.x)},${n(p.y)}`;
    open = true;
  }
  return d;
}

/** Mini gráfico de uma série diária (centavos ou null). Verde quando terminou mais barato que começou. */
export function sparkline(values, { width = 120, height = 32 } = {}) {
  const nums = values.filter((v) => v != null);
  if (nums.length < 2) return '';
  const lo = Math.min(...nums);
  const hi = Math.max(...nums);
  const step = width / (values.length - 1);
  const y = (v) => (hi === lo ? height / 2 : 3 + (1 - (v - lo) / (hi - lo)) * (height - 6));
  const d = stepPath(values.map((v, i) => ({ x: i * step, y: v == null ? null : y(v) })));
  const down = nums[nums.length - 1] < nums[0];
  return `<svg class="spark ${down ? 'down' : ''}" viewBox="0 0 ${width} ${height}" width="${width}" height="${height}" aria-hidden="true"><path d="${d}" fill="none" stroke-width="1.8"/></svg>`;
}

/**
 * Histórico de preços com uma linha por link.
 * `series` = [{ label, color, points: [{ t: ms, v: centavos | null }] }], pontos em ordem de tempo.
 */
export function stepChart(series, { height = 220 } = {}) {
  const W = 760;
  const H = height;
  const pad = 10;
  const all = series.flatMap((s) => s.points);
  const values = all.map((p) => p.v).filter((v) => v != null);
  if (!values.length) return '<p class="muted">Sem preços com estoque ainda.</p>';
  const t0 = Math.min(...all.map((p) => p.t));
  const t1 = Math.max(Date.now(), ...all.map((p) => p.t));
  let lo = Math.min(...values);
  let hi = Math.max(...values);
  const margin = Math.max((hi - lo) * 0.15, hi * 0.02, 100);
  lo = Math.max(0, lo - margin);
  hi += margin;
  const x = (t) => (t1 === t0 ? W : ((t - t0) / (t1 - t0)) * W);
  const y = (v) => pad + (1 - (v - lo) / (hi - lo)) * (H - 2 * pad);
  const grid = [0.2, 0.5, 0.8].map((f) => lo + (hi - lo) * f);

  const paths = series.map((s) => {
    const pts = s.points.map((p) => ({ x: x(p.t), y: p.v == null ? null : y(p.v) }));
    const last = s.points[s.points.length - 1];
    if (last) pts.push({ x: W, y: last.v == null ? null : y(last.v) }); // o último preço vale até agora
    return `<path d="${stepPath(pts)}" fill="none" stroke="${s.color}" stroke-width="2.2" vector-effect="non-scaling-stroke"/>`;
  }).join('');

  return `
    <div class="price-chart">
      <div class="chart-ylabels" style="height:${H}px">
        ${grid.map((v) => `<span style="top:${((y(v) / H) * 100).toFixed(1)}%">${brl(Math.round(v))}</span>`).join('')}
      </div>
      <svg viewBox="0 0 ${W} ${H}" preserveAspectRatio="none" style="height:${H}px" role="img" aria-label="Histórico de preços">
        ${grid.map((v) => `<line x1="0" x2="${W}" y1="${n(y(v))}" y2="${n(y(v))}" class="grid-line"/>`).join('')}
        ${paths}
      </svg>
      <div class="chart-xlabels"><span>${dayMonth(t0)}</span><span>agora</span></div>
      <div class="legend">${series.map((s) => `<span><i style="background:${s.color}"></i>${esc(s.label)}</span>`).join('')}</div>
    </div>`;
}
```

**Passo 4: rodar e ver passar**

Rode: `node --test "tests/*.test.js"`
Esperado: todos os testes PASS.

**Passo 5: commit**
```bash
git add src/format.js src/charts.js tests/
git commit -m "Formatação de preços e gráfico em degraus"
```

---

### Tarefa 3: regras do histórico de preços (Rust, TDD)

**Arquivos:**
- Criar: `src-tauri/src/prices.rs`
- Modificar: `src-tauri/src/lib.rs` (acrescentar `mod prices;` no topo)

**Passo 1: módulo com os tipos e os testes (implementações com `todo!()`)**

`src-tauri/src/prices.rs`, primeiro só com os tipos, as assinaturas usando `todo!()` e o bloco de testes abaixo:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn r(price: i64) -> Reading {
        Reading { price: Some(price), list_price: None, in_stock: true, free_shipping: false }
    }
    fn pt(at: &str, reading: Reading) -> PricePoint {
        PricePoint { at: at.into(), reading }
    }

    #[test]
    fn grava_quando_muda_ou_quando_vira_o_dia() {
        let last = pt("2026-10-07T10:00:00", r(10000));
        assert!(should_record(None, &r(10000), "2026-10-07T11:00:00"));
        assert!(!should_record(Some(&last), &r(10000), "2026-10-07T13:00:00"));
        assert!(should_record(Some(&last), &r(9990), "2026-10-07T13:00:00"));
        assert!(should_record(Some(&last), &r(10000), "2026-10-08T08:00:00"));
        let sem_estoque = Reading { in_stock: false, ..r(10000) };
        assert!(should_record(Some(&last), &sem_estoque, "2026-10-07T13:00:00"));
    }

    #[test]
    fn melhor_preco_por_dia_entre_as_lojas() {
        let ml = vec![pt("2026-10-01T10:00:00", r(500)), pt("2026-10-03T10:00:00", r(450))];
        let amz = vec![
            pt("2026-10-02T09:00:00", r(480)),
            pt("2026-10-04T09:00:00", Reading { in_stock: false, ..r(480) }),
        ];
        let links = [ml, amz];
        let days = last_days("2026-10-04", 5);
        assert_eq!(days, ["2026-09-30", "2026-10-01", "2026-10-02", "2026-10-03", "2026-10-04"]);
        assert_eq!(daily_best(&links, &days), vec![None, Some(500), Some(480), Some(450), Some(450)]);
        assert_eq!(lowest_ever(&links), Some(450));
        assert_eq!(current_best(&links), Some((0, 450)));
        assert_eq!(first_best(&links), Some(500));
    }

    #[test]
    fn sem_pontos_nao_tem_preco() {
        let links: [Vec<PricePoint>; 1] = [vec![]];
        assert_eq!(current_best(&links), None);
        assert_eq!(lowest_ever(&links), None);
        assert_eq!(first_best(&links), None);
    }
}
```

**Passo 2: rodar e ver falhar**

Rode: `cargo test --manifest-path src-tauri/Cargo.toml prices`
Esperado: compila e os testes FAIL com `not yet implemented`.

**Passo 3: implementação** (substituir os `todo!()`; o arquivo final fica assim, mais o bloco de testes):
```rust
//! Regras puras sobre o histórico de preços (sem banco nem rede, fáceis de testar).

use serde::{Deserialize, Serialize};

/// O que uma loja informou numa checagem. Valores em centavos.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reading {
    pub price: Option<i64>,
    pub list_price: Option<i64>,
    pub in_stock: bool,
    pub free_shipping: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PricePoint {
    /// Data e hora local, `AAAA-MM-DDTHH:MM:SS`.
    pub at: String,
    #[serde(flatten)]
    pub reading: Reading,
}

fn day(at: &str) -> &str {
    at.get(..10).unwrap_or(at)
}

/// Grava um ponto novo só quando algo muda, ou no primeiro registro do dia
/// (assim o histórico fica leve e o gráfico ainda mostra que houve checagem).
pub fn should_record(last: Option<&PricePoint>, new: &Reading, now: &str) -> bool {
    match last {
        None => true,
        Some(p) => p.reading != *new || day(&p.at) != day(now),
    }
}

/// Preço em vigor no fim do dia `d` (o último ponto até aquele dia). Sem estoque não conta.
pub fn price_on(points: &[PricePoint], d: &str) -> Option<i64> {
    points
        .iter()
        .take_while(|p| day(&p.at) <= d)
        .last()
        .filter(|p| p.reading.in_stock)
        .and_then(|p| p.reading.price)
}

/// Menor preço entre os links em cada dia.
pub fn daily_best(links: &[Vec<PricePoint>], days: &[String]) -> Vec<Option<i64>> {
    days.iter().map(|d| links.iter().filter_map(|pts| price_on(pts, d)).min()).collect()
}

/// Menor preço já registrado com estoque.
pub fn lowest_ever(links: &[Vec<PricePoint>]) -> Option<i64> {
    links.iter().flatten().filter(|p| p.reading.in_stock).filter_map(|p| p.reading.price).min()
}

/// Preço atual de um link: o último ponto, se tiver estoque.
pub fn current(points: &[PricePoint]) -> Option<i64> {
    points.last().filter(|p| p.reading.in_stock).and_then(|p| p.reading.price)
}

/// Índice do link com o menor preço atual, e esse preço.
pub fn current_best(links: &[Vec<PricePoint>]) -> Option<(usize, i64)> {
    links
        .iter()
        .enumerate()
        .filter_map(|(i, pts)| current(pts).map(|p| (i, p)))
        .min_by_key(|&(_, p)| p)
}

/// Melhor preço no primeiro dia acompanhado: base da variação "desde que começou".
pub fn first_best(links: &[Vec<PricePoint>]) -> Option<i64> {
    let first = links.iter().filter_map(|pts| pts.first()).map(|p| day(&p.at).to_string()).min()?;
    daily_best(links, &[first])[0]
}

/// Os últimos `n` dias terminando em `today` (AAAA-MM-DD), do mais antigo para o mais recente.
pub fn last_days(today: &str, n: usize) -> Vec<String> {
    let end = chrono::NaiveDate::parse_from_str(today, "%Y-%m-%d").expect("data no formato AAAA-MM-DD");
    (0..n as i64).rev().map(|i| (end - chrono::Duration::days(i)).format("%Y-%m-%d").to_string()).collect()
}
```

**Passo 4: rodar e ver passar**

Rode: `cargo test --manifest-path src-tauri/Cargo.toml prices`
Esperado: 3 testes PASS. (Avisos de "never used" são normais até a Tarefa 7.)

**Passo 5: commit**
```bash
git add src-tauri/src/prices.rs src-tauri/src/lib.rs
git commit -m "Regras do histórico de preços"
```

---

### Tarefa 4: banco SQLite (Rust, TDD)

**Arquivos:**
- Criar: `src-tauri/src/db.rs`
- Modificar: `src-tauri/src/lib.rs` (`mod db;`)

**Passo 1: testes que falham** (no fim de `db.rs`; as funções públicas começam com `todo!()`):
```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn r(price: i64) -> Reading {
        Reading { price: Some(price), list_price: None, in_stock: true, free_shipping: true }
    }
    fn link(code: &'static str) -> NewLink<'static> {
        NewLink { store: "ml", code, url: "https://produto.mercadolivre.com.br/x", title: "SSD 1TB", image: None }
    }
    fn count(c: &Connection, table: &str) -> i64 {
        c.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn guarda_historico_so_quando_muda() {
        let c = open_in_memory();
        let p = add_product(&c, "SSD", "2026-10-01T08:00:00").unwrap();
        let l = add_link(&c, p, &link("MLB1")).unwrap();
        assert!(record_reading(&c, l, &r(500), "2026-10-01T08:00:00").unwrap());
        assert!(!record_reading(&c, l, &r(500), "2026-10-01T11:00:00").unwrap());
        assert!(record_reading(&c, l, &r(450), "2026-10-01T14:00:00").unwrap());
        assert!(!record_reading(&c, l, &r(450), "2026-10-02T08:00:00").unwrap()); // mesmo preço, mas é o ponto do dia
        assert_eq!(count(&c, "prices"), 3);
    }

    #[test]
    fn resumo_do_produto_com_dois_links() {
        let c = open_in_memory();
        let p = add_product(&c, "SSD Kingston", "2026-10-01T08:00:00").unwrap();
        let a = add_link(&c, p, &link("MLB1")).unwrap();
        let b = add_link(&c, p, &link("MLB2")).unwrap();
        record_reading(&c, a, &r(500), "2026-10-01T08:00:00").unwrap();
        record_reading(&c, b, &r(480), "2026-10-01T08:00:00").unwrap();
        record_reading(&c, a, &r(450), "2026-10-02T08:00:00").unwrap();

        let list = list_products(&c, "2026-10-02").unwrap();
        assert_eq!(list.len(), 1);
        let s = &list[0];
        assert_eq!((s.best_price, s.best_store.as_deref()), (Some(450), Some("ml")));
        assert_eq!((s.lowest_ever, s.first_price), (Some(450), Some(480)));
        assert_eq!(s.spark.len(), SPARK_DAYS);
        assert_eq!(&s.spark[SPARK_DAYS - 2..], &[Some(480), Some(450)]);
        assert_eq!(s.links[0].current.as_ref().and_then(|r| r.price), Some(450));

        let d = product_detail(&c, p, "2026-10-02").unwrap();
        assert_eq!(d.history.len(), 2);
        assert_eq!(d.history[0].points.len(), 2);
    }

    #[test]
    fn link_repetido_falhas_e_exclusao() {
        let c = open_in_memory();
        let p = add_product(&c, "SSD", "2026-10-01T08:00:00").unwrap();
        let l = add_link(&c, p, &link("MLB1")).unwrap();
        assert_eq!(add_link(&c, p, &link("MLB1")).unwrap_err(), "Esse anúncio já está sendo acompanhado");

        record_failure(&c, l, "fora do ar", "2026-10-01T09:00:00").unwrap();
        record_failure(&c, l, "fora do ar", "2026-10-01T10:00:00").unwrap();
        let s = &list_products(&c, "2026-10-01").unwrap()[0];
        assert_eq!((s.links[0].failures, s.links[0].last_error.as_deref()), (2, Some("fora do ar")));
        record_reading(&c, l, &r(500), "2026-10-01T11:00:00").unwrap();
        assert_eq!(list_products(&c, "2026-10-01").unwrap()[0].links[0].failures, 0);

        delete_link(&c, l).unwrap();
        assert_eq!((count(&c, "products"), count(&c, "prices")), (0, 0), "sem links, o produto some junto com o histórico");
    }

    #[test]
    fn reconhece_um_backup_valido() {
        let c = open_in_memory();
        let dir = std::env::temp_dir().join(format!("radar-db-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ok = dir.join("ok.db");
        let _ = std::fs::remove_file(&ok);
        c.execute("VACUUM INTO ?1", [ok.to_string_lossy().into_owned()]).unwrap();
        assert!(check_backup(&ok).is_ok());
        let bad = dir.join("bad.db");
        std::fs::write(&bad, b"nao sou um banco").unwrap();
        assert!(check_backup(&bad).is_err());
    }
}
```

**Passo 2: rodar e ver falhar**

Rode: `cargo test --manifest-path src-tauri/Cargo.toml db::`
Esperado: FAIL (`not yet implemented`).

**Passo 3: implementação**
```rust
//! Banco SQLite local: produtos acompanhados, links por loja e histórico de preços.

use std::path::Path;

use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde::Serialize;

use crate::prices::{self, PricePoint, Reading};

/// Cada item roda uma vez, em ordem; `user_version` guarda quantos já rodaram.
const MIGRATIONS: &[&str] = &[r#"
CREATE TABLE products (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  target_price INTEGER,
  min_drop_pct REAL NOT NULL DEFAULT 5,
  notify_lowest INTEGER NOT NULL DEFAULT 1,
  archived INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL
);
CREATE TABLE links (
  id INTEGER PRIMARY KEY,
  product_id INTEGER NOT NULL REFERENCES products(id) ON DELETE CASCADE,
  store TEXT NOT NULL,
  code TEXT NOT NULL,
  url TEXT NOT NULL,
  title TEXT NOT NULL,
  image TEXT,
  last_check TEXT,
  last_error TEXT,
  failures INTEGER NOT NULL DEFAULT 0,
  UNIQUE (store, code)
);
CREATE TABLE prices (
  id INTEGER PRIMARY KEY,
  link_id INTEGER NOT NULL REFERENCES links(id) ON DELETE CASCADE,
  at TEXT NOT NULL,
  price INTEGER,
  list_price INTEGER,
  in_stock INTEGER NOT NULL,
  free_shipping INTEGER NOT NULL
);
CREATE INDEX prices_link_at ON prices (link_id, at);
"#];

/// Dias do mini gráfico da lista.
pub const SPARK_DAYS: usize = 30;

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let c = Connection::open(path)?;
    setup(&c)?;
    Ok(c)
}

#[cfg(test)]
pub fn open_in_memory() -> Connection {
    let c = Connection::open_in_memory().unwrap();
    setup(&c).unwrap();
    c
}

fn setup(c: &Connection) -> rusqlite::Result<()> {
    c.pragma_update(None, "foreign_keys", true)?;
    let version: i64 = c.pragma_query_value(None, "user_version", |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
        c.execute_batch(sql)?;
        c.pragma_update(None, "user_version", (i + 1) as i64)?;
    }
    Ok(())
}

/// Confere se o arquivo é um banco do Radar antes de restaurar.
pub fn check_backup(path: &Path) -> Result<(), String> {
    const NOT: &str = "Esse arquivo não é um backup do Radar de Preços";
    let c = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|_| NOT)?;
    let ok: bool = c
        .query_row(
            "SELECT count(*) = 3 FROM sqlite_master WHERE type = 'table' AND name IN ('products', 'links', 'prices')",
            [],
            |r| r.get(0),
        )
        .map_err(|_| NOT)?;
    if ok { Ok(()) } else { Err(NOT.into()) }
}

pub struct NewLink<'a> {
    pub store: &'a str,
    pub code: &'a str,
    pub url: &'a str,
    pub title: &'a str,
    pub image: Option<&'a str>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkInfo {
    pub id: i64,
    pub store: String,
    pub code: String,
    pub url: String,
    pub title: String,
    pub image: Option<String>,
    pub last_check: Option<String>,
    pub last_error: Option<String>,
    pub failures: i64,
    /// Última leitura registrada.
    pub current: Option<Reading>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductSummary {
    pub id: i64,
    pub name: String,
    pub target_price: Option<i64>,
    pub links: Vec<LinkInfo>,
    pub best_price: Option<i64>,
    pub best_store: Option<String>,
    pub lowest_ever: Option<i64>,
    /// Melhor preço no primeiro dia acompanhado.
    pub first_price: Option<i64>,
    /// Melhor preço de cada um dos últimos `SPARK_DAYS` dias.
    pub spark: Vec<Option<i64>>,
    pub last_check: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkHistory {
    pub link_id: i64,
    pub store: String,
    pub points: Vec<PricePoint>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductDetail {
    pub product: ProductSummary,
    pub history: Vec<LinkHistory>,
}

pub fn add_product(c: &Connection, name: &str, now: &str) -> rusqlite::Result<i64> {
    c.execute("INSERT INTO products (name, created_at) VALUES (?1, ?2)", params![name.trim(), now])?;
    Ok(c.last_insert_rowid())
}

pub fn add_link(c: &Connection, product_id: i64, l: &NewLink) -> Result<i64, String> {
    c.execute(
        "INSERT INTO links (product_id, store, code, url, title, image) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![product_id, l.store, l.code, l.url, l.title, l.image],
    )
    .map_err(|e| match e {
        rusqlite::Error::SqliteFailure(f, _) if f.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE => {
            "Esse anúncio já está sendo acompanhado".to_string()
        }
        e => e.to_string(),
    })?;
    Ok(c.last_insert_rowid())
}

fn row_to_point(r: &rusqlite::Row) -> rusqlite::Result<PricePoint> {
    Ok(PricePoint {
        at: r.get(0)?,
        reading: Reading { price: r.get(1)?, list_price: r.get(2)?, in_stock: r.get(3)?, free_shipping: r.get(4)? },
    })
}

const POINT_COLS: &str = "at, price, list_price, in_stock, free_shipping";

fn history(c: &Connection, link_id: i64) -> rusqlite::Result<Vec<PricePoint>> {
    let mut stmt = c.prepare(&format!("SELECT {POINT_COLS} FROM prices WHERE link_id = ?1 ORDER BY at, id"))?;
    let rows = stmt.query_map([link_id], row_to_point)?;
    rows.collect()
}

fn last_point(c: &Connection, link_id: i64) -> rusqlite::Result<Option<PricePoint>> {
    c.query_row(
        &format!("SELECT {POINT_COLS} FROM prices WHERE link_id = ?1 ORDER BY at DESC, id DESC LIMIT 1"),
        [link_id],
        row_to_point,
    )
    .optional()
}

/// Registra uma checagem bem-sucedida e zera as falhas.
/// Devolve true se a leitura mudou em relação à anterior (ou se é a primeira).
pub fn record_reading(c: &Connection, link_id: i64, r: &Reading, now: &str) -> rusqlite::Result<bool> {
    let last = last_point(c, link_id)?;
    if prices::should_record(last.as_ref(), r, now) {
        c.execute(
            "INSERT INTO prices (link_id, at, price, list_price, in_stock, free_shipping) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![link_id, now, r.price, r.list_price, r.in_stock, r.free_shipping],
        )?;
    }
    c.execute("UPDATE links SET last_check = ?2, last_error = NULL, failures = 0 WHERE id = ?1", params![link_id, now])?;
    Ok(last.map_or(true, |p| p.reading != *r))
}

pub fn record_failure(c: &Connection, link_id: i64, msg: &str, now: &str) -> rusqlite::Result<()> {
    c.execute(
        "UPDATE links SET last_check = ?2, last_error = ?3, failures = failures + 1 WHERE id = ?1",
        params![link_id, now, msg],
    )?;
    Ok(())
}

/// Atualiza título e foto quando a loja muda o anúncio.
pub fn update_link_meta(c: &Connection, link_id: i64, title: &str, image: Option<&str>) -> rusqlite::Result<()> {
    c.execute("UPDATE links SET title = ?2, image = COALESCE(?3, image) WHERE id = ?1", params![link_id, title, image])?;
    Ok(())
}

/// Links de uma loja em produtos ativos: (id do link, código na loja).
pub fn links_to_check(c: &Connection, store: &str) -> rusqlite::Result<Vec<(i64, String)>> {
    let mut stmt = c.prepare(
        "SELECT l.id, l.code FROM links l JOIN products p ON p.id = l.product_id
         WHERE l.store = ?1 AND p.archived = 0 ORDER BY l.id",
    )?;
    let rows = stmt.query_map([store], |r| Ok((r.get(0)?, r.get(1)?)))?;
    rows.collect()
}

fn summarize(
    c: &Connection,
    id: i64,
    name: String,
    target_price: Option<i64>,
    today: &str,
) -> rusqlite::Result<(ProductSummary, Vec<Vec<PricePoint>>)> {
    let mut stmt = c.prepare(
        "SELECT id, store, code, url, title, image, last_check, last_error, failures
         FROM links WHERE product_id = ?1 ORDER BY id",
    )?;
    let mut links: Vec<LinkInfo> = stmt
        .query_map([id], |r| {
            Ok(LinkInfo {
                id: r.get(0)?,
                store: r.get(1)?,
                code: r.get(2)?,
                url: r.get(3)?,
                title: r.get(4)?,
                image: r.get(5)?,
                last_check: r.get(6)?,
                last_error: r.get(7)?,
                failures: r.get(8)?,
                current: None,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    let hist = links.iter().map(|l| history(c, l.id)).collect::<rusqlite::Result<Vec<_>>>()?;
    for (l, h) in links.iter_mut().zip(&hist) {
        l.current = h.last().map(|p| p.reading.clone());
    }
    let best = prices::current_best(&hist);
    let summary = ProductSummary {
        id,
        name,
        target_price,
        best_price: best.map(|(_, p)| p),
        best_store: best.map(|(i, _)| links[i].store.clone()),
        lowest_ever: prices::lowest_ever(&hist),
        first_price: prices::first_best(&hist),
        spark: prices::daily_best(&hist, &prices::last_days(today, SPARK_DAYS)),
        last_check: links.iter().filter_map(|l| l.last_check.clone()).max(),
        links,
    };
    Ok((summary, hist))
}

pub fn list_products(c: &Connection, today: &str) -> rusqlite::Result<Vec<ProductSummary>> {
    let mut stmt = c.prepare("SELECT id, name, target_price FROM products WHERE archived = 0 ORDER BY name COLLATE NOCASE")?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<i64>>(2)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter().map(|(id, name, tp)| summarize(c, id, name, tp, today).map(|(s, _)| s)).collect()
}

pub fn product_detail(c: &Connection, id: i64, today: &str) -> rusqlite::Result<ProductDetail> {
    let (name, tp) = c.query_row("SELECT name, target_price FROM products WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?)))?;
    let (product, hist) = summarize(c, id, name, tp, today)?;
    let history = product
        .links
        .iter()
        .zip(hist)
        .map(|(l, points)| LinkHistory { link_id: l.id, store: l.store.clone(), points })
        .collect();
    Ok(ProductDetail { product, history })
}

pub fn rename_product(c: &Connection, id: i64, name: &str) -> rusqlite::Result<()> {
    c.execute("UPDATE products SET name = ?2 WHERE id = ?1", params![id, name.trim()])?;
    Ok(())
}

pub fn delete_product(c: &Connection, id: i64) -> rusqlite::Result<()> {
    c.execute("DELETE FROM products WHERE id = ?1", [id])?;
    Ok(())
}

/// Remove um link. Se era o último do produto, o produto sai também.
pub fn delete_link(c: &Connection, link_id: i64) -> rusqlite::Result<()> {
    let product: Option<i64> = c.query_row("SELECT product_id FROM links WHERE id = ?1", [link_id], |r| r.get(0)).optional()?;
    c.execute("DELETE FROM links WHERE id = ?1", [link_id])?;
    if let Some(p) = product {
        c.execute("DELETE FROM products WHERE id = ?1 AND NOT EXISTS (SELECT 1 FROM links WHERE product_id = ?1)", [p])?;
    }
    Ok(())
}
```

**Passo 4: rodar e ver passar**

Rode: `cargo test --manifest-path src-tauri/Cargo.toml db::`
Esperado: 4 testes PASS. A primeira compilação do `rusqlite` bundled demora uns minutos.

**Passo 5: commit**
```bash
git add src-tauri/src/db.rs src-tauri/src/lib.rs
git commit -m "Banco SQLite de produtos, links e histórico"
```

---

### Tarefa 5: leitura das respostas do Mercado Livre (Rust, TDD com fixtures)

As fixtures agora são **escritas à mão**, no formato documentado da API. Na Tarefa 8 elas são conferidas contra respostas reais.

**Arquivos:**
- Criar: `src-tauri/tests/fixtures/ml/search.json`, `items.json`, `sale_price.json`, `sale_price_sem_promo.json`, `product.json`
- Criar: `src-tauri/src/ml.rs`
- Modificar: `src-tauri/src/lib.rs` (`mod ml;`)

**Passo 1: fixtures**

`search.json`:
```json
{
  "site_id": "MLB",
  "query": "ssd 1tb",
  "paging": { "total": 3, "offset": 0, "limit": 50 },
  "results": [
    {
      "id": "MLB3456789012",
      "title": "SSD Kingston NV2 1TB M.2 NVMe",
      "condition": "new",
      "thumbnail": "http://http2.mlstatic.com/D_123456-MLA0000000000_012024-I.jpg",
      "permalink": "https://produto.mercadolivre.com.br/MLB-3456789012-ssd-kingston-nv2-1tb-_JM",
      "price": 389.9,
      "original_price": 499.9,
      "currency_id": "BRL",
      "available_quantity": 50,
      "shipping": { "free_shipping": true, "logistic_type": "fulfillment" },
      "seller": { "id": 123, "nickname": "KINGSTON OFICIAL" }
    },
    {
      "id": "MLB2222222222",
      "title": "SSD Sandisk 1TB",
      "thumbnail": "https://http2.mlstatic.com/D_2.jpg",
      "permalink": "https://produto.mercadolivre.com.br/MLB-2222222222-ssd-sandisk-_JM",
      "price": 359,
      "original_price": null,
      "shipping": { "free_shipping": false, "logistic_type": "cross_docking" },
      "seller": { "id": 456 }
    },
    {
      "id": "MLB3333333333",
      "title": "Anúncio sem preço",
      "permalink": "https://produto.mercadolivre.com.br/MLB-3333333333-x-_JM",
      "price": null
    }
  ]
}
```

`items.json` (consulta em lote `/items?ids=…`):
```json
[
  { "code": 200, "body": { "id": "MLB3456789012", "title": "SSD Kingston NV2 1TB M.2 NVMe", "price": 379.9, "original_price": 499.9,
      "status": "active", "available_quantity": 12, "permalink": "https://produto.mercadolivre.com.br/MLB-3456789012-ssd-_JM",
      "thumbnail": "http://http2.mlstatic.com/D_1.jpg", "shipping": { "free_shipping": true, "logistic_type": "fulfillment" } } },
  { "code": 200, "body": { "id": "MLB2222222222", "title": "SSD Sandisk 1TB", "price": 359, "original_price": null,
      "status": "paused", "available_quantity": 0, "permalink": "https://produto.mercadolivre.com.br/MLB-2222222222-ssd-_JM",
      "thumbnail": "https://http2.mlstatic.com/D_2.jpg", "shipping": { "free_shipping": false } } },
  { "code": 404, "body": { "message": "Item with id MLB9999999999 not found", "error": "not_found", "status": 404, "cause": [] } }
]
```

`sale_price.json`:
```json
{ "price_id": "MLB3456789012-1", "amount": 369.9, "regular_amount": 499.9, "currency_id": "BRL", "reference_date": "2026-10-07T12:00:00Z", "metadata": {} }
```

`sale_price_sem_promo.json`:
```json
{ "price_id": "MLB2222222222-1", "amount": 359, "regular_amount": null, "currency_id": "BRL", "reference_date": "2026-10-07T12:00:00Z", "metadata": {} }
```

`product.json` (página de catálogo `/products/{id}`):
```json
{ "id": "MLB19698968", "name": "SSD Kingston NV2 1TB", "status": "active", "buy_box_winner": { "item_id": "MLB3456789012", "price": 379.9 } }
```

**Passo 2: testes que falham** (no fim de `ml.rs`):
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_a_busca() {
        let offers = parse_search(include_str!("../tests/fixtures/ml/search.json")).unwrap();
        assert_eq!(offers.len(), 2, "anúncio sem preço fica de fora");
        let a = &offers[0];
        assert_eq!((a.code.as_str(), a.price, a.list_price), ("MLB3456789012", 38990, Some(49990)));
        assert!(a.free_shipping && a.full);
        assert_eq!(a.image.as_deref(), Some("https://http2.mlstatic.com/D_123456-MLA0000000000_012024-I.jpg"));
        assert_eq!(a.seller.as_deref(), Some("KINGSTON OFICIAL"));
        let b = &offers[1];
        assert_eq!((b.price, b.list_price, b.free_shipping, b.full, b.seller.clone()), (35900, None, false, false, None));
    }

    #[test]
    fn le_a_consulta_em_lote() {
        let items = parse_items(include_str!("../tests/fixtures/ml/items.json")).unwrap();
        assert_eq!(items.len(), 3);
        let a = items[0].as_ref().unwrap();
        assert_eq!(a.reading, Reading { price: Some(37990), list_price: Some(49990), in_stock: true, free_shipping: true });
        assert_eq!(a.image.as_deref(), Some("https://http2.mlstatic.com/D_1.jpg"));
        let b = items[1].as_ref().unwrap();
        assert!(!b.reading.in_stock, "anúncio pausado conta como sem estoque");
        assert_eq!(items[2].as_ref().unwrap_err(), "Anúncio não encontrado no Mercado Livre");
    }

    #[test]
    fn le_o_preco_de_venda_e_o_catalogo() {
        assert_eq!(parse_sale_price(include_str!("../tests/fixtures/ml/sale_price.json")), Some((36990, Some(49990))));
        assert_eq!(parse_sale_price(include_str!("../tests/fixtures/ml/sale_price_sem_promo.json")), Some((35900, None)));
        assert_eq!(parse_sale_price("{}"), None);
        assert_eq!(parse_buy_box_winner(include_str!("../tests/fixtures/ml/product.json")).as_deref(), Some("MLB3456789012"));
        assert_eq!(parse_buy_box_winner(r#"{"id":"MLB1","buy_box_winner":null}"#), None);
        assert_eq!(parse_nickname(r#"{"id":1,"nickname":"JEANK"}"#).as_deref(), Some("JEANK"));
    }

    #[test]
    fn reconhece_links_do_mercado_livre() {
        use LinkRef::*;
        let item = || Some(Item("MLB3456789012".into()));
        assert_eq!(parse_link("https://produto.mercadolivre.com.br/MLB-3456789012-ssd-kingston-_JM"), item());
        assert_eq!(parse_link("https://www.mercadolivre.com.br/ssd/p/MLB19698968#wid=MLB3456789012&sid=search"), item());
        assert_eq!(parse_link("https://www.mercadolivre.com.br/ssd/p/MLB19698968?pdp_filters=item_id:MLB3456789012"), item());
        assert_eq!(parse_link("  mlb3456789012 "), item());
        assert_eq!(parse_link("https://www.mercadolivre.com.br/ssd-kingston/p/MLB19698968"), Some(Catalog("MLB19698968".into())));
        assert_eq!(parse_link("https://www.amazon.com.br/dp/B0ABCDEFGH"), None);
    }
}
```

**Passo 3: rodar e ver falhar**

Rode: `cargo test --manifest-path src-tauri/Cargo.toml ml::`
Esperado: FAIL.

**Passo 4: implementação** (`src-tauri/src/ml.rs`, antes dos testes):
```rust
//! Mercado Livre: leitura das respostas da API oficial e chamadas HTTP.

use serde::{Deserialize, Serialize};

use crate::prices::Reading;

pub const STORE: &str = "ml";
pub const SITE: &str = "MLB";
const API: &str = "https://api.mercadolibre.com";
/// Máximo de anúncios por consulta em lote.
pub const BATCH: usize = 20;
/// Campos pedidos na consulta em lote, para respostas menores.
pub const ITEM_ATTRS: &str = "id,title,price,original_price,status,available_quantity,permalink,thumbnail,shipping";

/// Um resultado de busca, já em centavos. Também é o que a interface manda de volta para "Acompanhar".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Offer {
    pub store: String,
    pub code: String,
    pub title: String,
    pub url: String,
    pub image: Option<String>,
    pub price: i64,
    pub list_price: Option<i64>,
    pub free_shipping: bool,
    pub full: bool,
    pub seller: Option<String>,
}

/// Um anúncio consultado pelo código.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemInfo {
    pub code: String,
    pub title: String,
    pub url: String,
    pub image: Option<String>,
    pub reading: Reading,
}

pub fn to_cents(v: f64) -> i64 {
    (v * 100.0).round() as i64
}

/// A API ainda devolve algumas fotos em http, que a CSP do app bloqueia.
fn https(url: Option<String>) -> Option<String> {
    url.map(|u| match u.strip_prefix("http://") {
        Some(rest) => format!("https://{rest}"),
        None => u,
    })
}

/// O preço "de" só conta se for maior que o preço atual.
fn list_price(price: i64, original: Option<f64>) -> Option<i64> {
    original.map(to_cents).filter(|&o| o > price)
}

#[derive(Deserialize, Default)]
struct Shipping {
    #[serde(default)]
    free_shipping: bool,
    logistic_type: Option<String>,
}

impl Shipping {
    /// "Full": o produto sai do armazém do Mercado Livre.
    fn full(&self) -> bool {
        self.logistic_type.as_deref() == Some("fulfillment")
    }
}

#[derive(Deserialize)]
struct SearchResp {
    results: Vec<SearchItem>,
}

#[derive(Deserialize)]
struct SearchItem {
    id: String,
    title: String,
    price: Option<f64>,
    original_price: Option<f64>,
    permalink: String,
    thumbnail: Option<String>,
    shipping: Option<Shipping>,
    seller: Option<Seller>,
}

#[derive(Deserialize)]
struct Seller {
    nickname: Option<String>,
}

pub fn parse_search(json: &str) -> Result<Vec<Offer>, String> {
    let resp: SearchResp =
        serde_json::from_str(json).map_err(|e| format!("Resposta inesperada da busca do Mercado Livre: {e}"))?;
    Ok(resp
        .results
        .into_iter()
        .filter_map(|it| {
            let price = to_cents(it.price?);
            let ship = it.shipping.unwrap_or_default();
            Some(Offer {
                store: STORE.into(),
                code: it.id,
                title: it.title,
                url: it.permalink,
                image: https(it.thumbnail),
                price,
                list_price: list_price(price, it.original_price),
                free_shipping: ship.free_shipping,
                full: ship.full(),
                seller: it.seller.and_then(|s| s.nickname),
            })
        })
        .collect())
}

#[derive(Deserialize)]
struct BatchEntry {
    code: u16,
    body: serde_json::Value,
}

#[derive(Deserialize)]
struct ItemBody {
    id: String,
    title: String,
    price: Option<f64>,
    original_price: Option<f64>,
    status: String,
    available_quantity: Option<i64>,
    permalink: String,
    thumbnail: Option<String>,
    shipping: Option<Shipping>,
}

/// Consulta em lote: um resultado por código pedido, **na mesma ordem** do pedido.
pub fn parse_items(json: &str) -> Result<Vec<Result<ItemInfo, String>>, String> {
    let entries: Vec<BatchEntry> =
        serde_json::from_str(json).map_err(|e| format!("Resposta inesperada do Mercado Livre: {e}"))?;
    Ok(entries
        .into_iter()
        .map(|e| match e.code {
            200 => serde_json::from_value::<ItemBody>(e.body)
                .map(item_info)
                .map_err(|err| format!("Resposta inesperada do anúncio: {err}")),
            404 => Err("Anúncio não encontrado no Mercado Livre".into()),
            c => Err(format!("O Mercado Livre respondeu {c} para este anúncio")),
        })
        .collect())
}

fn item_info(b: ItemBody) -> ItemInfo {
    let price = b.price.map(to_cents);
    let ship = b.shipping.unwrap_or_default();
    let in_stock = b.status == "active" && b.available_quantity.map_or(true, |q| q > 0);
    ItemInfo {
        code: b.id,
        title: b.title,
        url: b.permalink,
        image: https(b.thumbnail),
        reading: Reading {
            price,
            list_price: price.and_then(|p| list_price(p, b.original_price)),
            in_stock,
            free_shipping: ship.free_shipping,
        },
    }
}

/// Preço de venda real, com promoções (`/items/{id}/sale_price`): (preço, preço "de").
pub fn parse_sale_price(json: &str) -> Option<(i64, Option<i64>)> {
    #[derive(Deserialize)]
    struct Sale {
        amount: Option<f64>,
        regular_amount: Option<f64>,
    }
    let s: Sale = serde_json::from_str(json).ok()?;
    let price = to_cents(s.amount?);
    Some((price, list_price(price, s.regular_amount)))
}

/// Anúncio que está vendendo numa página de catálogo (`/products/{id}`).
pub fn parse_buy_box_winner(json: &str) -> Option<String> {
    #[derive(Deserialize)]
    struct Winner {
        item_id: String,
    }
    #[derive(Deserialize)]
    struct Product {
        buy_box_winner: Option<Winner>,
    }
    serde_json::from_str::<Product>(json).ok()?.buy_box_winner.map(|w| w.item_id)
}

pub fn parse_nickname(json: &str) -> Option<String> {
    #[derive(Deserialize)]
    struct Me {
        nickname: String,
    }
    serde_json::from_str::<Me>(json).ok().map(|m| m.nickname)
}

#[derive(Debug, PartialEq)]
pub enum LinkRef {
    /// Anúncio (MLB + dígitos), consultado direto.
    Item(String),
    /// Página de catálogo (`/p/MLB…`): é preciso descobrir qual anúncio está vendendo.
    Catalog(String),
}

/// Dígitos logo depois de `pat` (ignorando um hífen), se forem pelo menos 6.
fn digits_after(s: &str, pat: &str) -> Option<String> {
    let mut rest = s;
    while let Some(i) = rest.find(pat) {
        let tail = &rest[i + pat.len()..];
        let tail = tail.strip_prefix('-').unwrap_or(tail);
        let digits: String = tail.chars().take_while(|c| c.is_ascii_digit()).collect();
        if digits.len() >= 6 {
            return Some(digits);
        }
        rest = &rest[i + pat.len()..];
    }
    None
}

/// Reconhece o que o usuário colou: link de anúncio, de catálogo ou só o código.
pub fn parse_link(input: &str) -> Option<LinkRef> {
    let s = input.trim().to_uppercase();
    // Catálogo com o anúncio escolhido: ...#wid=MLB123 ou ...item_id:MLB123
    for pat in ["WID=MLB", "ITEM_ID:MLB", "ITEM_ID%3AMLB"] {
        if let Some(d) = digits_after(&s, pat) {
            return Some(LinkRef::Item(format!("MLB{d}")));
        }
    }
    if let Some(d) = digits_after(&s, "/P/MLB") {
        return Some(LinkRef::Catalog(format!("MLB{d}")));
    }
    digits_after(&s, "MLB").map(|d| LinkRef::Item(format!("MLB{d}")))
}

#[derive(Debug)]
pub enum ApiError {
    Unauthorized,
    Forbidden(String),
    NotFound,
    TooMany,
    Other(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::Unauthorized => write!(f, "O acesso ao Mercado Livre expirou. Conecte de novo em Ajustes"),
            ApiError::Forbidden(body) => write!(f, "O Mercado Livre recusou a consulta (403): {}", short(body)),
            ApiError::NotFound => write!(f, "Não encontrado no Mercado Livre"),
            ApiError::TooMany => write!(f, "Muitas consultas seguidas. Tente de novo em alguns minutos"),
            ApiError::Other(msg) => write!(f, "{msg}"),
        }
    }
}

fn short(body: &str) -> String {
    body.chars().take(200).collect()
}

/// GET autenticado na API. Devolve o corpo da resposta.
pub async fn get(http: &reqwest::Client, token: &str, path: &str, query: &[(&str, &str)]) -> Result<String, ApiError> {
    let resp = http
        .get(format!("{API}{path}"))
        .bearer_auth(token)
        .query(query)
        .send()
        .await
        .map_err(|e| ApiError::Other(format!("Sem conexão com o Mercado Livre: {e}")))?;
    let status = resp.status().as_u16();
    let body = resp.text().await.map_err(|e| ApiError::Other(e.to_string()))?;
    match status {
        200..=299 => Ok(body),
        401 => Err(ApiError::Unauthorized),
        403 => Err(ApiError::Forbidden(body)),
        404 => Err(ApiError::NotFound),
        429 => Err(ApiError::TooMany),
        _ => Err(ApiError::Other(format!("O Mercado Livre respondeu {status}: {}", short(&body)))),
    }
}
```

**Passo 5: rodar e ver passar**

Rode: `cargo test --manifest-path src-tauri/Cargo.toml ml::`
Esperado: 4 testes PASS.

**Passo 6: commit**
```bash
git add src-tauri/src/ml.rs src-tauri/src/lib.rs src-tauri/tests/
git commit -m "Leitura das respostas da API do Mercado Livre"
```

---

### Tarefa 6: login OAuth com PKCE (Rust, TDD na parte pura)

**Arquivos:**
- Criar: `src-tauri/src/ml_auth.rs`, `src-tauri/src/secrets.rs`
- Modificar: `src-tauri/src/lib.rs` (`mod ml_auth; mod secrets;`)

**Passo 1: testes que falham** (fim de `ml_auth.rs`):
```rust
#[cfg(test)]
mod tests {
    use super::*;

    const REDIRECT: &str = "https://gocomercio.com.br/oauth/mercadolivre/callback";

    #[test]
    fn pkce_segue_a_rfc_7636() {
        // Exemplo do apêndice B da RFC 7636.
        assert_eq!(challenge_for("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
        let p = pkce();
        assert_eq!(p.verifier.len(), 43);
        assert_eq!(p.challenge, challenge_for(&p.verifier));
    }

    #[test]
    fn monta_o_endereco_de_autorizacao() {
        let u = authorize_url("417769415941125", REDIRECT, "abc", "st1").unwrap();
        assert_eq!(u.host_str(), Some("auth.mercadolivre.com.br"));
        let q: std::collections::HashMap<_, _> = u.query_pairs().into_owned().collect();
        assert_eq!(q["client_id"], "417769415941125");
        assert_eq!(q["redirect_uri"], REDIRECT);
        assert_eq!((q["code_challenge"].as_str(), q["code_challenge_method"].as_str()), ("abc", "S256"));
        assert_eq!(q["state"], "st1");
    }

    #[test]
    fn captura_o_retorno_do_login() {
        assert_eq!(callback_code("https://www.mercadolivre.com.br/login", REDIRECT, "st1"), None);
        assert_eq!(callback_code("https://gocomercio.com.br/outra", REDIRECT, "st1"), None);
        assert_eq!(callback_code(&format!("{REDIRECT}?code=TG-123&state=st1"), REDIRECT, "st1"), Some(Ok("TG-123".into())));
        assert!(matches!(callback_code(&format!("{REDIRECT}?code=TG-123&state=outro"), REDIRECT, "st1"), Some(Err(_))));
        assert!(matches!(callback_code(&format!("{REDIRECT}?error=access_denied"), REDIRECT, "st1"), Some(Err(_))));
    }
}
```

**Passo 2: rodar e ver falhar**

Rode: `cargo test --manifest-path src-tauri/Cargo.toml ml_auth`
Esperado: FAIL.

**Passo 3: implementação**

`src-tauri/src/ml_auth.rs`:
```rust
//! Login no Mercado Livre (OAuth 2 com PKCE) e renovação do acesso.
//!
//! O ML não aceita token só do app (`client_credentials`): o usuário entra uma vez numa janela do Radar,
//! o app captura o `code` no redirect e troca pelo token. O refresh token é de uso único: cada renovação
//! devolve um novo, que precisa ser guardado na hora.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64URL, Engine};
use rand::{rngs::OsRng, RngCore};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tauri::Url;

const AUTH_URL: &str = "https://auth.mercadolivre.com.br/authorization";
const TOKEN_URL: &str = "https://api.mercadolibre.com/oauth/token";

fn random_b64(bytes: usize) -> String {
    let mut b = vec![0u8; bytes];
    OsRng.fill_bytes(&mut b);
    B64URL.encode(b)
}

pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
}

pub fn pkce() -> Pkce {
    let verifier = random_b64(32);
    let challenge = challenge_for(&verifier);
    Pkce { verifier, challenge }
}

pub fn challenge_for(verifier: &str) -> String {
    B64URL.encode(Sha256::digest(verifier.as_bytes()))
}

/// Valor aleatório que volta no redirect, para recusar retornos que o app não pediu.
pub fn random_state() -> String {
    random_b64(16)
}

pub fn authorize_url(client_id: &str, redirect: &str, challenge: &str, state: &str) -> Result<Url, String> {
    Url::parse_with_params(
        AUTH_URL,
        &[
            ("response_type", "code"),
            ("client_id", client_id),
            ("redirect_uri", redirect),
            ("code_challenge", challenge),
            ("code_challenge_method", "S256"),
            ("state", state),
        ],
    )
    .map_err(|e| e.to_string())
}

/// Se `url` é o retorno do login (mesmo endereço do redirect), devolve o código ou o motivo da recusa.
/// `None` para qualquer outra página, que a janela continua carregando normalmente.
pub fn callback_code(url: &str, redirect: &str, state: &str) -> Option<Result<String, String>> {
    let u = Url::parse(url).ok()?;
    let r = Url::parse(redirect).ok()?;
    if u.scheme() != r.scheme() || u.host_str() != r.host_str() || u.path() != r.path() {
        return None;
    }
    let get = |k: &str| u.query_pairs().find(|(key, _)| key == k).map(|(_, v)| v.into_owned());
    if let Some(err) = get("error") {
        return Some(Err(format!("O Mercado Livre recusou o login: {}", get("error_description").unwrap_or(err))));
    }
    if get("state").as_deref() != Some(state) {
        return Some(Err("Retorno do login inválido. Tente conectar de novo".into()));
    }
    Some(get("code").ok_or_else(|| "O Mercado Livre não devolveu o código de acesso".to_string()))
}

#[derive(Debug, Deserialize)]
pub struct Token {
    pub access_token: String,
    /// Segundos (o ML usa 6 horas).
    pub expires_in: i64,
    pub refresh_token: Option<String>,
}

#[derive(Debug)]
pub enum TokenError {
    /// Refresh token vencido ou já usado: é preciso conectar de novo.
    Expired,
    Other(String),
}

impl std::fmt::Display for TokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenError::Expired => write!(f, "A conexão com o Mercado Livre expirou. Conecte de novo em Ajustes"),
            TokenError::Other(msg) => write!(f, "{msg}"),
        }
    }
}

async fn post(http: &reqwest::Client, form: &[(&str, &str)]) -> Result<Token, TokenError> {
    let resp = http
        .post(TOKEN_URL)
        .header("accept", "application/json")
        .form(form)
        .send()
        .await
        .map_err(|e| TokenError::Other(format!("Sem conexão com o Mercado Livre: {e}")))?;
    let status = resp.status();
    let body = resp.text().await.map_err(|e| TokenError::Other(e.to_string()))?;
    if status.is_success() {
        return serde_json::from_str(&body).map_err(|e| TokenError::Other(format!("Resposta inesperada do login: {e}")));
    }
    if body.contains("invalid_grant") {
        return Err(TokenError::Expired);
    }
    Err(TokenError::Other(format!("O Mercado Livre recusou o acesso ({status}): {body}")))
}

pub async fn exchange_code(
    http: &reqwest::Client,
    client_id: &str,
    secret: &str,
    redirect: &str,
    code: &str,
    verifier: &str,
) -> Result<Token, TokenError> {
    post(
        http,
        &[
            ("grant_type", "authorization_code"),
            ("client_id", client_id),
            ("client_secret", secret),
            ("code", code),
            ("redirect_uri", redirect),
            ("code_verifier", verifier),
        ],
    )
    .await
}

pub async fn refresh(http: &reqwest::Client, client_id: &str, secret: &str, refresh_token: &str) -> Result<Token, TokenError> {
    post(
        http,
        &[
            ("grant_type", "refresh_token"),
            ("client_id", client_id),
            ("client_secret", secret),
            ("refresh_token", refresh_token),
        ],
    )
    .await
}
```

`src-tauri/src/secrets.rs` (sem teste automático porque usa o cofre do Windows; é verificado à mão na Tarefa 8):
```rust
//! Segredos no Gerenciador de Credenciais do Windows: ficam fora do banco, do backup e do git.

use keyring::{Entry, Error};

const SERVICE: &str = "com.jeank.radar";
pub const ML_SECRET: &str = "ml-client-secret";
pub const ML_REFRESH: &str = "ml-refresh-token";

fn entry(key: &str) -> Result<Entry, String> {
    Entry::new(SERVICE, key).map_err(|e| format!("Gerenciador de Credenciais do Windows indisponível: {e}"))
}

pub fn get(key: &str) -> Result<Option<String>, String> {
    match entry(key)?.get_password() {
        Ok(v) => Ok(Some(v)),
        Err(Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("Não foi possível ler do Gerenciador de Credenciais: {e}")),
    }
}

pub fn set(key: &str, value: &str) -> Result<(), String> {
    entry(key)?.set_password(value).map_err(|e| format!("Não foi possível gravar no Gerenciador de Credenciais: {e}"))
}

pub fn delete(key: &str) -> Result<(), String> {
    match entry(key)?.delete_credential() {
        Ok(()) | Err(Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("Não foi possível apagar do Gerenciador de Credenciais: {e}")),
    }
}
```

**Passo 4: rodar e ver passar**

Rode: `cargo test --manifest-path src-tauri/Cargo.toml ml_auth`
Esperado: 3 testes PASS.

**Passo 5: commit**
```bash
git add src-tauri/src/ml_auth.rs src-tauri/src/secrets.rs src-tauri/src/lib.rs
git commit -m "Login no Mercado Livre com PKCE e segredos no Windows"
```

---

### Tarefa 7: estado do app, configuração e conexão com o ML + tela de Ajustes

**Arquivos:**
- Modificar: `src-tauri/src/lib.rs` (versão completa abaixo; as próximas tarefas só acrescentam comandos)
- Modificar: `src/settings.js` (versão só com o cartão do ML; o backup entra na Tarefa 12)
- Modificar: `src/style.css` (regras de Ajustes)

**Passo 1: `src-tauri/src/lib.rs`**
```rust
//! Núcleo do Radar de Preços: banco local, Mercado Livre e comandos da interface.

mod db;
mod ml;
mod ml_auth;
mod prices;
mod secrets;

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
    time::Duration,
};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

const DB_FILE: &str = "radar.db";
const ML_LOGIN_WINDOW: &str = "ml-login";
/// Do app "Radar Ofertas JEV" no DevCenter. O Client ID é público; a chave secreta não fica no código.
const DEFAULT_ML_CLIENT_ID: &str = "417769415941125";
/// Precisa ser idêntica à cadastrada no DevCenter. A página nunca chega a carregar: o app captura antes.
const DEFAULT_ML_REDIRECT: &str = "https://gocomercio.com.br/oauth/mercadolivre/callback";

type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[derive(Default)]
struct MlSession {
    access_token: Option<String>,
    /// Segundos Unix.
    expires_at: i64,
}

struct PendingLogin {
    verifier: String,
}

struct AppState {
    db: Mutex<Connection>,
    http: reqwest::Client,
    /// Mutex assíncrono: duas renovações ao mesmo tempo gastariam o refresh token (uso único).
    ml: tauri::async_runtime::Mutex<MlSession>,
    pending_login: Mutex<Option<PendingLogin>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn data_dir(app: &AppHandle) -> CmdResult<PathBuf> {
    let dir = app.path().app_data_dir().map_err(|e| format!("Pasta de dados indisponível: {e}"))?;
    fs::create_dir_all(&dir).map_err(|e| format!("Não foi possível criar a pasta de dados: {e}"))?;
    Ok(dir)
}

/// Configurações que não são secretas (texto puro em `config.json`, ao lado do banco).
#[derive(Serialize, Deserialize)]
#[serde(default)]
struct Config {
    backup_dir: Option<String>,
    ml_client_id: String,
    ml_redirect_uri: String,
    ml_nickname: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            backup_dir: None,
            ml_client_id: DEFAULT_ML_CLIENT_ID.into(),
            ml_redirect_uri: DEFAULT_ML_REDIRECT.into(),
            ml_nickname: None,
        }
    }
}

fn load_config(app: &AppHandle) -> Config {
    data_dir(app)
        .ok()
        .and_then(|d| fs::read(d.join("config.json")).ok())
        .and_then(|raw| serde_json::from_slice(&raw).ok())
        .unwrap_or_default()
}

fn save_config(app: &AppHandle, config: &Config) -> CmdResult<()> {
    let json = serde_json::to_vec_pretty(config).map_err(err)?;
    fs::write(data_dir(app)?.join("config.json"), json).map_err(|e| format!("Não foi possível salvar as configurações: {e}"))
}

fn now() -> String {
    chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

fn now_secs() -> i64 {
    chrono::Utc::now().timestamp()
}

// ---------- acesso ao Mercado Livre ----------

/// Token válido, renovando com o refresh token quando faltar menos de 1 minuto.
async fn ensure_token(state: &AppState, cfg: &Config) -> CmdResult<String> {
    let mut s = state.ml.lock().await;
    if let Some(t) = &s.access_token {
        if now_secs() < s.expires_at - 60 {
            return Ok(t.clone());
        }
    }
    let refresh = secrets::get(secrets::ML_REFRESH)?.ok_or("Conecte sua conta do Mercado Livre em Ajustes")?;
    let secret = secrets::get(secrets::ML_SECRET)?.ok_or("Informe a chave secreta do Mercado Livre em Ajustes")?;
    match ml_auth::refresh(&state.http, &cfg.ml_client_id, &secret, &refresh).await {
        Ok(tok) => {
            if let Some(r) = &tok.refresh_token {
                secrets::set(secrets::ML_REFRESH, r)?;
            }
            s.access_token = Some(tok.access_token.clone());
            s.expires_at = now_secs() + tok.expires_in;
            Ok(tok.access_token)
        }
        Err(ml_auth::TokenError::Expired) => {
            secrets::delete(secrets::ML_REFRESH)?;
            Err(ml_auth::TokenError::Expired.to_string())
        }
        Err(e) => Err(e.to_string()),
    }
}

/// GET na API do ML. Se o token for recusado (401), renova uma vez e tenta de novo.
async fn ml_get(app: &AppHandle, state: &AppState, path: &str, query: &[(&str, &str)]) -> CmdResult<String> {
    let cfg = load_config(app);
    for attempt in 0..2 {
        let token = ensure_token(state, &cfg).await?;
        match ml::get(&state.http, &token, path, query).await {
            Err(ml::ApiError::Unauthorized) if attempt == 0 => state.ml.lock().await.access_token = None,
            r => return r.map_err(err),
        }
    }
    Err(ml::ApiError::Unauthorized.to_string())
}

// ---------- comandos: conta do Mercado Livre ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MlStatus {
    client_id: String,
    redirect_uri: String,
    has_secret: bool,
    connected: bool,
    nickname: Option<String>,
}

#[tauri::command]
fn ml_status(app: AppHandle) -> CmdResult<MlStatus> {
    let cfg = load_config(&app);
    Ok(MlStatus {
        client_id: cfg.ml_client_id,
        redirect_uri: cfg.ml_redirect_uri,
        has_secret: secrets::get(secrets::ML_SECRET)?.is_some(),
        connected: secrets::get(secrets::ML_REFRESH)?.is_some(),
        nickname: cfg.ml_nickname,
    })
}

/// Salva Client ID e redirect. A chave secreta só é trocada quando vem preenchida.
/// Mudar Client ID ou redirect desconecta, porque o acesso pertence ao app antigo.
#[tauri::command]
async fn ml_save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    client_id: String,
    redirect_uri: String,
    client_secret: Option<String>,
) -> CmdResult<MlStatus> {
    let client_id = client_id.trim().to_string();
    let redirect_uri = redirect_uri.trim().to_string();
    if client_id.is_empty() || !client_id.chars().all(|c| c.is_ascii_digit()) {
        return Err("O Client ID tem só números".into());
    }
    if !redirect_uri.starts_with("https://") {
        return Err("A URI de redirect precisa começar com https://".into());
    }
    let mut cfg = load_config(&app);
    let changed = cfg.ml_client_id != client_id || cfg.ml_redirect_uri != redirect_uri;
    if let Some(secret) = client_secret.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) {
        secrets::set(secrets::ML_SECRET, &secret)?;
    }
    if changed {
        secrets::delete(secrets::ML_REFRESH)?;
        cfg.ml_nickname = None;
        state.ml.lock().await.access_token = None;
    }
    cfg.ml_client_id = client_id;
    cfg.ml_redirect_uri = redirect_uri;
    save_config(&app, &cfg)?;
    ml_status(app)
}

/// Abre a janela de login do ML. O resultado chega à interface pelo evento `ml-login`.
#[tauri::command]
fn ml_connect(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    let cfg = load_config(&app);
    if secrets::get(secrets::ML_SECRET)?.is_none() {
        return Err("Salve a chave secreta antes de conectar".into());
    }
    let pkce = ml_auth::pkce();
    let st = ml_auth::random_state();
    let url = ml_auth::authorize_url(&cfg.ml_client_id, &cfg.ml_redirect_uri, &pkce.challenge, &st)?;
    *lock(&state.pending_login) = Some(PendingLogin { verifier: pkce.verifier });
    if let Some(w) = app.get_webview_window(ML_LOGIN_WINDOW) {
        let _ = w.close();
    }
    let redirect = cfg.ml_redirect_uri;
    let handle = app.clone();
    WebviewWindowBuilder::new(&app, ML_LOGIN_WINDOW, WebviewUrl::External(url))
        .title("Conectar ao Mercado Livre")
        .inner_size(520.0, 760.0)
        .center()
        .on_navigation(move |u| match ml_auth::callback_code(u.as_str(), &redirect, &st) {
            None => true,
            Some(code) => {
                // Cancela a navegação: o endereço do redirect nunca é carregado.
                let h = handle.clone();
                tauri::async_runtime::spawn(async move { finish_login(h, code).await });
                false
            }
        })
        .build()
        .map_err(err)?;
    Ok(())
}

async fn finish_login(app: AppHandle, code: Result<String, String>) {
    if let Some(w) = app.get_webview_window(ML_LOGIN_WINDOW) {
        let _ = w.close();
    }
    let payload = match complete_login(&app, code).await {
        Ok(nickname) => json!({ "ok": true, "nickname": nickname }),
        Err(error) => json!({ "ok": false, "error": error }),
    };
    let _ = app.emit("ml-login", payload);
}

async fn complete_login(app: &AppHandle, code: Result<String, String>) -> CmdResult<Option<String>> {
    let code = code?;
    let state = app.state::<AppState>();
    let pending = lock(&state.pending_login).take().ok_or("O login expirou. Tente conectar de novo")?;
    let mut cfg = load_config(app);
    let secret = secrets::get(secrets::ML_SECRET)?.ok_or("Salve a chave secreta antes de conectar")?;
    let token = ml_auth::exchange_code(&state.http, &cfg.ml_client_id, &secret, &cfg.ml_redirect_uri, &code, &pending.verifier)
        .await
        .map_err(err)?;
    let refresh = token
        .refresh_token
        .as_deref()
        .ok_or("O Mercado Livre não liberou o acesso contínuo. Ative \"offline_access\" nas permissões do app no DevCenter")?;
    secrets::set(secrets::ML_REFRESH, refresh)?;
    {
        let mut s = state.ml.lock().await;
        s.access_token = Some(token.access_token.clone());
        s.expires_at = now_secs() + token.expires_in;
    }
    let nickname = ml::get(&state.http, &token.access_token, "/users/me", &[]).await.ok().and_then(|b| ml::parse_nickname(&b));
    cfg.ml_nickname = nickname.clone();
    save_config(app, &cfg)?;
    Ok(nickname)
}

#[tauri::command]
async fn ml_disconnect(app: AppHandle, state: State<'_, AppState>) -> CmdResult<MlStatus> {
    secrets::delete(secrets::ML_REFRESH)?;
    state.ml.lock().await.access_token = None;
    let mut cfg = load_config(&app);
    cfg.ml_nickname = None;
    save_config(&app, &cfg)?;
    ml_status(app)
}

/// Só em desenvolvimento: salva respostas reais em `src-tauri/tests/fixtures/ml` para os testes.
/// Busca e anúncios são dados públicos; `/users/me` (dados pessoais) nunca é salvo.
#[tauri::command]
async fn ml_dump_fixtures(app: AppHandle, state: State<'_, AppState>, query: String) -> CmdResult<String> {
    if !cfg!(debug_assertions) {
        return Err("Disponível só em desenvolvimento".into());
    }
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join("ml");
    let save = |name: &str, body: &str| fs::write(dir.join(name), body).map_err(err);
    let search = ml_get(&app, &state, &format!("/sites/{}/search", ml::SITE), &[("q", query.as_str()), ("limit", "5")]).await?;
    save("real-search.json", &search)?;
    let first = ml::parse_search(&search)?.into_iter().next().ok_or("A busca não trouxe resultados")?;
    let items = ml_get(&app, &state, "/items", &[("ids", first.code.as_str()), ("attributes", ml::ITEM_ATTRS)]).await?;
    save("real-items.json", &items)?;
    let sale = ml_get(&app, &state, &format!("/items/{}/sale_price", first.code), &[("context", "channel_marketplace")]).await?;
    save("real-sale-price.json", &sale)?;
    Ok(format!("Respostas salvas em {}", dir.display()))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let conn = db::open(&data_dir(app.handle())?.join(DB_FILE))?;
            let http = reqwest::Client::builder()
                .user_agent(concat!("RadarDePrecos/", env!("CARGO_PKG_VERSION")))
                .timeout(Duration::from_secs(20))
                .build()?;
            app.manage(AppState {
                db: Mutex::new(conn),
                http,
                ml: Default::default(),
                pending_login: Mutex::new(None),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ml_status,
            ml_save_settings,
            ml_connect,
            ml_disconnect,
            ml_dump_fixtures
        ])
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o aplicativo");
}
```

**Passo 2: compilar**

Rode: `cargo check --manifest-path src-tauri/Cargo.toml` e depois `cargo test --manifest-path src-tauri/Cargo.toml`
Esperado: compila sem erros e os testes das Tarefas 3 a 6 continuam PASS.

Se `on_navigation` não existir na versão instalada do Tauri, rode `cargo update -p tauri`. Ele existe no `WebviewWindowBuilder` do Tauri 2.

**Passo 3: `src/settings.js` (cartão do Mercado Livre)**
```js
// Ajustes: conta do Mercado Livre (o backup entra na Tarefa 12).
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getVersion } from '@tauri-apps/api/app';
import { esc, toast, setBusy, call } from './core.js';

let root;
let waitingLogin = false;
const $ = (s) => root.querySelector(s);

export const settingsModule = {
  render(el) {
    root = el;
    root.innerHTML = '<div class="page"><p class="muted">Carregando…</p></div>';
    let alive = true;
    let unlisten = null;
    listen('ml-login', onLogin).then((fn) => (alive ? (unlisten = fn) : fn()));
    load();
    return () => {
      alive = false;
      unlisten?.();
      waitingLogin = false;
    };
  },
};

async function load() {
  let data;
  try {
    data = await Promise.all([invoke('ml_status'), getVersion().catch(() => '')]);
  } catch (e) {
    toast(String(e), 'err');
    return;
  }
  if (root.isConnected) render(...data);
}

function onLogin({ payload }) {
  waitingLogin = false;
  if (payload.ok) toast(payload.nickname ? `Conectado como ${payload.nickname}` : 'Conectado ao Mercado Livre');
  else toast(payload.error, 'err');
  if (root?.isConnected) load();
}

function mlStatusText(ml) {
  if (ml.connected) return `<span class="ok-text">Conectado</span>${ml.nickname ? ` como ${esc(ml.nickname)}` : ''}`;
  return waitingLogin ? 'Aguardando o login na janela que abriu…' : 'Não conectado';
}

function mlActions(ml) {
  if (ml.connected) {
    return `<button id="mltest"><i class="ti ti-plug-connected"></i> Testar</button>
      <button class="ghost danger" id="mloff">Desconectar</button>
      ${import.meta.env.DEV ? '<button class="ghost" id="mldump" title="Salva respostas reais em src-tauri/tests/fixtures/ml">Salvar respostas para testes</button>' : ''}`;
  }
  if (waitingLogin) return '<button id="mlcancel">Cancelar</button>';
  return `<button class="primary" id="mlconnect" ${ml.hasSecret ? '' : 'disabled title="Salve a chave secreta primeiro"'}>
    <i class="ti ti-login"></i> Conectar</button>`;
}

function render(ml, version) {
  root.innerHTML = `
    <div class="page settings">
      <h2 class="pg-title">Ajustes</h2>

      <section class="card">
        <h3><i class="ti ti-shopping-bag"></i> Mercado Livre</h3>
        <div class="kv"><span>Situação</span><b>${mlStatusText(ml)}</b></div>
        <div class="actions">${mlActions(ml)}</div>
        <form id="mlf" class="ml-form" autocomplete="off">
          <label>Client ID <input id="mlid" value="${esc(ml.clientId)}"></label>
          <label>Chave secreta <input id="mlsecret" type="password"
            placeholder="${ml.hasSecret ? '•••••••• (guardada no Windows)' : 'Cole a Secret Key do DevCenter'}"></label>
          <label class="wide">URI de redirect <input id="mlredirect" value="${esc(ml.redirectUri)}"></label>
          <button class="small" id="mlsave">Salvar</button>
        </form>
        <p class="muted">Dados do app "Radar Ofertas JEV" no DevCenter do Mercado Livre. A chave secreta e o acesso ficam no
          Gerenciador de Credenciais do Windows, fora do banco e dos backups. A URI de redirect precisa ser idêntica à cadastrada.</p>
      </section>

      <div id="more"></div>
      ${version ? `<p class="muted small">Radar de Preços ${esc(version)}</p>` : ''}
    </div>`;

  $('#mlf').onsubmit = async (e) => {
    e.preventDefault();
    const btn = $('#mlsave');
    setBusy(btn, true);
    try {
      const next = await call('ml_save_settings', {
        clientId: $('#mlid').value,
        redirectUri: $('#mlredirect').value,
        clientSecret: $('#mlsecret').value || null,
      });
      toast('Mercado Livre salvo');
      render(next, version);
    } catch {
      setBusy(btn, false);
    }
  };
  $('#mlconnect') && ($('#mlconnect').onclick = async () => {
    try {
      await call('ml_connect');
      waitingLogin = true;
      render(ml, version);
    } catch { /* call já mostrou o erro */ }
  });
  $('#mlcancel') && ($('#mlcancel').onclick = () => { waitingLogin = false; render(ml, version); });
  $('#mloff') && ($('#mloff').onclick = async () => {
    if (!confirm('Desconectar a conta do Mercado Livre? Buscas e checagens param até conectar de novo.')) return;
    try {
      render(await call('ml_disconnect'), version);
      toast('Mercado Livre desconectado');
    } catch { /* call já mostrou o erro */ }
  });
  $('#mltest') && ($('#mltest').onclick = async (e) => {
    const btn = e.currentTarget;
    setBusy(btn, true, 'Testando…');
    try {
      const r = await call('search', { query: 'ssd' });
      toast(`Mercado Livre OK: ${r.length} ofertas para "ssd"`);
    } catch { /* call já mostrou o erro */ }
    setBusy(btn, false, 'Testar');
  });
  $('#mldump') && ($('#mldump').onclick = async () => {
    try { toast(await call('ml_dump_fixtures', { query: 'ssd 1tb' })); } catch { /* call já mostrou o erro */ }
  });
}
```

**Passo 4: CSS** (no fim de `src/style.css`):
```css
/* ---------- Radar de Preços ---------- */
.ok-text { color: var(--success); }
.warn-text { color: var(--warning); }
.small { font-size: 12px; }
.ml-form { display: grid; grid-template-columns: 1fr 1fr; gap: 10px 14px; margin: 14px 0 8px; align-items: end; }
.ml-form label { font-size: 12px; color: var(--text-3); display: flex; flex-direction: column; gap: 4px; }
.ml-form .wide { grid-column: 1 / -1; }
.ml-form button { justify-self: start; }
select {
  font: inherit; color: var(--text); width: 100%; height: 36px; padding: 0 8px;
  border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface-solid);
}
```

**Passo 5: verificação manual**

Rode: `npm run tauri dev` e abra **Ajustes**.
Esperado: Client ID `417769415941125` e a URI do gocomercio já preenchidos; situação "Não conectado"; botão Conectar desativado.

**Passo 6: commit**
```bash
git add -A
git commit -m "Conexão com o Mercado Livre e tela de Ajustes"
```

---

### Tarefa 8: PONTO DE DECISÃO — conexão real e fixtures reais

Esta tarefa é feita **com o usuário**. A chave secreta é digitada por ele no app e nunca passa pelo chat.

**Passo 1:** com `npm run tauri dev` aberto, peça ao usuário para:
1. Em Ajustes, colar a chave secreta (Secret Key do DevCenter) e clicar em **Salvar**.
2. Clicar em **Conectar**, entrar na conta do ML na janela que abre e autorizar o "Radar Ofertas JEV".

Esperado: a janela fecha sozinha, aparece o toast "Conectado como …" e a situação muda para **Conectado**.

Se der erro, o toast mostra a mensagem do ML. Erros comuns:
- `redirect_uri` diferente da cadastrada: conferir se a URI no app é idêntica à do DevCenter.
- "Ative offline_access": marcar a permissão no DevCenter e conectar de novo.
- Janela não fecha e carrega o site do gocomercio: o `on_navigation` não capturou. Depurar com um `eprintln!` da URL.

**Passo 2:** clicar em **Testar**.
- **"Mercado Livre OK: N ofertas"** → a busca funciona com o token do usuário. Seguir para o Passo 3.
- **"recusou a consulta (403)"** → **PARAR.** Anotar a resposta completa e conversar com o usuário antes da Tarefa 9.
  Alternativas a avaliar: `GET /products/search?site_id=MLB&q=…` (busca de catálogo) + `/products/{id}/items`, ou fazer a
  busca do ML pela janela escondida, como o desenho prevê para a Shopee.

**Passo 3:** clicar em **Salvar respostas para testes**.
Esperado: aparecem `real-search.json`, `real-items.json` e `real-sale-price.json` em `src-tauri/tests/fixtures/ml/`.

**Passo 4: comparar com as fixtures escritas à mão**

Abra os arquivos `real-*.json` e confira os campos que `ml.rs` lê: `results[].price`, `original_price`, `thumbnail`, `shipping`,
`seller.nickname`, o formato `[{code, body}]` do lote e `amount`/`regular_amount` do sale_price.
- Se algum campo tiver outro nome ou formato, ajuste `ml.rs` **e** a fixture escrita à mão para o formato real.
- Acrescente um teste que lê cada `real-*.json` e só exige que o parse dê certo e traga pelo menos um item com preço:
```rust
    #[test]
    fn le_respostas_reais() {
        assert!(!parse_search(include_str!("../tests/fixtures/ml/real-search.json")).unwrap().is_empty());
        assert!(parse_items(include_str!("../tests/fixtures/ml/real-items.json")).unwrap()[0].is_ok());
        assert!(parse_sale_price(include_str!("../tests/fixtures/ml/real-sale-price.json")).is_some());
    }
```

Rode: `cargo test --manifest-path src-tauri/Cargo.toml ml::`
Esperado: PASS.

**Passo 5: verificar o segredo**

No Windows, abra "Gerenciador de Credenciais" → "Credenciais do Windows". Devem existir duas entradas genéricas
`ml-client-secret` e `ml-refresh-token` do serviço `com.jeank.radar`. Rode `git status` e confirme que **nenhum** arquivo novo
contém a chave secreta (`git diff --cached` antes de commitar).

**Passo 6: commit**
```bash
git add src-tauri/tests/fixtures/ml/ src-tauri/src/ml.rs
git commit -m "Fixtures reais da API do Mercado Livre"
```

---

### Tarefa 9: Buscar e Acompanhar

**Arquivos:**
- Modificar: `src-tauri/src/lib.rs` (comandos `search`, `track`, `list_products`)
- Criar: `src/track.js`
- Modificar: `src/search.js`, `src/style.css`

**Passo 1: comandos no Rust** (acrescentar antes de `run()` e incluir no `generate_handler!`):
```rust
// ---------- comandos: busca e acompanhamento ----------

#[tauri::command]
async fn search(app: AppHandle, state: State<'_, AppState>, query: String) -> CmdResult<Vec<ml::Offer>> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let body = ml_get(&app, &state, &format!("/sites/{}/search", ml::SITE), &[("q", q), ("limit", "50")]).await?;
    ml::parse_search(&body)
}

/// Para onde vai o link acompanhado: um produto existente ou um novo
/// (sem nome, o produto novo usa o título do anúncio).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrackTarget {
    product_id: Option<i64>,
    new_name: Option<String>,
}

/// Cria o produto (se preciso), o link e o primeiro ponto do histórico numa transação só.
fn insert_tracked(state: &AppState, target: &TrackTarget, link: &db::NewLink, reading: &prices::Reading) -> CmdResult<i64> {
    let mut conn = lock(&state.db);
    let tx = conn.transaction().map_err(err)?;
    let now = now();
    let product_id = match target.product_id {
        Some(id) => id,
        None => {
            let name = target.new_name.as_deref().map(str::trim).filter(|s| !s.is_empty()).unwrap_or(link.title);
            db::add_product(&tx, name, &now).map_err(err)?
        }
    };
    let link_id = db::add_link(&tx, product_id, link)?;
    db::record_reading(&tx, link_id, reading, &now).map_err(err)?;
    tx.commit().map_err(err)?;
    Ok(product_id)
}

#[tauri::command]
async fn track(app: AppHandle, state: State<'_, AppState>, offer: ml::Offer, target: TrackTarget) -> CmdResult<i64> {
    let reading = prices::Reading { price: Some(offer.price), list_price: offer.list_price, in_stock: true, free_shipping: offer.free_shipping };
    let link = db::NewLink { store: &offer.store, code: &offer.code, url: &offer.url, title: &offer.title, image: offer.image.as_deref() };
    let id = insert_tracked(&state, &target, &link, &reading)?;
    after_change(&app, &state);
    Ok(id)
}

#[tauri::command]
fn list_products(state: State<'_, AppState>) -> CmdResult<Vec<db::ProductSummary>> {
    db::list_products(&lock(&state.db), &today()).map_err(err)
}

/// Chamado depois de cada mudança nos dados. Por enquanto não faz nada; a Tarefa 12 põe o backup aqui.
fn after_change(_app: &AppHandle, _state: &AppState) {}
```
No `generate_handler!`, acrescentar: `search, track, list_products`.

**Passo 2: `src/track.js`**
```js
// Diálogo "Acompanhar": criar um produto novo ou juntar o link a um produto existente.
import { call, esc, modal } from './core.js';

/** Resolve com { productId } ou { newName } (null = usar o título do anúncio), ou null se cancelar. */
export async function chooseProduct(suggestedName = '') {
  const products = await call('list_products');
  return new Promise((resolve) => {
    let done = false;
    const finish = (v) => { if (!done) { done = true; resolve(v); } };
    const { el, close } = modal(`
      <form id="tf" autocomplete="off">
        <h3>Acompanhar</h3>
        <label class="opt"><input type="radio" name="mode" value="new" checked> Novo produto</label>
        <input id="tname" value="${esc(suggestedName)}" maxlength="120" placeholder="Nome do produto (vazio = título do anúncio)">
        ${products.length ? `
          <label class="opt"><input type="radio" name="mode" value="join"> Juntar a um produto que já acompanho</label>
          <select id="tprod" disabled>${products.map((p) => `<option value="${p.id}">${esc(p.name)}</option>`).join('')}</select>` : ''}
        <div class="form-actions"><button type="button" id="tcancel">Cancelar</button><button class="primary">Acompanhar</button></div>
      </form>`, () => finish(null));
    const q = (s) => el.querySelector(s);
    const joining = () => q('input[name=mode]:checked').value === 'join';
    el.querySelectorAll('input[name=mode]').forEach((r) => (r.onchange = () => {
      q('#tname').disabled = joining();
      if (q('#tprod')) q('#tprod').disabled = !joining();
    }));
    q('#tcancel').onclick = close;
    q('#tf').onsubmit = (e) => {
      e.preventDefault();
      finish(joining() ? { productId: Number(q('#tprod').value) } : { newName: q('#tname').value.trim() || null });
      close();
    };
    q('#tname').select();
  });
}
```

**Passo 3: `src/search.js`**
```js
// Tela "Buscar": busca no Mercado Livre, ordenada por preço, com "Acompanhar" em cada oferta.
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
          <input id="q" placeholder="O que você procura? (ex.: SSD 1TB NVMe)" value="${esc(lastQuery)}" autofocus></div>
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
  $('#status').textContent = 'Buscando no Mercado Livre…';
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
  $('#status').textContent = lastQuery ? `${results.length} ofertas para "${lastQuery}", do menor preço para o maior` : '';
  $('#results').innerHTML = order.map((i) => card(results[i], i)).join('') || (lastQuery ? '<p class="muted">Nada encontrado.</p>' : '');
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
          ${o.seller ? `<span class="muted">${esc(o.seller)}</span>` : ''}
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
```

**Passo 4: CSS** (fim de `src/style.css`):
```css
.search-bar, .add-url { display: flex; gap: 8px; margin: 12px 0; }
.search-bar .search, .add-url input { flex: 1; }
.offers { display: grid; grid-template-columns: repeat(auto-fill, minmax(320px, 1fr)); gap: 12px; margin-top: 8px; }
.offer {
  display: grid; grid-template-columns: 72px minmax(0, 1fr); gap: 12px; padding: 12px;
  border: 1px solid var(--border); border-radius: var(--radius-lg); background: var(--glass);
}
.offer img, .prow img { width: 72px; height: 72px; object-fit: contain; border-radius: var(--radius); background: #fff; }
.offer h4 {
  margin: 4px 0; font-size: 13px; font-weight: 500;
  display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden;
}
.offer-actions { grid-column: 1 / -1; display: flex; gap: 6px; justify-content: flex-end; }
.price-line { display: flex; align-items: baseline; gap: 8px; }
.price-line b { font-size: 18px; }
.price-line s { color: var(--text-3); font-size: 12px; }
.off { color: var(--success); font-size: 12px; font-weight: 600; }
.tags { display: flex; gap: 6px; align-items: center; flex-wrap: wrap; margin-top: 4px; }
.tag.ok { background: rgba(52, 211, 153, 0.14); color: var(--success); }
.store-tag { display: inline-block; font-size: 11px; font-weight: 700; padding: 0 6px; border-radius: 4px; }
.store-ml { background: #ffe600; color: #2d3277; }
.store-amazon { background: #ff9900; color: #111; }
.store-shopee { background: #ee4d2d; color: #fff; }
.opt { display: flex; align-items: center; gap: 8px; margin: 12px 0 6px; font-size: 13px; }
```

**Passo 5: verificação manual**

`npm run tauri dev` → Buscar "ssd 1tb".
Esperado: cartões com foto, preço, "de" riscado com % quando houver, Frete grátis/Full, ordenados do mais barato.
O botão de abrir na loja leva ao navegador. Acompanhar → diálogo → leva para "Acompanhando", ainda um stub (normal até a Tarefa 10).
Repetir Acompanhar no mesmo anúncio → toast "Esse anúncio já está sendo acompanhado".

**Passo 6: commit**
```bash
git add -A
git commit -m "Tela Buscar e acompanhar ofertas"
```

---

### Tarefa 10: lista "Acompanhando", adicionar por link e "Checar agora"

**Arquivos:**
- Modificar: `src-tauri/src/lib.rs` (comandos `track_url`, `check_now`)
- Modificar: `src/products.js`, `src/style.css`

**Passo 1: comandos no Rust**
```rust
/// Um anúncio com o preço de venda real (promoções incluídas).
async fn fetch_item(app: &AppHandle, state: &AppState, code: &str) -> CmdResult<ml::ItemInfo> {
    let body = ml_get(app, state, "/items", &[("ids", code), ("attributes", ml::ITEM_ATTRS)]).await?;
    let mut info = ml::parse_items(&body)?.into_iter().next().ok_or("O Mercado Livre não devolveu o anúncio")??;
    apply_sale_price(app, state, &mut info).await;
    Ok(info)
}

/// Troca o preço da consulta em lote pelo preço de venda (com promoção), quando o ML informa.
/// Se essa consulta falhar, fica o preço do lote.
async fn apply_sale_price(app: &AppHandle, state: &AppState, info: &mut ml::ItemInfo) {
    if !info.reading.in_stock {
        return;
    }
    let path = format!("/items/{}/sale_price", info.code);
    if let Ok(body) = ml_get(app, state, &path, &[("context", "channel_marketplace")]).await {
        if let Some((price, list)) = ml::parse_sale_price(&body) {
            info.reading.list_price = list.or(info.reading.list_price.filter(|&l| l > price));
            info.reading.price = Some(price);
        }
    }
}

#[tauri::command]
async fn track_url(app: AppHandle, state: State<'_, AppState>, url: String, target: TrackTarget) -> CmdResult<i64> {
    let code = match ml::parse_link(&url).ok_or("Cole um link de anúncio do Mercado Livre")? {
        ml::LinkRef::Item(id) => id,
        ml::LinkRef::Catalog(id) => {
            let body = ml_get(&app, &state, &format!("/products/{id}"), &[]).await?;
            ml::parse_buy_box_winner(&body).ok_or("Esse produto do catálogo não tem um anúncio à venda agora")?
        }
    };
    let info = fetch_item(&app, &state, &code).await?;
    let link = db::NewLink { store: ml::STORE, code: &info.code, url: &info.url, title: &info.title, image: info.image.as_deref() };
    let id = insert_tracked(&state, &target, &link, &info.reading)?;
    after_change(&app, &state);
    Ok(id)
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct CheckSummary {
    checked: usize,
    changed: usize,
    failed: usize,
}

/// Consulta todos os anúncios acompanhados, em lotes de 20, e grava o que mudou.
#[tauri::command]
async fn check_now(app: AppHandle, state: State<'_, AppState>) -> CmdResult<CheckSummary> {
    let links = db::links_to_check(&lock(&state.db), ml::STORE).map_err(err)?;
    let mut sum = CheckSummary::default();
    for chunk in links.chunks(ml::BATCH) {
        let ids = chunk.iter().map(|(_, code)| code.as_str()).collect::<Vec<_>>().join(",");
        let batch = ml_get(&app, &state, "/items", &[("ids", ids.as_str()), ("attributes", ml::ITEM_ATTRS)])
            .await
            .and_then(|body| ml::parse_items(&body));
        let items = match batch {
            Ok(items) => items,
            Err(e) => {
                let conn = lock(&state.db);
                for (link_id, _) in chunk {
                    db::record_failure(&conn, *link_id, &e, &now()).map_err(err)?;
                }
                sum.failed += chunk.len();
                continue;
            }
        };
        let mut items = items.into_iter();
        for (link_id, _) in chunk {
            match items.next().unwrap_or_else(|| Err("O Mercado Livre não devolveu este anúncio".into())) {
                Ok(mut info) => {
                    apply_sale_price(&app, &state, &mut info).await;
                    let conn = lock(&state.db);
                    if db::record_reading(&conn, *link_id, &info.reading, &now()).map_err(err)? {
                        sum.changed += 1;
                    }
                    db::update_link_meta(&conn, *link_id, &info.title, info.image.as_deref()).map_err(err)?;
                    sum.checked += 1;
                }
                Err(e) => {
                    db::record_failure(&lock(&state.db), *link_id, &e, &now()).map_err(err)?;
                    sum.failed += 1;
                }
            }
        }
    }
    after_change(&app, &state);
    Ok(sum)
}
```
No `generate_handler!`, acrescentar: `track_url, check_now`.

Se o compilador reclamar que o future não é `Send` por causa de um `MutexGuard`, o guard está vivo durante um `.await`.
Mova o `lock(...)` para um bloco `{ }` que termine antes do próximo `.await`.

**Passo 2: `src/products.js` (lista; o detalhe entra na Tarefa 11)**
```js
// Tela "Acompanhando": lista de produtos, adicionar por link e checar agora. O detalhe fica em showDetail (Tarefa 11).
import { call, esc, toast, setBusy } from './core.js';
import { brl, pct, ago, STORES } from './format.js';
import { sparkline } from './charts.js';
import { chooseProduct } from './track.js';

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
};

const lastCheck = (products) => {
  const t = products.map((p) => p.lastCheck).filter(Boolean).sort().pop();
  return t ? ` · última checagem ${ago(t)}` : '';
};

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
        <input id="addurl" placeholder="Cole o link de um anúncio do Mercado Livre para acompanhar">
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
      </div>
      <div class="prow-meta">
        ${change ? `<span class="chg ${change < 0 ? 'down' : 'up'}">${change > 0 ? '+' : ''}${String(change).replace('.', ',')}%</span>` : '<span class="muted">—</span>'}
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
  setBusy(btn, true, 'Buscando anúncio…');
  try {
    const id = await call('track_url', { url, target });
    toast('Produto acompanhado');
    showDetail(id);
  } catch {
    if (root.isConnected) setBusy(btn, false, 'Acompanhar link');
  }
}

// Provisório: substituído na Tarefa 11.
async function showDetail(id) {
  openId = null;
  toast(`Detalhe do produto ${id} chega na próxima tarefa`);
  showList();
}
```

**Passo 3: CSS** (fim de `src/style.css`):
```css
.pg-head { display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; margin-bottom: 8px; }
.plist { display: flex; flex-direction: column; gap: 8px; }
.prow {
  display: grid; grid-template-columns: 48px minmax(0, 1fr) 130px 130px 120px; gap: 14px; align-items: center;
  padding: 10px 14px; border: 1px solid var(--border); border-radius: var(--radius-lg); background: var(--glass); cursor: pointer;
}
.prow:hover, .prow:focus-visible { border-color: rgba(167, 139, 250, 0.4); outline: none; }
.prow img { width: 48px; height: 48px; }
.prow-main, .prow-price, .prow-meta { display: flex; flex-direction: column; min-width: 0; }
.prow-main b { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.prow-price { text-align: right; }
.prow-price b { font-size: 16px; }
.prow-meta { align-items: flex-end; }
.chg { font-weight: 600; font-size: 13px; }
.chg.down { color: var(--success); }
.chg.up { color: var(--danger); }
.spark path { stroke: var(--accent-hover); }
.spark.down path { stroke: var(--success); }
.empty { text-align: center; padding: 48px 0; color: var(--text-2); }
.empty .ti { font-size: 40px; color: var(--accent-text); }
@media (max-width: 1000px) {
  .prow { grid-template-columns: 48px minmax(0, 1fr) 120px; }
  .prow-spark, .prow-meta { display: none; }
}
```

**Passo 4: verificação manual**
1. "Acompanhando" mostra o produto da Tarefa 9 com preço e loja.
2. Colar o link de um anúncio (`produto.mercadolivre.com.br/MLB-…`) → diálogo → produto novo na lista.
3. Colar um link de catálogo (`/p/MLB…`) → entra o anúncio que está vendendo.
4. Colar `https://www.amazon.com.br/dp/X` → toast "Cole um link de anúncio do Mercado Livre".
5. **Checar agora** → toast "N checados · 0 com mudança". A última checagem passa a "agora".

**Passo 5: commit**
```bash
git add -A
git commit -m "Lista Acompanhando, adicionar por link e Checar agora"
```

---

### Tarefa 11: detalhe do produto com histórico

**Arquivos:**
- Modificar: `src-tauri/src/lib.rs` (comandos `product_detail`, `rename_product`, `delete_product`, `delete_link`)
- Modificar: `src/products.js` (trocar o `showDetail` provisório), `src/style.css`

**Passo 1: comandos no Rust**
```rust
#[tauri::command]
fn product_detail(state: State<'_, AppState>, id: i64) -> CmdResult<db::ProductDetail> {
    db::product_detail(&lock(&state.db), id, &today()).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => "Produto não encontrado".to_string(),
        e => e.to_string(),
    })
}

#[tauri::command]
fn rename_product(app: AppHandle, state: State<'_, AppState>, id: i64, name: String) -> CmdResult<()> {
    if name.trim().is_empty() {
        return Err("O nome não pode ficar vazio".into());
    }
    db::rename_product(&lock(&state.db), id, &name).map_err(err)?;
    after_change(&app, &state);
    Ok(())
}

#[tauri::command]
fn delete_product(app: AppHandle, state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    db::delete_product(&lock(&state.db), id).map_err(err)?;
    after_change(&app, &state);
    Ok(())
}

#[tauri::command]
fn delete_link(app: AppHandle, state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    db::delete_link(&lock(&state.db), id).map_err(err)?;
    after_change(&app, &state);
    Ok(())
}
```
No `generate_handler!`, acrescentar: `product_detail, rename_product, delete_product, delete_link`.

**Passo 2: `showDetail` em `src/products.js`**

Trocar os imports do topo por:
```js
import { openUrl } from '@tauri-apps/plugin-opener';
import { call, esc, toast, setBusy } from './core.js';
import { brl, pct, ago, STORES } from './format.js';
import { sparkline, stepChart } from './charts.js';
import { chooseProduct } from './track.js';
```
Substituir o `showDetail` provisório por:
```js
const SERIES_COLORS = ['#a78bfa', '#60a5fa', '#34d399', '#fbbf24', '#f472b6'];

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
            ${change ? ` · <span class="chg ${change < 0 ? 'down' : 'up'}">${change > 0 ? '+' : ''}${String(change).replace('.', ',')}% desde que começou</span>` : ''}</p>
        </div>
        <button class="primary" id="check"><i class="ti ti-refresh"></i> Checar agora</button>
      </div>
      <section class="card">${stepChart(series)}</section>
      <section class="card">
        <h3>Links</h3>
        <table class="links">
          <thead><tr><th>Loja</th><th>Anúncio</th><th>Preço</th><th>Checado</th><th></th></tr></thead>
          <tbody>${p.links.map(linkRow).join('')}</tbody>
        </table>
      </section>
      <div class="danger-zone"><button class="ghost danger" id="del"><i class="ti ti-trash"></i> Parar de acompanhar</button></div>
    </div>`;

  $('#back').onclick = () => showList();
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
    : !c.inStock ? '<span class="muted">sem estoque</span>'
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
```
Em `showList`, na linha `root.querySelectorAll('.prow')…`, nada muda: ela já chama `showDetail`.

**Passo 3: CSS** (fim de `src/style.css`):
```css
.title-input {
  font-size: 20px; font-weight: 600; background: transparent; border-color: transparent;
  padding: 0 6px; margin-left: -6px; height: 38px;
}
.title-input:hover { border-color: var(--border-strong); }
.price-chart { position: relative; padding-left: 84px; }
.price-chart svg { width: 100%; display: block; overflow: visible; }
.chart-ylabels { position: absolute; left: 0; top: 0; width: 80px; }
.chart-ylabels span { position: absolute; right: 6px; transform: translateY(-50%); font-size: 11px; color: var(--text-3); }
.chart-xlabels { display: flex; justify-content: space-between; font-size: 11px; color: var(--text-3); margin-top: 4px; }
.legend { display: flex; flex-wrap: wrap; gap: 12px; margin-top: 10px; font-size: 12px; color: var(--text-2); }
.legend i { display: inline-block; width: 10px; height: 10px; border-radius: 3px; margin-right: 6px; vertical-align: -1px; }
table.links { width: 100%; border-collapse: collapse; font-size: 13px; }
table.links th { text-align: left; color: var(--text-3); font-weight: 500; padding: 6px 8px; border-bottom: 1px solid var(--border); }
table.links td { padding: 8px; border-bottom: 1px solid var(--border); vertical-align: top; }
.ltitle { max-width: 380px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.row-actions { white-space: nowrap; text-align: right; }
.danger-zone { margin-top: 16px; }
```

**Passo 4: verificação manual**
1. Clicar num produto → gráfico (um ponto vira uma linha reta até "agora"), tabela de links, nome editável.
2. Renomear e apertar Enter → toast "Nome salvo"; voltar à lista e conferir.
3. Juntar um segundo anúncio ao mesmo produto (Buscar → Acompanhar → "Juntar a um produto") → duas linhas no gráfico, uma cor por link.
4. Remover um link → some da tabela. Remover o último → volta para a lista sem o produto.

**Passo 5: commit**
```bash
git add -A
git commit -m "Detalhe do produto com histórico de preços"
```

---

### Tarefa 12: backup automático e restauração

**Arquivos:**
- Criar: `src-tauri/src/backup.rs` (adaptado do Cofre)
- Modificar: `src-tauri/src/lib.rs` (`mod backup;`, `after_change`, 4 comandos)
- Modificar: `src/settings.js` (cartões de backup), `src/main.js` (nada), `src/style.css` (nada)

**Passo 1: `src-tauri/src/backup.rs`**

Partir de uma cópia do `backup.rs` do Cofre e mudar:
- Comentário do topo: `//! Backup do banco. Uma cópia por dia (`radar-AAAA-MM-DD.db`), sobrescrita a cada mudança daquele dia, mantendo as `KEEP` mais recentes.`
- `PREFIX = "radar-"`, `EXT = ".db"`.
- `run` recebe a conexão em vez do caminho e usa `VACUUM INTO`, que faz uma cópia consistente mesmo com o banco aberto:
```rust
pub fn run(db: &rusqlite::Connection, dir: &Path, date: &str, keep: usize) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| format!("Não foi possível acessar a pasta de backup: {e}"))?;
    let target = dir.join(format!("{PREFIX}{date}{EXT}"));
    let tmp = dir.join(format!("{PREFIX}{date}.tmp"));
    let _ = fs::remove_file(&tmp); // VACUUM INTO não sobrescreve
    db.execute("VACUUM INTO ?1", [tmp.to_string_lossy().into_owned()])
        .map_err(|e| format!("Não foi possível gravar o backup: {e}"))?;
    fs::rename(&tmp, &target).map_err(|e| format!("Não foi possível gravar o backup: {e}"))?;

    for old in list(dir).map_err(|e| e.to_string())?.into_iter().skip(keep) {
        let _ = fs::remove_file(old);
    }
    Ok(())
}
```
- `status`: sugestão `...join("Backups").join("Radar de Precos")`.
- `check_writable`: arquivo de teste `.radar-teste`.
- Testes: nomes `radar-2026-10-06.db` no lugar de `cofre-…dat`. O teste de rotação passa a usar um banco em memória:
```rust
    #[test]
    fn um_arquivo_por_dia_e_mantem_os_mais_recentes() {
        let base = temp_dir("rotacao");
        let dir = base.join("backups");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("outro-arquivo.txt"), b"nao mexer").unwrap();
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE t (x); INSERT INTO t VALUES (1);").unwrap();

        for day in 1..=5 {
            run(&db, &dir, &format!("2026-10-0{day}"), 3).unwrap();
        }
        db.execute("INSERT INTO t VALUES (2)", []).unwrap();
        run(&db, &dir, "2026-10-05", 3).unwrap(); // mesmo dia: sobrescreve

        let names: Vec<String> = list(&dir).unwrap().iter().map(|p| p.file_name().unwrap().to_string_lossy().into()).collect();
        assert_eq!(names, ["radar-2026-10-05.db", "radar-2026-10-04.db", "radar-2026-10-03.db"]);
        let copy = rusqlite::Connection::open(dir.join("radar-2026-10-05.db")).unwrap();
        let n: i64 = copy.query_row("SELECT count(*) FROM t", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 2, "o backup do dia tem a versão mais recente");
        assert!(dir.join("outro-arquivo.txt").exists(), "não pode apagar arquivos que não são backup");
        assert_eq!(status(Some(dir.to_str().unwrap())).last.as_deref(), Some("2026-10-05"));
    }
```
  O teste `reconhece_somente_nomes_de_backup` muda para `radar-2026-10-06.db` (válido) e `radar-2026-10-6.db`, `radar.db`,
  `radar-antes-restauracao.db`, `foto-2026-10-06.db` (inválidos).

Rode: `cargo test --manifest-path src-tauri/Cargo.toml backup`
Esperado: 2 testes PASS.

**Passo 2: comandos e backup automático em `lib.rs`**

Acrescentar `mod backup;` e trocar o `after_change` vazio por:
```rust
/// Depois de cada mudança nos dados: backup do dia na pasta escolhida.
/// Falhas não impedem a operação; aparecem no status do backup em Ajustes.
fn after_change(app: &AppHandle, state: &AppState) {
    if let Some(dir) = load_config(app).backup_dir {
        let _ = backup::run(&lock(&state.db), Path::new(&dir), &today(), backup::KEEP);
    }
}

// ---------- comandos: backup ----------

#[tauri::command]
fn backup_status(app: AppHandle) -> backup::BackupStatus {
    backup::status(load_config(&app).backup_dir.as_deref())
}

#[tauri::command]
fn set_backup_dir(app: AppHandle, state: State<'_, AppState>, dir: Option<String>) -> CmdResult<backup::BackupStatus> {
    if let Some(d) = &dir {
        backup::check_writable(Path::new(d))?;
    }
    let mut cfg = load_config(&app);
    cfg.backup_dir = dir.clone();
    save_config(&app, &cfg)?;
    if let Some(d) = &dir {
        backup::run(&lock(&state.db), Path::new(d), &today(), backup::KEEP)?;
    }
    Ok(backup::status(dir.as_deref()))
}

#[tauri::command]
fn backup_now(app: AppHandle, state: State<'_, AppState>) -> CmdResult<backup::BackupStatus> {
    let dir = load_config(&app).backup_dir.ok_or("Escolha uma pasta de backup primeiro")?;
    backup::run(&lock(&state.db), Path::new(&dir), &today(), backup::KEEP)?;
    Ok(backup::status(Some(&dir)))
}

/// Troca o banco atual por um backup. O atual é guardado ao lado, renomeado.
#[tauri::command]
fn restore_backup(app: AppHandle, state: State<'_, AppState>, path: String) -> CmdResult<()> {
    let source = PathBuf::from(&path);
    db::check_backup(&source)?;
    let target = data_dir(&app)?.join(DB_FILE);
    let keep = target.with_file_name(format!("radar-antes-restauracao-{}.db", chrono::Local::now().format("%Y%m%d-%H%M%S")));
    let mut conn = lock(&state.db);
    conn.execute("VACUUM INTO ?1", [keep.to_string_lossy().into_owned()])
        .map_err(|e| format!("Não foi possível guardar o banco atual: {e}"))?;
    *conn = Connection::open_in_memory().map_err(err)?; // fecha o arquivo para poder substituir
    let copied = fs::copy(&source, &target);
    *conn = db::open(&target).map_err(err)?; // reabre mesmo se a cópia falhou
    copied.map_err(|e| format!("Não foi possível restaurar: {e}"))?;
    Ok(())
}
```
No `generate_handler!`, acrescentar: `backup_status, set_backup_dir, backup_now, restore_backup`.

**Passo 3: cartões de backup em `src/settings.js`**

Acrescentar `import { open } from '@tauri-apps/plugin-dialog';`. Em `load`, buscar também o status:
`data = await Promise.all([invoke('ml_status'), invoke('backup_status'), getVersion().catch(() => '')]);` e mudar a assinatura para
`render(ml, st, version)`. Atualize as chamadas internas de `render(next, version)` para `render(next, st, version)`.

No HTML, trocar `<div id="more"></div>` por:
```js
      <section class="card">
        <h3><i class="ti ti-cloud-upload"></i> Backup automático</h3>
        ${st.dir ? `
          <div class="kv"><span>Pasta</span><b class="selectable">${esc(st.dir)}</b></div>
          <div class="kv"><span>Último backup</span><b>${st.last ? st.last.split('-').reverse().join('/') : 'nenhum ainda'}</b></div>
          <div class="kv"><span>Cópias guardadas</span><b>${st.count} (uma por dia, até 30)</b></div>
          ${st.error ? `<p class="error">${esc(st.error)}</p>` : ''}` : `
          <div class="alert"><i class="ti ti-alert-triangle"></i>
            <div><b>Backup desativado.</b> Produtos e histórico de preços ficam só neste computador.</div></div>
          <p class="muted">Escolha uma pasta em outro lugar, como OneDrive ou Google Drive. O backup não inclui a chave do Mercado Livre.</p>`}
        <div class="actions">
          ${st.suggestion && st.dir !== st.suggestion ? `<button class="${st.dir ? '' : 'primary'}" id="bod"><i class="ti ti-brand-onedrive"></i> Usar o OneDrive</button>` : ''}
          <button id="bpick"><i class="ti ti-folder"></i> ${st.dir ? 'Trocar pasta…' : 'Escolher pasta…'}</button>
          ${st.dir ? `<button id="bnow"><i class="ti ti-refresh"></i> Fazer backup agora</button>
                      <button class="ghost danger" id="boff">Desativar</button>` : ''}
        </div>
        ${st.suggestion && st.dir !== st.suggestion ? `<p class="muted small">OneDrive: ${esc(st.suggestion)}</p>` : ''}
      </section>

      <section class="card">
        <h3><i class="ti ti-restore"></i> Restaurar um backup</h3>
        <p class="muted">Substitui produtos e histórico pelo conteúdo de um backup. Os dados atuais não são apagados: ficam guardados ao lado, renomeados.</p>
        <div class="actions"><button id="brestore"><i class="ti ti-file-upload"></i> Escolher backup…</button></div>
      </section>
```
E os handlers, no fim de `render`:
```js
  const applyBackup = async (btn, fn, okMsg) => {
    setBusy(btn, true);
    try {
      const next = await fn();
      render(ml, next, version);
      toast(okMsg);
    } catch (e) {
      setBusy(btn, false);
      toast(String(e), 'err');
    }
  };
  $('#bod') && ($('#bod').onclick = (e) => applyBackup(e.currentTarget, () => invoke('set_backup_dir', { dir: st.suggestion }), 'Backup ativado no OneDrive'));
  $('#bpick').onclick = async (e) => {
    const btn = e.currentTarget;
    const dir = await open({ directory: true, title: 'Pasta para os backups do Radar' });
    if (dir) applyBackup(btn, () => invoke('set_backup_dir', { dir }), 'Backup ativado');
  };
  $('#bnow') && ($('#bnow').onclick = (e) => applyBackup(e.currentTarget, () => invoke('backup_now'), 'Backup feito'));
  $('#boff') && ($('#boff').onclick = (e) => {
    if (!confirm('Desativar o backup automático? Os backups já feitos continuam na pasta.')) return;
    applyBackup(e.currentTarget, () => invoke('set_backup_dir', { dir: null }), 'Backup desativado');
  });
  $('#brestore').onclick = async () => {
    const path = await open({ title: 'Escolha o backup do Radar', filters: [{ name: 'Backup do Radar', extensions: ['db'] }] });
    if (!path) return;
    if (!confirm('Restaurar este backup? Produtos e histórico atuais serão substituídos (uma cópia fica guardada).')) return;
    try {
      await invoke('restore_backup', { path });
      toast('Backup restaurado');
      load();
    } catch (e) {
      toast(String(e), 'err');
    }
  };
```

**Passo 4: verificação manual**
1. Ajustes → Usar o OneDrive (ou Escolher pasta…) → aparece `radar-AAAA-MM-DD.db` na pasta.
2. Acompanhar um produto novo → o arquivo do dia é atualizado (confira a data de modificação).
3. Restaurar esse backup → toast; os produtos continuam lá; aparece `radar-antes-restauracao-….db` em `%APPDATA%\com.jeank.radar`.
4. Escolher um `.db` qualquer que não seja do Radar → "Esse arquivo não é um backup do Radar de Preços".

**Passo 5: commit**
```bash
git add -A
git commit -m "Backup automático e restauração"
```

---

### Tarefa 13: backend simulado, README e versão 0.1.0

**Arquivos:**
- Modificar: `src/dev-mock.js`
- Criar: `README.md`
- Modificar: `.claude/launch.json` (já copiado: confere se aponta `npm run dev` na 1420)

**Passo 1: `src/dev-mock.js` completo**
```js
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

const offers = [
  { store: 'ml', code: 'MLB3456789012', title: 'SSD Kingston NV2 1TB M.2 2280 NVMe PCIe 4.0', url: 'https://www.mercadolivre.com.br', image: '', price: 38990, listPrice: 49990, freeShipping: true, full: true, seller: 'KINGSTON OFICIAL' },
  { store: 'ml', code: 'MLB2222222222', title: 'SSD Sandisk Plus 1TB SATA', url: 'https://www.mercadolivre.com.br', image: '', price: 35900, listPrice: null, freeShipping: false, full: false, seller: null },
  { store: 'ml', code: 'MLB4444444444', title: 'SSD WD Green SN350 1TB NVMe', url: 'https://www.mercadolivre.com.br', image: '', price: 41900, listPrice: 45900, freeShipping: true, full: false, seller: 'WD STORE' },
];

const history = {
  1: [[at(20), R(45990)], [at(12), R(42990)], [at(5), R(41990)], [at(1), R(38990, { listPrice: 49990 })]],
  2: [[at(18), R(43900)], [at(3), R(40900)], [at(0), R(40900, { inStock: false })]],
};
const link = (id, code, title) => ({ id, store: 'ml', code, title, url: 'https://www.mercadolivre.com.br', image: '', lastCheck: at(0, new Date().getHours()), lastError: null, failures: 0 });
let products = [{ id: 1, name: 'SSD Kingston NV2 1TB', links: [link(1, 'MLB3456789012', 'SSD Kingston NV2 1TB M.2 NVMe'), link(2, 'MLB5555555555', 'Kingston NV2 1TB (outro vendedor)')] }];

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
    id: p.id, name: p.name, targetPrice: null,
    links: p.links.map((l, i) => ({ ...l, current: hist[i].at(-1)?.[1] ?? null })),
    bestPrice: best, bestStore: best == null ? null : p.links[cur.indexOf(best)].store,
    lowestEver: all.length ? Math.min(...all) : null,
    firstPrice: first ? bestOn(first.slice(0, 10)) : null,
    spark: Array.from({ length: 30 }, (_, i) => bestOn(at(29 - i).slice(0, 10))),
    lastCheck: p.links[0]?.lastCheck ?? null,
  };
}

let ml = { clientId: '417769415941125', redirectUri: 'https://gocomercio.com.br/oauth/mercadolivre/callback', hasSecret: true, connected: true, nickname: 'JEANK' };
let backup = { dir: null, last: null, count: 0, error: null, suggestion: 'C:\\Users\\voce\\OneDrive\\Backups\\Radar de Precos' };
const later = (v, ms = 500) => new Promise((r) => setTimeout(() => r(v), ms));

mockIPC((cmd, args) => {
  switch (cmd) {
    case 'search': return later(offers.filter((o) => o.title.toLowerCase().includes(args.query.toLowerCase().split(' ')[0])));
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
    case 'check_now': return later({ checked: 2, changed: 1, failed: 0 }, 800);
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
```

**Passo 2: verificar no navegador**

Rode `npm run dev`, abra http://localhost:1420 e passe pelas três telas.
Esperado: busca "ssd" com 3 cartões, lista com 1 produto e mini gráfico, detalhe com 2 linhas (a segunda some no fim, sem estoque),
Ajustes conectado como JEANK.

**Passo 3: `README.md`**

Seguir a estrutura do README do Cofre, com estas seções:
- **Radar de Preços**: uma frase sobre o que é + link para o desenho em `docs/plans/`.
- **Telas**: tabela Buscar (`Ctrl+1`), Acompanhando (`Ctrl+2`), Ajustes.
- **Conectar o Mercado Livre**: o app do DevCenter ("Radar Ofertas JEV"), onde colar a chave secreta, o botão Conectar,
  a URI de redirect que precisa ser idêntica à do DevCenter, e onde ficam os segredos (Gerenciador de Credenciais do Windows,
  serviço `com.jeank.radar`).
- **Onde ficam os dados**: `%APPDATA%\com.jeank.radar\radar.db` e `config.json`.
- **Backup**: igual ao do Cofre, com `radar-AAAA-MM-DD.db`.
- **Desenvolvimento**: `npm install`, `npm run tauri dev`, `npm run dev` com o backend simulado, `npm test`, e
  "Salvar respostas para testes" para atualizar as fixtures.
- **Gerar o instalador**: `npm run tauri build` → `src-tauri/target/release/bundle/nsis/`.
- **Estrutura**: lista dos arquivos de `src/` e `src-tauri/src/` com uma linha cada.

**Passo 4: testes completos e instalador**

Rode: `npm test`
Esperado: todos os testes de JS e Rust PASS.

Rode: `npm run tauri build`
Esperado: instalador em `src-tauri/target/release/bundle/nsis/`. Instale, abra, conecte o ML de novo (o app instalado usa a
mesma pasta de dados e as mesmas credenciais do Windows) e faça uma busca.

**Passo 5: commit e push**
```bash
git add -A
git commit -m "Primeira versão: busca e acompanhamento no Mercado Livre (0.1.0)"
git push
```

---

## Verificação final da etapa

- [ ] `npm test` passa (JS + Rust), incluindo `le_respostas_reais`.
- [ ] Conectar, Testar e Desconectar funcionam; a chave secreta não aparece em nenhum arquivo do repositório (`git grep -i secret` só acha nomes de variáveis).
- [ ] Buscar → Acompanhar → Acompanhando → Checar agora → detalhe com gráfico.
- [ ] Adicionar por link de anúncio e de catálogo.
- [ ] Backup diário na pasta escolhida e restauração.
- [ ] Instalador gerado e testado.

Próxima etapa (plano separado): checagem automática em segundo plano, bandeja, iniciar com o Windows, notificações e regras de alerta.
