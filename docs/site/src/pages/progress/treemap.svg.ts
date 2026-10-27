import type { APIRoute } from 'astro';
import { treemapImage } from '../../lib/progress-svg';

export const GET: APIRoute = () => new Response(treemapImage(), {
  headers: { 'Content-Type': 'image/svg+xml; charset=utf-8' },
});
