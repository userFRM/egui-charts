import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parse } from 'parse5';

const site = new URL('https://userfrm.github.io/egui-charts/docs/');

export function checkLinks(root, api = join(root, 'api')) {
  root = resolve(root);
  api = resolve(api);
  const pages = new Map();
  const errors = [];
  let links = 0;

  function page(file) {
    if (pages.has(file)) return pages.get(file);
    const result = { ids: new Set(), links: [], redirect: undefined };
    function visit(node) {
      const attrs = Object.fromEntries((node.attrs ?? []).map(({ name, value }) => [name, value]));
      if (attrs.id) result.ids.add(attrs.id);
      if (node.tagName === 'a' && attrs.name) result.ids.add(attrs.name);
      if (['a', 'area'].includes(node.tagName) && attrs.href !== undefined) result.links.push(attrs.href);
      if (node.tagName === 'meta' && attrs['http-equiv']?.toLowerCase() === 'refresh') {
        result.redirect = attrs.content?.match(/url\s*=\s*['"]?([^'"]+)/i)?.[1];
      }
      for (const child of node.childNodes ?? []) visit(child);
    }
    visit(parse(readFileSync(file, 'utf8')));
    pages.set(file, result);
    return result;
  }

  function target(url, seen = new Set()) {
    const pathname = decodeURIComponent(url.pathname.slice(site.pathname.length));
    const fileRoot = pathname.startsWith('api/') ? api : root;
    let file = resolve(fileRoot, pathname.startsWith('api/') ? pathname.slice(4) : pathname);
    if (file !== fileRoot && !file.startsWith(fileRoot + sep)) throw new Error('path escapes the site');
    if (statSync(file).isDirectory()) file = join(file, 'index.html');
    if (!file.endsWith('.html')) return;
    const document = page(file);
    if (document.redirect) {
      if (seen.has(file)) throw new Error('redirect cycle');
      seen.add(file);
      const destination = new URL(document.redirect, url);
      if (!destination.hash) destination.hash = url.hash;
      return target(destination, seen);
    }
    const id = decodeURIComponent(url.hash.slice(1));
    if (id && !document.ids.has(id) && id.toLowerCase() !== 'top') throw new Error(`missing anchor #${id}`);
  }

  function walk(directory) {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const file = join(directory, entry.name);
      if (entry.isDirectory()) {
        // Rustdoc generates its own navigation; validate API targets reached from the guide.
        if (file !== join(root, 'api')) walk(file);
      } else if (entry.name.endsWith('.html')) {
        const from = new URL(relative(root, file), site);
        for (const href of page(file).links) {
          try {
            const url = new URL(href, from);
            if (url.origin !== site.origin || !url.pathname.startsWith(site.pathname)) continue;
            // The runnable examples are published by the Pages assembly and checked by its own test.
            if (url.pathname.startsWith(`${site.pathname}examples/`)) continue;
            links++;
            target(url);
          } catch (error) {
            errors.push(`${relative(root, file)}: ${href}: ${error.message}`);
          }
        }
      }
    }
  }
  walk(root);
  return { links, errors };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const root = resolve(process.argv[2] ?? fileURLToPath(new URL('dist', import.meta.url)));
  const api = process.argv[2] ? join(root, 'api') : process.env.CARGO_TARGET_DIR
    ? resolve(process.env.CARGO_TARGET_DIR, 'doc')
    : fileURLToPath(new URL('../../target/doc', import.meta.url));
  const { links, errors } = checkLinks(root, api);
  if (errors.length) {
    console.error(errors.join('\n'));
    process.exitCode = 1;
  } else {
    console.log(`Checked ${links} local guide and API links, including anchors.`);
  }
}
