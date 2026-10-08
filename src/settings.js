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
