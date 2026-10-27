import { areas, counts, features, history, percentage, statuses, treemap, type Feature } from './progress';

const colours = { done: 'var(--done, #39a871)', partial: 'var(--partial, #dba134)', missing: 'var(--missing, #414855)' };
const panel = 'var(--sl-color-gray-6, #171b23)';
const text = 'var(--sl-color-text, #edf0f5)';
const muted = 'var(--sl-color-gray-2, #bdc4d0)';
const escape = (value: string) => value.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;').replaceAll('"', '&quot;');
const svg = (height: number, label: string, content: string) =>
  `<svg xmlns="http://www.w3.org/2000/svg" width="900" height="${height}" viewBox="0 0 900 ${height}" role="img" aria-label="${escape(label)}" style="font-family:system-ui,sans-serif"><title>${escape(label)}</title>${content}</svg>`;

const treemapHeight = (rows: Feature[]) => Math.max(250, rows.length * 1.9);

/** SVG text keeps group captions visible when embedded as an image. */
export function treemapSvg(rows: Feature[]) {
  const height = treemapHeight(rows);
  const groups = treemap(rows, 900, height).map((group) => {
    const cells = group.features.map((feature, index) => {
      const box = group.cells[index];
      const host = feature.provided_by === 'host';
      return `<rect x="${box.x}" y="${box.y}" width="${box.width}" height="${box.height}" fill="${colours[feature.status]}" stroke="${host ? text : panel}" stroke-width="${host ? 1.25 : 1}" ${host ? 'stroke-dasharray="3 2"' : ''} data-feature="${escape(feature.feature)}"><title>${escape(feature.feature)}: ${feature.status}</title></rect>`;
    }).join('');
    const box = group.box;
    const lines: string[] = [''];
    const capacity = Math.max(8, Math.floor((box.width - 24) / 7));
    for (const word of group.name.split(' ')) {
      const last = lines.length - 1;
      if (lines[last] && lines[last].length + word.length + 1 > capacity) lines.push(word);
      else lines[last] += `${lines[last] ? ' ' : ''}${word}`;
    }
    const width = Math.min(box.width - 8, Math.max(...lines.map((line) => line.length)) * 7 + 16);
    const labelHeight = lines.length * 16 + 8;
    const cx = box.x + box.width / 2, cy = box.y + box.height / 2;
    return `<g>${cells}<rect x="${box.x}" y="${box.y}" width="${box.width}" height="${box.height}" fill="none" stroke="${panel}" stroke-width="3" style="pointer-events:none"/>
      <g style="pointer-events:none"><rect x="${cx - width / 2}" y="${cy - labelHeight / 2}" width="${width}" height="${labelHeight}" rx="4" fill="${panel}"/>
      <text text-anchor="middle" fill="${text}" font-size="12" font-weight="600">${lines.map((line, index) => `<tspan x="${cx}" y="${cy - labelHeight / 2 + 16 + index * 16}">${escape(line)}</tspan>`).join('')}</text></g></g>`;
  }).join('');
  const summary = counts(rows);
  return svg(height, `${rows[0]?.area ?? 'Features'}: ${summary.done} done, ${summary.partial} partial, ${summary.missing} missing`, groups);
}

function legend() {
  return statuses.map((status, index) => `<rect x="${20 + index * 130}" y="7" width="12" height="12" rx="2" fill="${colours[status]}"/><text x="${40 + index * 130}" y="18" fill="${muted}" font-size="12">${status}</text>`).join('');
}

export function historySvg(snapshots: ReturnType<typeof history>) {
  if (!snapshots.length) return svg(80, 'Progress history', `<text x="20" y="40" fill="${text}" font-size="14">History starts with the first committed feature list.</text>`);
  const dates = snapshots.map((point) => Date.parse(point.date));
  const start = Math.min(...dates), end = Math.max(...dates);
  const maximum = Math.max(1, ...snapshots.flatMap((point) => statuses.map((status) => point[status])));
  const x = (index: number) => start === end ? 452 : 52 + (dates[index] - start) / (end - start) * 800;
  const y = (count: number) => 20 + 190 * (1 - count / maximum);
  const axes = [0, 0.5, 1].map((fraction) => `<line x1="52" x2="852" y1="${y(maximum * fraction)}" y2="${y(maximum * fraction)}" stroke="var(--sl-color-hairline, #414855)"/><text x="42" y="${y(maximum * fraction) + 4}" text-anchor="end" fill="${muted}" font-size="12">${Math.round(maximum * fraction)}</text>`).join('');
  const lines = statuses.map((status) => `<polyline fill="none" stroke="${colours[status]}" stroke-width="2" points="${snapshots.map((point, index) => `${x(index)},${y(point[status])}`).join(' ')}"/>${snapshots.map((point, index) => `<circle cx="${x(index)}" cy="${y(point[status])}" r="4" fill="${colours[status]}" stroke="${text}"><title>${point.date.slice(0, 10)}: ${point[status]} ${status} (${point.commit.slice(0, 8)})</title></circle>`).join('')}`).join('');
  const datesLabel = `<g fill="${muted}" font-size="12"><text x="52" y="239">${snapshots[0].date.slice(0, 10)}</text>${snapshots.length > 1 ? `<text x="852" y="239" text-anchor="end">${snapshots.at(-1)!.date.slice(0, 10)}</text>` : ''}</g>`;
  return svg(280, `Feature counts across ${snapshots.length} commits`, `${axes}${lines}${datesLabel}<g transform="translate(32 250)">${legend()}</g>`);
}

/** Opaque panels keep the exported images readable on any surrounding background. */
export function treemapImage() {
  let y = 54;
  const panels = areas.map((area) => {
    const rows = features.filter((row) => row.area === area);
    const summary = counts(rows);
    const height = treemapHeight(rows) * 868 / 900;
    const content = `<g transform="translate(0 ${y})"><text x="16" y="20" fill="${text}" font-size="17" font-weight="600">${area} · ${percentage(rows)}%</text><text x="884" y="20" text-anchor="end" fill="${muted}" font-size="12">${summary.done} done · ${summary.partial} partial · ${summary.missing} missing</text><g transform="translate(16 34) scale(${868 / 900})">${treemapSvg(rows)}</g></g>`;
    y += height + 58;
    return content;
  }).join('');
  return svg(y, 'Feature status by area', `<rect width="900" height="${y}" rx="12" fill="${panel}"/><g transform="translate(0 12)">${legend()}<rect x="430" y="7" width="12" height="12" fill="none" stroke="${text}" stroke-dasharray="3 2"/><text x="450" y="18" fill="${muted}" font-size="12">Dashed outline: host-provided</text></g>${panels}`);
}

export function historyImage() {
  return svg(340, 'Progress history', `<rect width="900" height="340" rx="12" fill="${panel}"/><text x="20" y="32" fill="${text}" font-size="17" font-weight="600">Progress history</text><g transform="translate(0 48)">${historySvg(history())}</g>`);
}
