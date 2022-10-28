import type { NavbarConfig } from '@vuepress/theme-default'
import { version } from '../meta.js'

export const navbarZhHant: NavbarConfig = [
  {
    text: '葵花寶典',
    link: '/zh-hant/book/',
  },
  {
    text: '中間件',
    children: [
      {
        text: '常用功能',
        children: [
          '/zh-hant/book/middlewares/affix.md',
          '/zh-hant/book/middlewares/compression.md',
          '/zh-hant/book/middlewares/flash.md',
          '/zh-hant/book/middlewares/force-https.md',
          '/zh-hant/book/middlewares/logging.md',
          '/zh-hant/book/middlewares/proxy.md',
          '/zh-hant/book/middlewares/serve-static.md',
          '/zh-hant/book/middlewares/session.md',
          '/zh-hant/book/middlewares/sse.md',
          '/zh-hant/book/middlewares/timeout.md',
          '/zh-hant/book/middlewares/trailing-slash.md',
          '/zh-hant/book/middlewares/ws.md',
        ],
      },
      {
        text: '用戶驗證',
        children: [
          '/zh-hant/book/middlewares/basic-auth.md',
          '/zh-hant/book/middlewares/jwt-auth.md',
        ],
      },
      {
        text: '安全防護',
        children: [
          '/zh-hant/book/middlewares/cors.md',
          '/zh-hant/book/middlewares/csrf.md',
          '/zh-hant/book/middlewares/rate-limiter.md',
        ],
      },
      {
        text: '數據緩存',
        children: [
          '/zh-hant/book/middlewares/cache.md',
          '/zh-hant/book/middlewares/caching-headers.md',
        ],
      },
    ],
  },
  {
    text: '資助項目',
    link: '/zh-hant/donate.md',
  },
]
