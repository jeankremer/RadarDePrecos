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
