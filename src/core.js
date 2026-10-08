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
