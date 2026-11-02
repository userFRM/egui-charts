import type { APIRoute } from 'astro';
import { areas, counts, features, statuses } from '../../lib/progress';

const colours = { done: '#2ea043', partial: '#d29922', missing: '#6e7681' };

export const GET: APIRoute = () => {
  const rows = areas.map((area, index) => {
    const summary = counts(features.filter((feature) => feature.area === area));
    const y = 48 + index * 36;
    let x = 108;
    const bars = statuses.map((status, column) => {
      const width = summary.total ? summary[status] / summary.total * 136 : 0;
      const bar = `<rect x="${x}" y="${y - 13}" width="${width}" height="16" fill="${colours[status]}"/>`;
      x += width;
      return `${bar}<text x="${276 + column * 56}" y="${y}" text-anchor="end">${summary[status]}</text>`;
    });
    return `<g><title>${area}: ${summary.done} done, ${summary.partial} partial, ${summary.missing} missing</title><text x="8" y="${y}">${area}</text>${bars.join('')}</g>`;
  });
  return new Response(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 400 ${areas.length * 36 + 32}" width="400" height="${areas.length * 36 + 32}" role="img" aria-labelledby="title description">
<title id="title">Feature status by area</title>
<desc id="description">Each bar shows the share of features done in green, partial in yellow and missing in grey. Counts are listed beside each bar.</desc>
<style>text{font:14px sans-serif;fill:#1f2328}@media(prefers-color-scheme:dark){text{fill:#f0f6fc}}</style>
<text x="8" y="18">Area</text>
${statuses.map((status, index) => `<rect x="${250 + index * 56}" y="5" width="8" height="4" fill="${colours[status]}"/><text x="${276 + index * 56}" y="24" text-anchor="end" style="font-size:12px">${status[0].toUpperCase() + status.slice(1)}</text>`).join('')}
${rows.join('\n')}
</svg>`, { headers: { 'Content-Type': 'image/svg+xml; charset=utf-8' } });
};
