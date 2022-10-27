import type { SidebarConfig } from '@vuepress/theme-default'

export const sidebarZhHans: SidebarConfig = {
  '/zh-hans/book/': [
    {
      text: '教程',
      children: [
        '/zh-hans/book/README.md',
        '/zh-hans/book/getting-started.md',
        '/zh-hans/book/configuration.md',
        '/zh-hans/book/page.md',
        '/zh-hans/book/markdown.md',
        '/zh-hans/book/assets.md',
        '/zh-hans/book/i18n.md',
        '/zh-hans/book/deployment.md',
        '/zh-hans/book/theme.md',
        '/zh-hans/book/plugin.md',
        '/zh-hans/book/bundler.md',
        '/zh-hans/book/migration.md',
      ],
    },
  ],
  '/zh-hans/reference/': [
    {
      text: '官方插件参考',
      collapsible: true,
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
  ],
}
