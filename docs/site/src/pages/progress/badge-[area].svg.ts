import type { APIRoute } from 'astro';
import { areas, features, slug } from '../../lib/progress';
import { badgeSvg } from '../../lib/progress-svg';

export const getStaticPaths = () => areas.map((area) => ({ params: { area: slug(area) }, props: { area } }));

export const GET: APIRoute = ({ props }) => new Response(badgeSvg(props.area.toLowerCase(), features.filter((row) => row.area === props.area)), {
  headers: { 'Content-Type': 'image/svg+xml; charset=utf-8' },
});
