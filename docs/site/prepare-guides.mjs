import { readFile, writeFile } from 'node:fs/promises';
import { guides } from './guides.mjs';

for (const { slug, source, title } of guides) {
  const markdown = await readFile(new URL(`../${source}`, import.meta.url), 'utf8');
  if (!markdown.startsWith('# ')) throw new Error(`Missing guide title: ${source}`);
  const editUrl = `https://github.com/userFRM/egui-charts/edit/main/docs/${source}`;
  const frontmatter = `---\ntitle: ${title}\neditUrl: ${editUrl}\n---\n`;
  await writeFile(
    new URL(`src/content/docs/${slug}.md`, import.meta.url),
    frontmatter + markdown.slice(markdown.indexOf('\n') + 1),
  );
}
