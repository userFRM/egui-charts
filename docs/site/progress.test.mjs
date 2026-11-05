import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { areas, counts, features, percentage, slug, statuses } from './src/lib/progress.ts';

const page = readFileSync(new URL('./dist/progress/index.html', import.meta.url), 'utf8');
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

test('published group tables and badges count only independently verified features', () => {
  assert.deepEqual(statuses, ['verified', 'unverified', 'incomplete', 'missing']);
  const sample = statuses.map((status) => ({ ...features[0], status }));
  assert.deepEqual(counts(sample), { verified: 1, unverified: 1, incomplete: 1, missing: 1, total: 4 });
  assert.equal(percentage(sample), '25.0');
  assert.equal(percentage([]), '0.0');
  assert.equal(sections.length, areas.length);
  for (const [index, area] of areas.entries()) {
    const members = features.filter((feature) => feature.area === area);
    const summary = statuses.map((status) => members.filter((feature) => feature.status === status).length);
    const badge = readFileSync(new URL(`./dist/progress/badge-${slug(area)}.svg`, import.meta.url), 'utf8');
    assert.ok(badge.includes(`aria-label="${area.toLowerCase()} verified: ${(summary[0] / members.length * 100).toFixed(1)}%"`), area);
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

test('features awaiting verification or implementation link their issues and omit verified features', () => {
  for (const [index, area] of areas.entries()) {
    const members = features.filter((feature) => feature.area === area);
    const expected = [...new Set(members.map((feature) => feature.group))].flatMap((group) =>
      members.filter((feature) => feature.group === group && feature.status !== 'verified')).map((feature) => {
      const links = feature.issues.split(';').filter(Boolean).map((issue) =>
        `<a href="https://github.com/userFRM/egui-charts/issues/${issue}">#${issue}</a>`).join(', ');
      return `${escape(feature.feature)} (${feature.status})${links ? ': ' + links : ''}`;
    });
    assert.deepEqual([...sections[index].matchAll(/<li\b[^>]*>(.*?)<\/li>/gs)].map((match) =>
      match[1].replaceAll('&#39;', "'").replace(/<a\b[^>]*href="([^"]+)"[^>]*>/g, '<a href="$1">')), expected, area);
  }
});
