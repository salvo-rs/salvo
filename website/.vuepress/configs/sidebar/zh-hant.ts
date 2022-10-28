import type { SidebarConfig } from '@vuepress/theme-default'
import { coreChildren } from './core-children'
import { topicsChildren } from './topics-children'
import { middlewaresChildren } from './middlewares-children'

export const sidebarZhHant: SidebarConfig = {
  '/zh-hant/book/': [
    {
      text: 'Book',
      children: [
        '/zh-hant/book/README.md',
        '/zh-hant/book/guide.md',
        {
          text: '核心功能',
          children: coreChildren('/zh-hant/book'),
        },
        {
          text: '專題講解',
          children: topicsChildren('/zh-hant/book'),
        },
        {
          text: '中間件',
          children: middlewaresChildren('/zh-hant/book'),
        }
      ],
    },
  ],
}
