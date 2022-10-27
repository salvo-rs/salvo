import type { SidebarConfig } from '@vuepress/theme-default'

export const sidebarEn: SidebarConfig = {
  '/book/': [
    {
      text: 'Book',
      children: [
        '/book/README.md',
        '/book/getting-started.md',
        '/book/configuration.md',
        '/book/page.md',
        '/book/markdown.md',
        '/book/assets.md',
        '/book/i18n.md',
        '/book/deployment.md',
        '/book/theme.md',
        '/book/plugin.md',
        '/book/bundler.md',
        '/book/migration.md',
      ],
    },
  ],
  '/advanced/': [
    {
      text: 'Advanced',
      children: [
        '/advanced/architecture.md',
        '/advanced/plugin.md',
        '/advanced/theme.md',
      ],
    },
    {
      text: 'Cookbook',
      children: [
        '/advanced/cookbook/README.md',
        '/advanced/cookbook/usage-of-client-config.md',
        '/advanced/cookbook/adding-extra-pages.md',
        '/advanced/cookbook/making-a-theme-extendable.md',
        '/advanced/cookbook/passing-data-to-client-code.md',
        '/advanced/cookbook/markdown-and-vue-sfc.md',
      ],
    },
  ],
  '/reference/': [
    {
      text: 'VuePress Reference',
      collapsible: true,
      children: [
        '/reference/cli.md',
        '/reference/config.md',
        '/reference/frontmatter.md',
        '/reference/components.md',
        '/reference/plugin-api.md',
        '/reference/theme-api.md',
        '/reference/client-api.md',
        '/reference/node-api.md',
      ],
    },
    {
      text: 'Bundlers Reference',
      collapsible: true,
      children: ['/reference/bundler/vite.md', '/reference/bundler/webpack.md'],
    },
    {
      text: 'Default Theme Reference',
      collapsible: true,
      children: [
        '/reference/default-theme/config.md',
        '/reference/default-theme/frontmatter.md',
        '/reference/default-theme/components.md',
        '/reference/default-theme/markdown.md',
        '/reference/default-theme/styles.md',
        '/reference/default-theme/extending.md',
      ],
    },
    {
      text: 'Official Plugins Reference',
      collapsible: true,
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
  ],
}
