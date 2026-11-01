import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { areas, features, statuses } from './src/lib/progress.ts';

const page = readFileSync(new URL('./dist/progress/index.html', import.meta.url), 'utf8');
const svg = readFileSync(new URL('./dist/progress/areas.svg', import.meta.url), 'utf8');
const escape = (value) => value.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;').replaceAll('"', '&quot;');
const sections = [...page.matchAll(/<section\b[^>]*>(.*?)<\/section>/gs)].map((match) => match[1]);

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
    const rows = [...sections[index].matchAll(/<tr>(.*?)<\/tr>/gs)].slice(1);
    assert.deepEqual(rows.map((row) => [...row[1].matchAll(/<t[dh]\b[^>]*>(.*?)<\/t[dh]>/g)].map((cell) => cell[1])), expected, area);
  }
});

test('incomplete features link their recorded issues and omit completed features', () => {
  assert.ok(page.includes('href="https://github.com/userFRM/egui-charts/issues/1759"'));
  for (const [index, area] of areas.entries()) {
    const expected = features.filter((feature) => feature.area === area && feature.status !== 'done').map((feature) => {
      const links = feature.issues ? ': ' + feature.issues.split(';').map((issue) =>
        `<a href="https://github.com/userFRM/egui-charts/issues/${issue}">#${issue}</a>`).join(', ') : '';
      return `${escape(feature.feature)} (${feature.status})${links}`;
    });
    assert.deepEqual([...sections[index].matchAll(/<li>(.*?)<\/li>/gs)].map((match) => match[1]), expected, area);
  }
});
