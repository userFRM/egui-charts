import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';

export const areas = ['Chart', 'Drawings', 'Studies', 'Order flow', 'Terminal', 'Builder API'] as const;
export const statuses = ['done', 'partial', 'missing'] as const;
type Status = typeof statuses[number];
export type Feature = {
  area: typeof areas[number];
  group: string;
  feature: string;
  status: Status;
  provided_by: 'library' | 'host';
  source: string;
};

export const slug = (area: string) => area.toLowerCase().replaceAll(' ', '-');
const root = execFileSync('git', ['rev-parse', '--show-toplevel'], { encoding: 'utf8' }).trim();
const path = 'docs/progress/features.csv';

function parseFeatures(csv: string): Feature[] {
  const rows: string[][] = [];
  let row: string[] = [], field = '', quoted = false;
  for (let i = 0; i < csv.length; i++) {
    const char = csv[i];
    if (char === '"') {
      if (quoted && csv[i + 1] === '"') { field += '"'; i++; }
      else quoted = !quoted;
    } else if (!quoted && (char === ',' || char === '\n')) {
      row.push(field);
      field = '';
      if (char === '\n') { rows.push(row); row = []; }
    } else if (char !== '\r' || quoted) field += char;
  }
  if (quoted) throw new Error('Unclosed feature CSV field');
  if (field || row.length) rows.push([...row, field]);
  const columns = ['area', 'group', 'feature', 'status', 'provided_by', 'source'];
  if (rows.shift()?.join(',') !== columns.join(',')) throw new Error('Invalid feature CSV columns');
  const identities = new Set<string>();
  return rows.map((values) => {
    const feature = Object.fromEntries(columns.map((key, i) => [key, values[i]])) as Feature;
    const identity = JSON.stringify([feature.area, feature.group, feature.feature]);
    if (values.length !== columns.length || !areas.includes(feature.area) ||
        !statuses.includes(feature.status) || !['library', 'host'].includes(feature.provided_by) ||
        !feature.group || !feature.feature || identities.has(identity) ||
        (feature.status === 'missing' && feature.source)) {
      throw new Error(`Invalid or duplicate feature: ${identity}`);
    }
    identities.add(identity);
    return feature;
  });
}

export function counts(features: Feature[]) {
  return {
    done: features.filter((feature) => feature.status === 'done').length,
    partial: features.filter((feature) => feature.status === 'partial').length,
    missing: features.filter((feature) => feature.status === 'missing').length,
    total: features.length,
  };
}
export const percentage = (features: Feature[]) =>
  (features.length ? counts(features).done / features.length * 100 : 0).toFixed(1);
export const features = parseFeatures(readFileSync(`${root}/${path}`, 'utf8'));

export function history() {
  const commits = execFileSync('git', ['log', '--format=%H%x09%cI', '--', path], { cwd: root, encoding: 'utf8' }).trim();
  if (!commits) return [];
  return commits.split('\n').map((entry) => {
    const [commit, date] = entry.split('\t');
    const csv = execFileSync('git', ['show', `${commit}:${path}`], { cwd: root, encoding: 'utf8' });
    return { commit, date, ...counts(parseFeatures(csv)) };
  }).sort((a, b) => Date.parse(a.date) - Date.parse(b.date));
}

type Box = { x: number; y: number; width: number; height: number };

/** Lay out positive weights in rows chosen to minimise the worst cell aspect ratio. */
function squarify(weights: number[], box: Box): Box[] {
  const total = weights.reduce((sum, weight) => sum + weight, 0);
  if (!weights.length) return [];
  if (weights.some((weight) => !Number.isFinite(weight) || weight <= 0) ||
      !Number.isFinite(total) || box.width <= 0 || box.height <= 0) {
    throw new Error('Treemap weights and dimensions must be positive and finite');
  }
  const items = weights.map((weight, index) => ({ index, area: weight / total * box.width * box.height }))
    .sort((a, b) => b.area - a.area);
  const result: Box[] = new Array(weights.length);
  let remaining = { ...box }, offset = 0;
  while (offset < items.length) {
    const side = Math.min(remaining.width, remaining.height);
    let length = 1, sum = items[offset].area;
    const worst = (area: number, min: number, max: number) =>
      Math.max(side * side * max / (area * area), area * area / (side * side * min));
    while (offset + length < items.length) {
      const next = items[offset + length].area;
      if (worst(sum + next, next, items[offset].area) >
          worst(sum, items[offset + length - 1].area, items[offset].area)) break;
      sum += next;
      length++;
    }
    const vertical = remaining.width >= remaining.height;
    const thickness = sum / side;
    let cursor = vertical ? remaining.y : remaining.x;
    for (const item of items.slice(offset, offset + length)) {
      const span = item.area / thickness;
      result[item.index] = vertical
        ? { x: remaining.x, y: cursor, width: thickness, height: span }
        : { x: cursor, y: remaining.y, width: span, height: thickness };
      cursor += span;
    }
    if (vertical) { remaining.x += thickness; remaining.width -= thickness; }
    else { remaining.y += thickness; remaining.height -= thickness; }
    offset += length;
  }
  return result;
}

export function treemap(features: Feature[], width: number, height: number) {
  const names = [...new Set(features.map((feature) => feature.group))];
  const groups = names.map((name) => ({ name, features: features.filter((feature) => feature.group === name) }));
  const blocks = squarify(groups.map((group) => group.features.length), { x: 0, y: 0, width, height });
  return groups.map((group, index) => ({
    ...group,
    box: blocks[index],
    cells: squarify(group.features.map(() => 1), blocks[index]),
  }));
}
