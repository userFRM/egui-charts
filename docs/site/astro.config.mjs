import { defineConfig } from 'astro/config';
import { unified } from '@astrojs/markdown-remark';
import starlight from '@astrojs/starlight';
import remarkDocs from './remark-docs.mjs';

export default defineConfig({
  site: 'https://userfrm.github.io',
  base: '/egui-charts/docs/',
  // Sidebar topic links already include a slash before their fragment.
  trailingSlash: 'ignore',
  markdown: { processor: unified({ remarkPlugins: [remarkDocs] }) },
  integrations: [
    starlight({
      title: 'egui-charts',
      logo: {
        light: '../images/logo-light.svg',
        dark: '../images/logo-dark.svg',
        replacesTitle: true,
      },
      description: 'Build financial charts with your own data, in Rust and egui.',
      favicon: '/favicon.svg',
      disable404Route: true,
      social: [{ icon: 'github', label: 'GitHub', href: 'https://github.com/userFRM/egui-charts' }],
      editLink: { baseUrl: 'https://github.com/userFRM/egui-charts/edit/main/docs/site/' },
      customCss: ['./src/styles/custom.css'],
      components: {
        Header: './src/components/Header.astro',
        Hero: './src/components/Hero.astro',
        Footer: './src/components/Footer.astro',
      },
      expressiveCode: { themes: ['github-dark', 'github-light'] },
      sidebar: [
        { slug: 'index', label: 'Introduction' },
        { label: 'Status', link: '/progress/' },
        {
          label: 'Learning',
          items: ['installation', 'first-chart', 'tutorial/second-series', { slug: 'builder-guide', label: 'Builder tutorial' }, 'data-model', 'data', 'own-data'],
        },
        {
          label: 'Chart concepts',
          items: [
            'chart-types',
            { slug: 'scales', label: 'Scales and interaction overview' },
            'interaction',
            'price-scale',
            'time-scale',
            'horizontal-scales',
            'panes',
            { label: 'Markers', link: '/api-reference/#markers' },
          ],
        },
        {
          label: 'Studies and extensions',
          items: ['studies', 'study-data', 'drawings', 'order-flow', 'theming', 'time-zones', 'accessibility', 'extensions', 'extensions/custom-series'],
        },
        {
          label: 'Recipes',
          items: [
            { slug: 'recipes', label: 'Integration recipes' },
            'recipes/annotations', 'recipes/price-volume', 'recipes/host-readout',
            'recipes/synchronized', 'recipes/export',
            'application-ui', 'replay', 'strategy-report', 'host-services', 'performance',
          ],
        },
        { label: 'API', items: ['api-reference'] },
        { label: 'Versions', items: ['migration', 'release-notes'] },
      ],
    }),
  ],
});
