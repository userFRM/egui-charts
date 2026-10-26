import { defineConfig } from 'astro/config';
import { unified } from '@astrojs/markdown-remark';
import starlight from '@astrojs/starlight';
import remarkDocs from './remark-docs.mjs';

export default defineConfig({
  site: 'https://userfrm.github.io',
  base: '/egui-charts/docs/',
  trailingSlash: 'always',
  markdown: { processor: unified({ remarkPlugins: [remarkDocs] }) },
  integrations: [
    starlight({
      title: 'egui-charts',
      description: 'Build financial charts with your own data, in Rust and egui.',
      favicon: '/favicon.svg',
      social: [{ icon: 'github', label: 'GitHub', href: 'https://github.com/userFRM/egui-charts' }],
      editLink: { baseUrl: 'https://github.com/userFRM/egui-charts/edit/main/docs/site/' },
      customCss: ['./src/styles/custom.css'],
      sidebar: [
        { slug: 'index', label: 'Introduction' },
        {
          label: 'Getting started',
          items: ['installation', 'first-chart', 'data-model', 'data'],
        },
        {
          label: 'Features',
          items: [
            'chart-types', 'scales',
            { label: 'Studies', items: ['studies', 'study-data'] },
            'drawings', 'order-flow', 'replay', 'strategy-report', 'host-services',
            'theming', 'application-ui', 'performance',
          ],
        },
        { label: 'Integration reference', items: ['builder-guide', 'api-reference'] },
      ],
    }),
  ],
});
