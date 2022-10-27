import type { NavbarConfig } from '@vuepress/theme-default'
import { version } from '../meta.js'

export const navbarZhHans: NavbarConfig = [
  {
    text: '教程',
    link: '/zh-hans/book/',
  },
  {
    text: '插件',
    children: [
      {
        text: '常用功能',
        children: [
          '/zh-hans/reference/plugin/back-to-top.md',
          '/zh-hans/reference/plugin/container.md',
          '/zh-hans/reference/plugin/external-link-icon.md',
          '/zh-hans/reference/plugin/google-analytics.md',
          '/zh-hans/reference/plugin/medium-zoom.md',
          '/zh-hans/reference/plugin/nprogress.md',
          '/zh-hans/reference/plugin/register-components.md',
        ],
      },
      {
        text: '内容搜索',
        children: [
          '/zh-hans/reference/plugin/docsearch.md',
          '/zh-hans/reference/plugin/search.md',
        ],
      },
      {
        text: 'PWA',
        children: [
          '/zh-hans/reference/plugin/pwa.md',
          '/zh-hans/reference/plugin/pwa-popup.md',
        ],
      },
      {
        text: '语法高亮',
        children: [
          '/zh-hans/reference/plugin/prismjs.md',
          '/zh-hans/reference/plugin/shiki.md',
        ],
      },
      {
        text: '主题开发',
        children: [
          '/zh-hans/reference/plugin/active-header-links.md',
          '/zh-hans/reference/plugin/git.md',
          '/zh-hans/reference/plugin/palette.md',
          '/zh-hans/reference/plugin/theme-data.md',
          '/zh-hans/reference/plugin/toc.md',
        ],
      },
    ],
  },
  {
    text: '了解更多',
    children: [
      {
        text: '深入',
        children: [
          '/zh-hans/advanced/architecture.md',
          '/zh-hans/advanced/plugin.md',
          '/zh-hans/advanced/theme.md',
          {
            text: 'Cookbook',
            link: '/zh-hans/advanced/cookbook/',
          },
        ],
      },
      {
        text: '其他资源',
        children: [
          '/zh-hans/contributing.md',
          {
            text: 'Awesome VuePress',
            link: 'https://github.com/vuepress/awesome-vuepress',
          },
        ],
      },
    ],
  },
  {
    text: `v${version}`,
    children: [
      {
        text: '更新日志',
        link: 'https://github.com/vuepress/vuepress-next/blob/main/CHANGELOG.md',
      },
      {
        text: 'v1.x',
        link: 'https://v1.vuepress.vuejs.org/zh-hans/',
      },
      {
        text: 'v0.x',
        link: 'https://v0.vuepress.vuejs.org/zh-hans/',
      },
    ],
  },
]
