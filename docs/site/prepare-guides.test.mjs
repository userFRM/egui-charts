import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { copyFile, mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { promisify } from 'node:util';
import test from 'node:test';

const run = promisify(execFile);
const guide = { slug: 'release-notes', source: 'CHANGELOG.md', editPath: 'CHANGELOG.md', title: 'Release notes' };

async function fixture(t) {
  const root = await mkdtemp(join(tmpdir(), 'egui-charts-guides-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  const site = join(root, 'docs/site');
  await mkdir(join(site, 'src/content/docs'), { recursive: true });
  await copyFile(new URL('./prepare-guides.mjs', import.meta.url), join(site, 'prepare-guides.mjs'));
  await writeFile(join(root, 'CHANGELOG.md'), '# Changelog\n\n## [Unreleased]\n\n## [0.2.0] - 2026-06-04\n');
  return { root, site };
}

test('publish root and nested sources with their original edit paths and intact release sections', async (t) => {
  const { root, site } = await fixture(t);
  const nested = { slug: 'replay', source: 'docs/replay.md', editPath: 'docs/original-replay.md', title: 'Replay: "bars"' };
  const runnable = { ...nested, slug: 'order-flow', scene: 'order-flow', source: 'docs/flow.md' };
  await writeFile(join(site, 'guides.mjs'), `export const guides = ${JSON.stringify([guide, nested, runnable])};`);
  await writeFile(join(root, 'docs/replay.md'), '# Recording\r\n\r\nKeep this body.\r\n');
  await writeFile(join(root, 'docs/flow.md'), '# Execution inputs\n\nReadable before running.\n\n<!-- example -->\n\nThe contract stays here.\n');
  await mkdir(join(root, 'examples/docs/scenes'), { recursive: true });
  await writeFile(join(root, 'examples/docs/scenes/order_flow.rs'), '// Compiled scene\n');
  await writeFile(join(site, 'src/content/docs/order-flow.md'), 'Old generated page');
  await run(process.execPath, [join(site, 'prepare-guides.mjs')], { cwd: tmpdir() });
  for (const entry of [guide, nested]) {
    const source = await readFile(join(root, entry.source), 'utf8');
    const page = await readFile(join(site, `src/content/docs/${entry.slug}.md`), 'utf8');
    assert.equal(page, `---\ntitle: ${JSON.stringify(entry.title)}\neditUrl: https://github.com/userFRM/egui-charts/edit/main/${entry.editPath}\n---\n` + source.slice(source.indexOf('\n') + 1));
  }
  const page = await readFile(join(site, 'src/content/docs/order-flow.mdx'), 'utf8');
  assert.match(page, /import Example from '\.\.\/\.\.\/components\/Example.astro'/);
  assert.match(page, /import source from '\.\.\/\.\.\/\.\.\/\.\.\/\.\.\/examples\/docs\/scenes\/order_flow.rs\?raw'/);
  assert.match(page, /Readable before running\.[\s\S]*<Example scene="order-flow"[^>]*run="manual"[\s\S]*The contract stays here\./);
  assert.doesNotMatch(page, /<!-- example -->/);
  await assert.rejects(readFile(join(site, 'src/content/docs/order-flow.md')), { code: 'ENOENT' });
});

test('reject invalid guide manifests and missing titles before writing pages', async (t) => {
  const { root, site } = await fixture(t);
  for (const [name, entries, markdown, message] of [
    ['duplicate route', [guide, guide], '# Changelog\n', /Invalid or duplicate guide/],
    ['route traversal', [{ ...guide, slug: '../outside' }], '# Changelog\n', /Invalid or duplicate guide/],
    ['source traversal', [{ ...guide, source: '../outside.md' }], '# Changelog\n', /Invalid guide source/],
    ['absolute source', [{ ...guide, source: '/CHANGELOG.md' }], '# Changelog\n', /Invalid guide source/],
    ['edit traversal', [{ ...guide, editPath: 'docs/../../outside.md' }], '# Changelog\n', /Invalid guide edit path/],
    ['missing edit path', [{ ...guide, editPath: undefined }], '# Changelog\n', /Invalid guide edit path/],
    ['empty title', [{ ...guide, title: '' }], '# Changelog\n', /Invalid guide title/],
    ['missing source heading', [guide], '## Unreleased\n', /Missing guide title/],
    ['scene traversal', [{ ...guide, scene: '../outside' }], '# Changelog\n', /Invalid guide scene/],
    ['missing example marker', [{ ...guide, scene: 'host-report' }], '# Changelog\n', /Expected one example marker/],
    ['duplicate example markers', [{ ...guide, scene: 'host-report' }], '# Changelog\n<!-- example --><!-- example -->\n', /Expected one example marker/],
  ]) {
    await writeFile(join(site, 'guides.mjs'), `export const guides = ${JSON.stringify(entries)};`);
    await writeFile(join(root, 'CHANGELOG.md'), markdown);
    await assert.rejects(run(process.execPath, [join(site, 'prepare-guides.mjs')]), message, name);
    await assert.rejects(readFile(join(site, 'src/content/docs/release-notes.md')), { code: 'ENOENT' }, name);
  }
});
