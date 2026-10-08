// Ajustes: conta do Mercado Livre, checagem automática, backup e restauração.
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getVersion } from '@tauri-apps/api/app';
import { open } from '@tauri-apps/plugin-dialog';
import { esc, toast, setBusy, call } from './core.js';
import { ago } from './format.js';

const INTERVALS = [[0, 'Desligada'], [1, 'A cada 1 hora'], [3, 'A cada 3 horas'], [6, 'A cada 6 horas'], [12, 'A cada 12 horas'], [24, 'Uma vez por dia']];

let root;
let waitingLogin = false;
const $ = (s) => root.querySelector(s);

export const settingsModule = {
  /** Atualiza "última automática" quando uma checagem termina, sem atrapalhar quem está digitando. */
  refresh() {
    if (root?.contains(document.activeElement) && document.activeElement.matches('input, select')) return;
    load();
  },
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
    data = await Promise.all([invoke('ml_status'), invoke('check_settings'), invoke('backup_status'), getVersion().catch(() => '')]);
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
      ${import.meta.env.DEV ? '<button class="ghost" id="mldump" title="Testa vários endereços da API e salva as respostas em src-tauri/tests/fixtures/ml">Diagnóstico da API</button>' : ''}`;
  }
  if (waitingLogin) return '<button id="mlcancel">Cancelar</button>';
  return `<button class="primary" id="mlconnect" ${ml.hasSecret ? '' : 'disabled title="Salve a chave secreta primeiro"'}>
    <i class="ti ti-login"></i> Conectar</button>`;
}

function render(ml, cs, st, version) {
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

      <section class="card">
        <h3><i class="ti ti-clock-play"></i> Checagem automática</h3>
        <form id="csf" class="check-form">
          <label>Checar os preços <select id="csint">${INTERVALS.map(([h, t]) => `<option value="${h}" ${h === cs.intervalHours ? 'selected' : ''}>${t}</option>`).join('')}</select></label>
          <label><input type="checkbox" id="csnotify" ${cs.notify ? 'checked' : ''}> Mostrar notificação do Windows</label>
          <label><input type="checkbox" id="csauto" ${cs.autostart ? 'checked' : ''}> Iniciar com o Windows (perto do relógio)</label>
        </form>
        <div class="kv"><span>Última automática</span><b>${cs.lastAutoCheck ? ago(cs.lastAutoCheck) : 'nenhuma ainda'}${cs.checking ? ' · checando agora…' : ''}</b></div>
        <div class="actions"><button id="cstest"><i class="ti ti-bell-ringing"></i> Testar notificação</button></div>
        <p class="muted">Fechar a janela não encerra o Radar: ele continua checando perto do relógio. Para fechar de vez,
          clique com o botão direito no ícone e escolha Sair. A checagem só acontece com o computador ligado.</p>
      </section>

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
      render(next, cs, st, version);
    } catch {
      setBusy(btn, false);
    }
  };
  $('#mlconnect') && ($('#mlconnect').onclick = async () => {
    try {
      await call('ml_connect');
      waitingLogin = true;
      render(ml, cs, st, version);
    } catch { /* call já mostrou o erro */ }
  });
  $('#mlcancel') && ($('#mlcancel').onclick = () => { waitingLogin = false; render(ml, cs, st, version); });
  $('#mloff') && ($('#mloff').onclick = async () => {
    if (!confirm('Desconectar a conta do Mercado Livre? Buscas e checagens param até conectar de novo.')) return;
    try {
      render(await call('ml_disconnect'), cs, st, version);
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

  // ---- checagem automática ----
  $('#csf').onchange = async () => {
    try {
      const next = await call('set_check_settings', {
        intervalHours: Number($('#csint').value),
        notify: $('#csnotify').checked,
        autostart: $('#csauto').checked,
      });
      render(ml, next, st, version);
      toast('Checagem automática salva');
    } catch {
      render(ml, cs, st, version);
    }
  };
  $('#cstest').onclick = async () => {
    try {
      await call('test_notification');
      toast('Notificação enviada. Se não apareceu, confira as notificações do Windows');
    } catch { /* call já mostrou o erro */ }
  };

  // ---- backup ----
  const applyBackup = async (btn, fn, okMsg) => {
    setBusy(btn, true);
    try {
      const next = await fn();
      render(ml, cs, next, version);
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
}
