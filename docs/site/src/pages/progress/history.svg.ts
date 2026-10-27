import type { APIRoute } from 'astro';
import { historyImage } from '../../lib/progress-svg';

export const GET: APIRoute = () => new Response(historyImage(), {
  headers: { 'Content-Type': 'image/svg+xml; charset=utf-8' },
});
