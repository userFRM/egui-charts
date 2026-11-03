import { readFile, writeFile } from 'node:fs/promises';
import { guides } from './guides.mjs';

const slugs = new Set();
const repositoryPath = /^(?:[\w-]+\/)*[\w.-]+\.md$/;
for (const { slug, source, editPath, title } of guides) {
  if (typeof slug !== 'string' || !/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(slug) || slugs.has(slug)) {
    throw new Error(`Invalid or duplicate guide: ${slug}`);
  }
  slugs.add(slug);
  if (typeof source !== 'string' || !repositoryPath.test(source)) throw new Error(`Invalid guide source: ${source}`);
  if (typeof editPath !== 'string' || !repositoryPath.test(editPath)) throw new Error(`Invalid guide edit path: ${editPath}`);
  if (typeof title !== 'string' || !title.trim()) throw new Error(`Invalid guide title: ${slug}`);
}

// Validate every source before replacing any generated page.
const pages = await Promise.all(guides.map(async ({ slug, source, editPath, title }) => {
  const markdown = await readFile(new URL(`../../${source}`, import.meta.url), 'utf8');
  const heading = markdown.match(/^# [^\r\n]+(?:\r?\n|$)/);
  if (!heading) throw new Error(`Missing guide title: ${source}`);
  const editUrl = `https://github.com/userFRM/egui-charts/edit/main/${editPath}`;
  const frontmatter = `---\ntitle: ${JSON.stringify(title)}\neditUrl: ${editUrl}\n---\n`;
  return { slug, content: frontmatter + markdown.slice(heading[0].length) };
}));

for (const { slug, content } of pages) {
  await writeFile(
    new URL(`src/content/docs/${slug}.md`, import.meta.url),
    content,
  );
}
