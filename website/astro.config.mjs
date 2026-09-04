import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
  site: 'https://orouta.dev',
  outDir: 'dist',
  publicDir: 'public',
  integrations: [
    starlight({
      title: 'orouta',
      description: 'Several Ollama hosts behind one port. The model name picks the host.',
      favicon: '/favicon.ico',
      logo: {
        src: '../docs/logo.png',
        alt: 'orouta',
        replacesTitle: true,
      },
      social: [
        {
          icon: 'github',
          label: 'GitHub',
          href: 'https://github.com/pmdroid/orouta',
        },
      ],
      customCss: ['./src/styles/starlight.css'],
      sidebar: [
        {
          label: 'Start',
          items: [
            { label: 'Docs', link: '/docs/' },
            { label: 'Install', link: '/docs/install/' },
            { label: 'Config', link: '/docs/config/' },
            { label: 'API', link: '/docs/api/' },
          ],
        },
      ],
    }),
  ],
});
