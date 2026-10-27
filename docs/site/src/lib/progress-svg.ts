import { areas, counts, features, percentage, treemap, type Feature } from './progress';

const background = '#0d1117';
const columns = 3, gap = 5, width = 1000, panelHeight = 300;
const panelWidth = (width - gap * (columns - 1)) / columns;
const escape = (value: string) => value.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;').replaceAll('"', '&quot;');
const text = (x: number, y: number, size: number, value: string) =>
  `<text x="${x.toFixed(2)}" y="${y.toFixed(2)}" font-family="sans-serif" font-size="${size}" fill="#ffffff" stroke="${background}" stroke-width="3" paint-order="stroke">${escape(value)}</text>`;

/** One panel per area: a block per group with one cell per feature, done cells first. */
function panel(area: string, x: number, y: number) {
  const rows = features.filter((row) => row.area === area);
  const sorted = [...rows].sort((a, b) => Number(b.status === 'done') - Number(a.status === 'done'));
  const { done, total } = counts(rows);
  const groups = treemap(sorted, panelWidth - 2, panelHeight - 32).map((group) => {
    const ox = x + 1, oy = y + 31;
    const cells = group.features.map((feature: Feature, index) => {
      const cell = group.cells[index];
      return `<rect x="${(ox + cell.x).toFixed(2)}" y="${(oy + cell.y).toFixed(2)}" width="${cell.width.toFixed(2)}" height="${cell.height.toFixed(2)}" fill="${feature.status === 'done' ? '#2ea043' : '#6e7681'}" stroke="${background}" stroke-width="0.5"/>`;
    }).join('\n');
    const groupDone = counts(group.features).done;
    const label = group.name.slice(0, Math.floor((group.box.width - 6) / 7));
    return `<g><title>${escape(group.name)}: ${groupDone}/${group.features.length} (${Math.round(groupDone / group.features.length * 100)}%)</title>\n${cells}\n</g>` +
      (label.length > 1 ? `\n${text(ox + group.box.x + 4, oy + group.box.y + 15, 12, label)}` : '');
  });
  return [text(x + 4, y + 21, 16, `${area}: ${percentage(rows)}% (${done}/${total})`), ...groups].join('\n');
}

export function progressSvg() {
  const height = Math.ceil(areas.length / columns) * (panelHeight + gap) - gap;
  const panels = areas.map((area, index) =>
    panel(area, (index % columns) * (panelWidth + gap), Math.floor(index / columns) * (panelHeight + gap)));
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${width} ${height}" width="${width}" height="${height}">
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
