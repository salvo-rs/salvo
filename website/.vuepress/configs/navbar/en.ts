import type { NavbarConfig } from '@vuepress/theme-default'
import { version } from '../meta.js'

export const navbarEn: NavbarConfig = [
  {
    text: 'Book',
    link: '/book/',
  },
  {
    text: 'Middlewares',
    children: [
      {
        text: 'Common Features',
        children: [
          '/reference/plugin/back-to-top.md',
          '/reference/plugin/container.md',
          '/reference/plugin/external-link-icon.md',
          '/reference/plugin/google-analytics.md',
          '/reference/plugin/medium-zoom.md',
          '/reference/plugin/nprogress.md',
          '/reference/plugin/register-components.md',
        ],
      },
      {
        text: 'Content Search',
        children: [
          '/reference/plugin/docsearch.md',
          '/reference/plugin/search.md',
        ],
      },
      {
        text: 'PWA',
        children: [
          '/reference/plugin/pwa.md',
          '/reference/plugin/pwa-popup.md',
        ],
      },
      {
        text: 'Syntax Highlighting',
        children: [
          '/reference/plugin/prismjs.md',
          '/reference/plugin/shiki.md',
        ],
      },
      {
        text: 'Theme Development',
        children: [
          '/reference/plugin/active-header-links.md',
          '/reference/plugin/git.md',
          '/reference/plugin/palette.md',
          '/reference/plugin/theme-data.md',
          '/reference/plugin/toc.md',
        ],
      },
    ],
  },
  {
    text: `v${version}`,
    children: [
      {
        text: 'Changelog',
        link: 'https://github.com/vuepress/vuepress-next/blob/main/CHANGELOG.md',
      },
      {
        text: 'v1.x',
        link: 'https://v1.vuepress.vuejs.org',
      },
      {
        text: 'v0.x',
        link: 'https://v0.vuepress.vuejs.org',
      },
    ],
  },
]
