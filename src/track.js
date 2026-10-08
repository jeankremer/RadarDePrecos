// Diálogo "Acompanhar": criar um produto novo ou juntar o link a um produto existente.
import { call, esc, modal } from './core.js';

/** Resolve com { productId } ou { newName } (null = usar o nome do catálogo), ou null se cancelar. */
export async function chooseProduct(suggestedName = '') {
  const products = await call('list_products');
  return new Promise((resolve) => {
    let done = false;
    const finish = (v) => { if (!done) { done = true; resolve(v); } };
    const { el, close } = modal(`
      <form id="tf" autocomplete="off">
        <h3>Acompanhar</h3>
        <label class="opt"><input type="radio" name="mode" value="new" checked> Novo produto</label>
        <input id="tname" value="${esc(suggestedName)}" maxlength="120" placeholder="Nome do produto (vazio = nome do catálogo)">
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
