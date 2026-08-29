// @ts-check
import { defineConfig } from 'astro/config';
import sitemap from '@astrojs/sitemap';
import starlight from '@astrojs/starlight';

export default defineConfig({
  site: 'https://tsk-gules.vercel.app',
  integrations: [
    sitemap(),
    starlight({
      title: 'tsk',
      description: 'A task board for your terminal.',
      favicon: '/icon-dark.svg',
      logo: {
        light: './src/assets/wordmark-light.svg',
        dark: './src/assets/wordmark-dark.svg',
        alt: 'tsk',
        replacesTitle: true,
      },
      social: [
        {
          icon: 'github',
          label: 'GitHub',
          href: 'https://github.com/smarzban/herdr-tsk',
        },
      ],
      editLink: {
        baseUrl: 'https://github.com/smarzban/herdr-tsk/edit/main/site/',
      },
      lastUpdated: true,
      customCss: ['./src/styles/starlight.css'],
      components: {
        ThemeSelect: './src/components/ThemeSelect.astro',
      },
      sidebar: [
        {
          label: 'Start here',
          items: [
            { label: 'Overview', slug: 'docs' },
            { label: 'Install', slug: 'docs/install' },
            { label: 'Board', slug: 'docs/board' },
            { label: 'Keys', slug: 'docs/keys' },
            { label: 'Capture', slug: 'docs/capture' },
            { label: 'CLI', slug: 'docs/cli' },
          ],
        },
      ],
      head: [
        {
          tag: 'script',
          attrs: { src: '/theme.js' },
        },
        {
          // Dark by default; respects saved tsk-theme / starlight-theme.
          tag: 'script',
          content: `(function(){var k='tsk-theme';var t=null;try{t=localStorage.getItem(k)||localStorage.getItem('starlight-theme');}catch(e){}if(t!=='light'&&t!=='dark')t='dark';document.documentElement.dataset.theme=t;document.documentElement.style.colorScheme=t;try{localStorage.setItem(k,t);localStorage.setItem('starlight-theme',t);}catch(e){}})();`,
        },
        {
          tag: 'link',
          attrs: { rel: 'preconnect', href: 'https://fonts.googleapis.com' },
        },
        {
          tag: 'link',
          attrs: {
            rel: 'preconnect',
            href: 'https://fonts.gstatic.com',
            crossorigin: true,
          },
        },
        {
          tag: 'link',
          attrs: {
            rel: 'stylesheet',
            href: 'https://fonts.googleapis.com/css2?family=IBM+Plex+Mono:wght@400;500&family=Syne:wght@600;700;800&display=swap',
          },
        },
        {
          tag: 'link',
          attrs: {
            rel: 'icon',
            href: '/icon-dark.svg',
            type: 'image/svg+xml',
            media: '(prefers-color-scheme: dark)',
          },
        },
        {
          tag: 'link',
          attrs: {
            rel: 'icon',
            href: '/icon-light.svg',
            type: 'image/svg+xml',
            media: '(prefers-color-scheme: light)',
          },
        },
        {
          tag: 'link',
          attrs: { rel: 'apple-touch-icon', href: '/apple-touch-icon.png' },
        },
        {
          tag: 'link',
          attrs: {
            rel: 'apple-touch-icon',
            href: '/apple-touch-icon-dark.png',
            media: '(prefers-color-scheme: dark)',
          },
        },
        {
          tag: 'link',
          attrs: {
            rel: 'apple-touch-icon',
            href: '/apple-touch-icon-light.png',
            media: '(prefers-color-scheme: light)',
          },
        },
      ],
    }),
  ],
});
