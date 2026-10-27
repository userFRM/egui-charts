import type { APIRoute } from 'astro';
import { areas, counts, features, percentage, slug } from '../../lib/progress';

export function getStaticPaths() {
  return [
    { params: { badge: 'overall' }, props: { label: 'Overall', rows: features } },
    ...areas.map((area) => ({ params: { badge: slug(area) }, props: { label: area, rows: features.filter((row) => row.area === area) } })),
  ];
}

export const GET: APIRoute = ({ props }) => {
  const value = percentage(props.rows);
  const { done, total } = counts(props.rows);
  const share = total ? done / total : 0;
  const colour = share < 0.5 ? '#c23b3b' : share < 0.8 ? '#a66b00' : '#238636';
  const labelWidth = props.label.length * 7 + 16;
  const width = labelWidth + 58;
  return new Response(`<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="20" role="img" aria-label="${props.label}: ${value}% done">
<title>${props.label}: ${value}% done</title>
<linearGradient id="shine" x2="0" y2="100%"><stop stop-color="#fff" stop-opacity=".1"/><stop offset="1" stop-opacity=".1"/></linearGradient>
<clipPath id="shape"><rect width="${width}" height="20" rx="3"/></clipPath>
<g clip-path="url(#shape)"><path fill="#555" d="M0 0h${labelWidth}v20H0z"/><path fill="${colour}" d="M${labelWidth} 0h58v20h-58z"/><rect width="${width}" height="20" fill="url(#shine)"/></g>
<g fill="#fff" text-anchor="middle" font-family="Verdana,DejaVu Sans,sans-serif" font-size="11"><text x="${labelWidth / 2}" y="14">${props.label}</text><text x="${labelWidth + 29}" y="14">${value}%</text></g>
</svg>`, { headers: { 'Content-Type': 'image/svg+xml; charset=utf-8' } });
};
