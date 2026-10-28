import type { APIRoute } from 'astro';
import { progressSvg } from '../../lib/progress-svg';

export const GET: APIRoute = () => new Response(progressSvg(), {
  headers: { 'Content-Type': 'image/svg+xml; charset=utf-8' },
});
