import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
  site: 'https://orouta.dev',
  outDir: 'dist',
  publicDir: 'public',
  server: { host: '0.0.0.0' },
  preview: { host: '0.0.0.0' },
  integrations: [
    starlight({
      title: 'orouta',
      description: 'Several Ollama hosts behind one port. The model name picks the host.',
      favicon: '/favicon.ico',
      social: [
        {
          icon: 'github',
          label: 'GitHub',
          href: 'https://github.com/pmdroid/orouta',
        },
      ],
      customCss: ['./src/styles/starlight.css'],
      components: { Head: './src/components/StarlightHead.astro' },
      expressiveCode: {
        themes: ['everforest-dark', 'everforest-light'],
      },
      sidebar: [
        {
          label: 'Start',
          items: [
            { label: 'Docs', link: '/docs/' },
            { label: 'Install', link: '/docs/install/' },
            { label: 'Config', link: '/docs/config/' },
            { label: 'Expose Ollama', link: '/docs/ollama-host/' },
            { label: 'API', link: '/docs/api/' },
            { label: 'Roadmap', link: '/docs/roadmap/' },
          ],
        },
      ],
    }),
  ],
});
