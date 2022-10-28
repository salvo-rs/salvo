import type { NavbarConfig } from '@vuepress/theme-default'
import { version } from '../meta.js'

export const navbarZhHans: NavbarConfig = [
  {
    text: '葵花宝典',
    link: '/zh-hans/book/',
  },
  {
    text: '中间件',
    children: [
      {
        text: '常用功能',
        children: [
          '/zh-hans/book/middlewares/affix.md',
          '/zh-hans/book/middlewares/compression.md',
          '/zh-hans/book/middlewares/flash.md',
          '/zh-hans/book/middlewares/force-https.md',
          '/zh-hans/book/middlewares/logging.md',
          '/zh-hans/book/middlewares/proxy.md',
          '/zh-hans/book/middlewares/serve-static.md',
          '/zh-hans/book/middlewares/session.md',
          '/zh-hans/book/middlewares/sse.md',
          '/zh-hans/book/middlewares/timeout.md',
          '/zh-hans/book/middlewares/trailing-slash.md',
          '/zh-hans/book/middlewares/ws.md',
        ],
      },
      {
        text: '用户验证',
        children: [
          '/zh-hans/book/middlewares/basic-auth.md',
          '/zh-hans/book/middlewares/jwt-auth.md',
        ],
      },
      {
        text: '安全防护',
        children: [
          '/zh-hans/book/middlewares/cors.md',
          '/zh-hans/book/middlewares/csrf.md',
          '/zh-hans/book/middlewares/rate-limiter.md',
        ],
      },
      {
        text: '数据缓存',
        children: [
          '/zh-hans/book/middlewares/cache.md',
          '/zh-hans/book/middlewares/caching-headers.md',
        ],
      },
    ],
  },
  {
    text: '资助项目',
    link: '/zh-hans/donate.md',
  },
]
