import assert from 'node:assert/strict';
import test from 'node:test';
import remarkDocs from './remark-docs.mjs';

test('publish source links at the project base and hide only Rust setup lines', () => {
  const tree = { children: [
    { type: 'link', url: 'first-chart.mdx#keep-state-between-frames' },
    { type: 'link', url: '../../../../order-flow-replay.md' },
    { type: 'link', url: 'https://docs.rs/egui-charts/0.2.0' },
    { type: 'link', url: '#local-heading' },
    { type: 'link', url: 'https://userfrm.github.io/egui-charts/docs/api/' },
    { type: 'code', lang: 'rust,no_run', value: '# let setup = 1;\nassert_eq!(setup, 1);' },
    { type: 'code', lang: 'sh', value: '# Keep shell comments\ncargo check' },
  ] };
  remarkDocs()(tree);
  assert.deepEqual(tree.children, [
    { type: 'link', url: '/egui-charts/docs/first-chart/#keep-state-between-frames' },
    { type: 'link', url: '/egui-charts/docs/replay/' },
    { type: 'link', url: 'https://docs.rs/egui-charts/0.2.0' },
    { type: 'link', url: '#local-heading' },
    { type: 'link', url: '/egui-charts/docs/api/' },
    { type: 'code', lang: 'rust', value: 'assert_eq!(setup, 1);' },
    { type: 'code', lang: 'sh', value: '# Keep shell comments\ncargo check' },
  ]);
});
