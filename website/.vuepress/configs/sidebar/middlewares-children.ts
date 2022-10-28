export const middlewaresChildren = function (bookRoot: string) {
  return [
    `${bookRoot}/middlewares/affix.md`,
    `${bookRoot}/middlewares/basic-auth.md`,
    `${bookRoot}/middlewares/cache.md`,
    `${bookRoot}/middlewares/caching-headers.md`,
    `${bookRoot}/middlewares/compression.md`,
    `${bookRoot}/middlewares/cors.md`,
    `${bookRoot}/middlewares/csrf.md`,
    `${bookRoot}/middlewares/flash.md`,
    `${bookRoot}/middlewares/force-https.md`,
    `${bookRoot}/middlewares/jwt-auth.md`,
    `${bookRoot}/middlewares/logging.md`,
    `${bookRoot}/middlewares/proxy.md`,
    `${bookRoot}/middlewares/rate-limiter.md`,
    `${bookRoot}/middlewares/serve-static.md`,
    `${bookRoot}/middlewares/session.md`,
    `${bookRoot}/middlewares/size-limiter.md`,
    `${bookRoot}/middlewares/sse.md`,
    `${bookRoot}/middlewares/timeout.md`,
    `${bookRoot}/middlewares/trailing-slash.md`,
    `${bookRoot}/middlewares/ws.md`,
  ];
}