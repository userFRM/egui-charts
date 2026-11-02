import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { areas, features, statuses } from './src/lib/progress.ts';

const page = readFileSync(new URL('./dist/progress/index.html', import.meta.url), 'utf8');
const svg = readFileSync(new URL('./dist/progress/areas.svg', import.meta.url), 'utf8');
const escape = (value) => value.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;').replaceAll('"', '&quot;');
const sections = [...page.matchAll(/<section\b[^>]*>(.*?)<\/section>/gs)].map((match) => match[1]);

test('map labels end at word boundaries and retain their full group tooltip', () => {
  const map = readFileSync(new URL('./dist/progress/progress.svg', import.meta.url), 'utf8');
  const groups = [...map.matchAll(/<g><title>(.*?)<\/title>(.*?)<\/g>/gs)];
  const names = areas.flatMap((area) => [...new Set(features.filter((feature) => feature.area === area).map((feature) => feature.group))]);
  assert.deepEqual(groups.map((group) => group[1].split(':')[0]).sort(), names.map(escape).sort());
  let labels = 0;
  for (const group of groups) {
    const name = group[1].split(':')[0];
    const label = group[2].match(/<text\b[^>]*>(.*?)<\/text>/)?.[1];
    if (label) {
      labels++;
      assert.ok(label === name || (label.endsWith('…') && name.startsWith(`${label.slice(0, -1)} `)), `${name}: ${label}`);
    }
  }
  assert.ok(labels > 0);
  assert.equal([...map.matchAll(/<text\b/g)].length, labels + areas.length);
  assert.ok(page.includes(map), 'the inline map exposes cell tooltips on the status page');
});

test('published area bars and group tables count every CSV status', () => {
  assert.equal(sections.length, areas.length);
  const bars = [...svg.matchAll(/<g>(.*?)<\/g>/gs)].map((match) => match[1]);
  assert.equal(bars.length, areas.length);
  for (const [index, area] of areas.entries()) {
    const members = features.filter((feature) => feature.area === area);
    const summary = statuses.map((status) => members.filter((feature) => feature.status === status).length);
    assert.deepEqual([...bars[index].matchAll(/<text\b[^>]*>(.*?)<\/text>/g)].map((match) => match[1]),
      [area, ...summary.map(String)], area);
    const widths = [...bars[index].matchAll(/<rect\b[^>]*width="([^"]+)"/g)].map((match) => Number(match[1]));
    assert.equal(widths.length, statuses.length);
    widths.forEach((width, column) => assert.ok(Math.abs(width - summary[column] / members.length * 136) < 1e-9, area));
    const groups = [...new Set(members.map((feature) => feature.group))];
    const expected = groups.map((group) => {
      const rows = members.filter((feature) => feature.group === group);
      const counts = statuses.map((status) => rows.filter((feature) => feature.status === status).length);
      return [escape(group), ...counts.map(String), String(rows.length), `${Math.round(counts[0] / rows.length * 100)}%`];
    });
    expected.push(['Total', ...summary.map(String), String(members.length), `${(summary[0] / members.length * 100).toFixed(1)}%`]);
    const rows = [...sections[index].matchAll(/<tr\b[^>]*>(.*?)<\/tr>/gs)].slice(1);
    assert.deepEqual(rows.map((row) => [...row[1].matchAll(/<t[dh]\b[^>]*>(.*?)<\/t[dh]>/g)].map((cell) => cell[1])), expected, area);
  }
});

test('incomplete features link their recorded issues and omit completed features', () => {
  for (const [index, area] of areas.entries()) {
    const expected = features.filter((feature) => feature.area === area && feature.status !== 'done').map((feature) => {
      const links = feature.issues.split(';').filter(Boolean).map((issue) =>
        `<a href="https://github.com/userFRM/egui-charts/issues/${issue}">#${issue}</a>`).join(', ');
      return `${escape(feature.feature)} (${feature.status})${links ? ': ' + links : ''}`;
    });
    assert.deepEqual([...sections[index].matchAll(/<li\b[^>]*>(.*?)<\/li>/gs)].map((match) =>
      match[1].replace(/<a\b[^>]*href="([^"]+)"[^>]*>/g, '<a href="$1">')), expected, area);
  }
});
