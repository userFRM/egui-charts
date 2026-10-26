import { basename } from 'node:path';
import { guides } from './guides.mjs';

const base = '/egui-charts/docs/';
const shared = new Map(guides.map(({ source, slug }) => [source, slug]));

// Keep GitHub source links intact while publishing chapter URLs and hiding Rust setup lines.
export default function remarkDocs() {
  return function transform(node) {
    if (['link', 'definition'].includes(node.type)) {
      node.url = node.url.replace('https://userfrm.github.io/egui-charts/docs/', base);
    }
    if (['link', 'definition'].includes(node.type) && !/^(?:[a-z]+:|\/|#)/i.test(node.url)) {
      const [path, anchor] = node.url.split('#');
      const name = basename(path);
      if (/\.mdx?$/.test(name)) {
        const slug = shared.get(name) ?? name.replace(/\.mdx?$/, '');
        node.url = `${base}${slug === 'index' ? '' : `${slug}/`}${anchor ? `#${anchor}` : ''}`;
      }
    }
    if (node.type === 'code' && node.lang?.startsWith('rust')) {
      node.lang = 'rust';
      node.value = node.value.split('\n').filter((line) => !/^\s*#(?: |$)/.test(line)).join('\n');
    }
    for (const child of node.children ?? []) transform(child);
  };
}
