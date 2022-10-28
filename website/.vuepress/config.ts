import process from 'node:process'
import { viteBundler } from '@vuepress/bundler-vite'
import { webpackBundler } from '@vuepress/bundler-webpack'
import { defineUserConfig } from '@vuepress/cli'
import { docsearchPlugin } from '@vuepress/plugin-docsearch'
import { googleAnalyticsPlugin } from '@vuepress/plugin-google-analytics'
import { registerComponentsPlugin } from '@vuepress/plugin-register-components'
import { shikiPlugin } from '@vuepress/plugin-shiki'
import { defaultTheme } from '@vuepress/theme-default'
import { getDirname, path } from '@vuepress/utils'
import {
  head,
  navbarEn,
  navbarZhHans,
  navbarZhHant,
  sidebarEn,
  sidebarZhHans,
  sidebarZhHant,
} from './configs/index.js'

const __dirname = getDirname(import.meta.url)
const isProd = process.env.NODE_ENV === 'production'

export default defineUserConfig({
  // set site base to default value
  base: '/',

  // extra tags in `<head>`
  head,

  // site-level locales config
  locales: {
    '/': {
      lang: 'en',
      title: 'Salvo',
      description: 'Salvo is a powerful and simplest web server framework in Rust world',
    },
    '/zh-hans/': {
      lang: 'zh-hans',
      title: 'Salvo',
      description: '一个功能强大且简单的 Rust Web 服务端框架',
    },
    '/zh-hant/': {
      lang: 'zh-hant',
      title: 'Salvo',
      description: '一個功能強大且簡單的 Rust Web 服務端框架',
    },
  },

  // specify bundler via environment variable
  bundler:
    process.env.DOCS_BUNDLER === 'webpack' ? webpackBundler() : viteBundler(),

  // configure default theme
  theme: defaultTheme({
    logo: '/images/logo-text-h.svg',
    repo: 'salvo-rs/salvo',
    docsDir: 'book',

    // theme-level locales config
    locales: {
      /**
       * English locale config
       *
       * As the default locale of @vuepress/theme-default is English,
       * we don't need to set all of the locale fields
       */
      '/': {
        // navbar
        navbar: navbarEn,
        // sidebar
        sidebar: sidebarEn,
        // page meta
        editLinkText: 'Edit this page on GitHub',
      },

      /**
       * Chinese locale config
       */
      '/zh-hans/': {
        // navbar
        navbar: navbarZhHans,
        selectLanguageName: '简体中文',
        selectLanguageText: '选择语言',
        selectLanguageAriaLabel: '选择语言',
        // sidebar
        sidebar: sidebarZhHans,
        // page meta
        editLinkText: '在 GitHub 上编辑此页',
        lastUpdatedText: '上次更新',
        contributorsText: '贡献者',
        // custom containers
        tip: '提示',
        warning: '注意',
        danger: '警告',
        // 404 page
        notFound: [
          '这里什么都没有',
          '我们怎么到这来了？',
          '这是一个 404 页面',
          '看起来我们进入了错误的链接',
        ],
        backToHome: '返回首页',
        // a11y
        openInNewWindow: '在新窗口打开',
        toggleColorMode: '切换颜色模式',
        toggleSidebar: '切换侧边栏',
      },
      '/zh-hant/': {
        // navbar
        navbar: navbarZhHant,
        selectLanguageName: '簡體中文',
        selectLanguageText: '選擇語言',
        selectLanguageAriaLabel: '選擇語言',
        // sidebar
        sidebar: sidebarZhHant,
        // page meta
        editLinkText: '在 GitHub 上編輯此頁',
        lastUpdatedText: '上次更新',
        contributorsText: '貢獻者',
        // custom containers
        tip: '提示',
        warning: '註意',
        danger: '警告',
        // 404 page
        notFound: [
          '這裏什麼都冇有',
          '我們怎麼到這來了？',
          '這是一個 404 頁麵',
          '看起來我們進入了錯誤的鏈接',
        ],
        backToHome: '返回首頁',
        // a11y
        openInNewWindow: '在新窗口打開',
        toggleColorMode: '切換顔色模式',
        toggleSidebar: '切換側邊欄',
      },
    },

    themePlugins: {
      // only enable git plugin in production mode
      git: isProd,
      // use shiki plugin in production mode instead
      prismjs: !isProd,
    },
  }),

  // configure markdown
  markdown: {
    importCode: {
      handleImportPath: (str) =>
        str.replace(/^@vuepress/, path.resolve(__dirname, '../../ecosystem')),
    },
  },

  // use plugins
  plugins: [
    docsearchPlugin({
      appId: '34YFD9IUQ2',
      apiKey: '9a9058b8655746634e01071411c366b8',
      indexName: 'salvo.rs',
      searchParameters: {
        facetFilters: ['tags:v2'],
      },
      locales: {
        '/zh-hans/': {
          placeholder: '搜索文档',
          translations: {
            button: {
              buttonText: '搜索文档',
              buttonAriaLabel: '搜索文档',
            },
            modal: {
              searchBox: {
                resetButtonTitle: '清除查询条件',
                resetButtonAriaLabel: '清除查询条件',
                cancelButtonText: '取消',
                cancelButtonAriaLabel: '取消',
              },
              startScreen: {
                recentSearchesTitle: '搜索历史',
                noRecentSearchesText: '没有搜索历史',
                saveRecentSearchButtonTitle: '保存至搜索历史',
                removeRecentSearchButtonTitle: '从搜索历史中移除',
                favoriteSearchesTitle: '收藏',
                removeFavoriteSearchButtonTitle: '从收藏中移除',
              },
              errorScreen: {
                titleText: '无法获取结果',
                helpText: '你可能需要检查你的网络连接',
              },
              footer: {
                selectText: '选择',
                navigateText: '切换',
                closeText: '关闭',
                searchByText: '搜索提供者',
              },
              noResultsScreen: {
                noResultsText: '无法找到相关结果',
                suggestedQueryText: '你可以尝试查询',
                reportMissingResultsText: '你认为该查询应该有结果？',
                reportMissingResultsLinkText: '点击反馈',
              },
            },
          },
        },
        '/zh-hant/': {
          placeholder: '搜索文檔',
          translations: {
            button: {
              buttonText: '搜索文檔',
              buttonAriaLabel: '搜索文檔',
            },
            modal: {
              searchBox: {
                resetButtonTitle: '清除查詢條件',
                resetButtonAriaLabel: '清除查詢條件',
                cancelButtonText: '取消',
                cancelButtonAriaLabel: '取消',
              },
              startScreen: {
                recentSearchesTitle: '搜索曆史',
                noRecentSearchesText: '冇有搜索曆史',
                saveRecentSearchButtonTitle: '保存至搜索曆史',
                removeRecentSearchButtonTitle: '從搜索曆史中移除',
                favoriteSearchesTitle: '收藏',
                removeFavoriteSearchButtonTitle: '從收藏中移除',
              },
              errorScreen: {
                titleText: '無法獲取結果',
                helpText: '你可能需要檢查你的網絡連接',
              },
              footer: {
                selectText: '選擇',
                navigateText: '切換',
                closeText: '關閉',
                searchByText: '搜索提供者',
              },
              noResultsScreen: {
                noResultsText: '無法找到相關結果',
                suggestedQueryText: '你可以嘗試查詢',
                reportMissingResultsText: '你認為該查詢應該有結果？',
                reportMissingResultsLinkText: '點擊反饋',
              },
            },
          },
        },
      },
    }),
    googleAnalyticsPlugin({
      // we have multiple deployments, which would use different id
      id: process.env.DOCS_GA_ID ?? '',
    }),
    registerComponentsPlugin({
      componentsDir: path.resolve(__dirname, './components'),
    }),
    // only enable shiki plugin in production mode
    isProd ? shikiPlugin({ theme: 'dark-plus' }) : [],
  ],
})
