import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { basename, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parse } from 'parse5';

const site = new URL('https://userfrm.github.io/egui-charts/docs/');
const memberKinds = ['method', 'tymethod', 'structfield', 'variant', 'associatedconstant', 'associatedtype'];
const memberSpan = /^(?:[a-z_][a-z0-9_]*::)*([A-Z][A-Za-z0-9_]*)::([A-Za-z_][A-Za-z0-9_]*)(?:\(.*\))?$/s;
// The release notes quote members as each release named them; they are not a guide.
const notGuides = new Set(['release-notes/index.html']);

export function checkLinks(root, api = join(root, 'api')) {
  root = resolve(root);
  api = resolve(api);
  const pages = new Map();
  const errors = [];
  let links = 0;

  function page(file) {
    if (pages.has(file)) return pages.get(file);
    const result = { ids: new Set(), links: [], codes: [], redirect: undefined };
    const text = (node) => node.value ?? (node.childNodes ?? []).map(text).join('');
    function visit(node, pre = false) {
      const attrs = Object.fromEntries((node.attrs ?? []).map(({ name, value }) => [name, value]));
      if (attrs.id) result.ids.add(attrs.id);
      if (node.tagName === 'a' && attrs.name) result.ids.add(attrs.name);
      if (['a', 'area'].includes(node.tagName) && attrs.href !== undefined) result.links.push(attrs.href);
      if (node.tagName === 'meta' && attrs['http-equiv']?.toLowerCase() === 'refresh') {
        result.redirect = attrs.content?.match(/url\s*=\s*['"]?([^'"]+)/i)?.[1];
      }
      if (node.tagName === 'code' && !pre) result.codes.push(text(node).trim());
      for (const child of node.childNodes ?? []) visit(child, pre || node.tagName === 'pre');
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
    return `${file}#${id}`;
  }

  let types;
  function members(type, member) {
    if (!types) {
      types = new Map();
      const crate = join(api, 'egui_charts');
      for (const entry of existsSync(crate) ? readdirSync(crate, { recursive: true }) : []) {
        const name = basename(entry).match(/^(?:struct|enum|trait|union|type)\.(\w+)\.html$/)?.[1];
        if (name) types.set(name, [...(types.get(name) ?? []), join(crate, entry)]);
      }
    }
    return (types.get(type) ?? []).filter((file) => !page(file).redirect).flatMap((file) =>
      memberKinds.filter((kind) => page(file).ids.has(`${kind}.${member}`)).map((kind) => `${file}#${kind}.${member}`));
  }

  function walk(directory) {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const file = join(directory, entry.name);
      if (entry.isDirectory()) {
        // Rustdoc generates its own navigation; validate API targets reached from the guide.
        if (file !== join(root, 'api')) walk(file);
      } else if (entry.name.endsWith('.html')) {
        const from = new URL(relative(root, file), site);
        const reached = new Set();
        for (const href of page(file).links) {
          try {
            const url = new URL(href, from);
            if (url.origin !== site.origin || !url.pathname.startsWith(site.pathname)) continue;
            // The runnable examples are published by the Pages assembly and checked by its own test.
            if (url.pathname.startsWith(`${site.pathname}examples/`)) continue;
            links++;
            reached.add(target(url));
          } catch (error) {
            errors.push(`${relative(root, file)}: ${href}: ${error.message}`);
          }
        }
        if (notGuides.has(relative(root, file).split(sep).join('/'))) continue;
        for (const code of new Set(page(file).codes)) {
          const [, type, member] = code.match(memberSpan) ?? [];
          const anchors = type ? members(type, member) : [];
          if (anchors.length && !anchors.some((anchor) => reached.has(anchor))) {
            errors.push(`${relative(root, file)}: \`${code}\` has no link to ${anchors.map((anchor) => relative(api, anchor)).join(' or ')}`);
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
