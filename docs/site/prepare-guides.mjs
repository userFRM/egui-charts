import { readFile, unlink, writeFile } from 'node:fs/promises';
import { guides } from './guides.mjs';

const slugs = new Set();
const repositoryPath = /^(?:[\w-]+\/)*[\w.-]+\.md$/;
for (const { slug, source, editPath, title, scene } of guides) {
  if (typeof slug !== 'string' || !/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(slug) || slugs.has(slug)) {
    throw new Error(`Invalid or duplicate guide: ${slug}`);
  }
  slugs.add(slug);
  if (typeof source !== 'string' || !repositoryPath.test(source)) throw new Error(`Invalid guide source: ${source}`);
  if (typeof editPath !== 'string' || !repositoryPath.test(editPath)) throw new Error(`Invalid guide edit path: ${editPath}`);
  if (typeof title !== 'string' || !title.trim()) throw new Error(`Invalid guide title: ${slug}`);
  if (scene !== undefined && (typeof scene !== 'string' || !/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(scene))) {
    throw new Error(`Invalid guide scene: ${slug}`);
  }
}

// Validate every source before replacing any generated page.
const pages = await Promise.all(guides.map(async ({ slug, source, editPath, title, scene }) => {
  const markdown = await readFile(new URL(`../../${source}`, import.meta.url), 'utf8');
  const heading = markdown.match(/^# [^\r\n]+(?:\r?\n|$)/);
  if (!heading) throw new Error(`Missing guide title: ${source}`);
  const editUrl = `https://github.com/userFRM/egui-charts/edit/main/${editPath}`;
  const frontmatter = `---\ntitle: ${JSON.stringify(title)}\neditUrl: ${editUrl}\n---\n`;
  let body = markdown.slice(heading[0].length);
  if (scene) {
    const marker = '<!-- example -->';
    if (body.split(marker).length !== 2) throw new Error(`Expected one example marker: ${source}`);
    const sceneSource = `examples/docs/scenes/${scene.replaceAll('-', '_')}.rs`;
    await readFile(new URL(`../../${sceneSource}`, import.meta.url));
    body = `import Example from '../../components/Example.astro';\nimport source from '../../../../../${sceneSource}?raw';\n\n` + body.replace(marker,
      `<Example scene="${scene}" title={${JSON.stringify(title)}} run="manual" task="Run the example, then try the controls described in this guide." code={source.split('#[cfg(test)]')[0].trim()} />`);
  }
  return { slug, extension: scene ? 'mdx' : 'md', content: frontmatter + body };
}));

for (const { slug, extension, content } of pages) {
  if (extension === 'mdx') {
    await unlink(new URL(`src/content/docs/${slug}.md`, import.meta.url)).catch((error) => {
      if (error.code !== 'ENOENT') throw error;
    });
  }
  await writeFile(
    new URL(`src/content/docs/${slug}.${extension}`, import.meta.url),
    content,
  );
}
