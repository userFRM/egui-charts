import { areas, counts, features, percentage, treemap, type Feature } from './progress';

const background = 'var(--progress-background, #ffffff)';
const colours = { verified: '#2ea043', unverified: '#388bfd', incomplete: '#d29922', missing: '#6e7681' };
const order = { verified: 0, unverified: 1, incomplete: 2, missing: 3 };
const columns = 3, gap = 5, width = 1000, panelHeight = 300;
const panelWidth = (width - gap * (columns - 1)) / columns;
const escape = (value: string) => value.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;').replaceAll('"', '&quot;');
const text = (x: number, y: number, size: number, value: string) =>
  `<text x="${x.toFixed(2)}" y="${y.toFixed(2)}" font-family="sans-serif" font-size="${size}" fill="var(--progress-text, #1f2328)" stroke="${background}" stroke-width="3" paint-order="stroke">${escape(value)}</text>`;

/** One panel per area: a block per group with one cell per feature, cells ordered as verified, unverified, incomplete, then missing. */
function panel(area: string, x: number, y: number) {
  const rows = features.filter((row) => row.area === area);
  const sorted = [...rows].sort((a, b) => order[a.status] - order[b.status]);
  const { verified, total } = counts(rows);
  const groups = treemap(sorted, panelWidth - 2, panelHeight - 32).map((group) => {
    const ox = x + 1, oy = y + 31;
    const cells = group.features.map((feature: Feature, index) => {
      const cell = group.cells[index];
      return `<rect x="${(ox + cell.x).toFixed(2)}" y="${(oy + cell.y).toFixed(2)}" width="${cell.width.toFixed(2)}" height="${cell.height.toFixed(2)}" fill="${colours[feature.status]}" stroke="${background}" stroke-width="0.5"/>`;
    }).join('\n');
    const groupVerified = counts(group.features).verified;
    const capacity = Math.max(0, Math.floor((group.box.width - 10) / 8));
    const shortened = group.name.slice(0, Math.max(0, capacity - 1));
    const label = group.name.length <= capacity ? group.name :
      shortened.includes(' ') ? `${shortened.slice(0, shortened.lastIndexOf(' '))}…` : '';
    return `<g><title>${escape(group.name)}: ${groupVerified}/${group.features.length} verified (${Math.round(groupVerified / group.features.length * 100)}%)</title>\n${cells}` +
      (label && group.box.height >= 18 ? `\n${text(ox + group.box.x + 4, oy + group.box.y + 15, 12, label)}` : '') + '\n</g>';
  });
  return [text(x + 4, y + 21, 16, `${area}: ${percentage(rows)}% (${verified}/${total})`), ...groups].join('\n');
}

export function progressSvg() {
  const height = Math.ceil(areas.length / columns) * (panelHeight + gap) - gap;
  const panels = areas.map((area, index) =>
    panel(area, (index % columns) * (panelWidth + gap), Math.floor(index / columns) * (panelHeight + gap)));
  return `<svg xmlns="http://www.w3.org/2000/svg" class="progress-map" viewBox="0 0 ${width} ${height}" width="${width}" height="${height}" role="img" aria-label="Feature map grouped by area: green is verified, blue is unverified, yellow is incomplete and grey is missing. Percentages count verified features.">
<style>
@media(prefers-color-scheme:dark){.progress-map{--progress-background:#0d1117;--progress-text:#ffffff}}
[data-theme='light'] .progress-map{--progress-background:#ffffff;--progress-text:#1f2328}
[data-theme='dark'] .progress-map{--progress-background:#0d1117;--progress-text:#ffffff}
</style>
<rect x="0" y="0" width="${width}" height="${height}" fill="${background}"/>
${panels.join('\n')}
</svg>`;
}

export function badgeSvg(label: string, rows: Feature[]) {
  const share = Number(percentage(rows)), value = `${percentage(rows)}%`;
  const colour = share >= 100 ? '#4c1' : share >= 80 ? '#97ca00' : share >= 60 ? '#a4a61d' : share >= 40 ? '#dfb317' : share >= 20 ? '#fe7d37' : '#e05d44';
  const left = label.length * 7 + 10, right = value.length * 7 + 10;
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${left + right}" height="20" role="img" aria-label="${escape(label)}: ${value}"><rect width="${left}" height="20" fill="#555"/><rect x="${left}" width="${right}" height="20" fill="${colour}"/><g fill="#fff" font-family="Verdana,DejaVu Sans,sans-serif" font-size="11" text-anchor="middle"><text x="${left / 2}" y="14">${escape(label)}</text><text x="${left + right / 2}" y="14">${value}</text></g></svg>`;
}
