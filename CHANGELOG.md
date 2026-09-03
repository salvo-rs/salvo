# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Security

- Static file responses no longer serve XML-based documents inline, which allowed stored XSS
  when an application served attacker-supplied uploads. `image/svg+xml` and `text/xml` were
  classified as `inline` by their top-level `image`/`text` type, so an uploaded SVG carrying
  `<script>` — or an XML document naming an XSLT stylesheet through `<?xml-stylesheet ?>` —
  executed script in the serving origin when opened. XML-based content types now default to
  `attachment`, matching how `application/xml` and `application/xhtml+xml` already behaved.
  That covers the `+xml` suffix, an `xml` subtype, RFC 7303's `xml-dtd` and
  `xml-external-parsed-entity`, and the legacy `text/xsl` that `<?xml-stylesheet ?>` itself
  names. `text/html` still defaults to `inline`. Reported by sl91994.
- `NamedFile` and `StaticEmbed` responses now carry `X-Content-Type-Options: nosniff`.
- `salvo-otel` no longer records the request URI, so query strings holding access tokens,
  signed-URL signatures or session identifiers stop being copied into metric dimensions and
  span attributes. `Metrics` dropped `url.full` entirely, and `Tracing` replaced it with
  `url.path` plus a `url.query` whose `sig`, `Signature`, `AWSAccessKeyId` and
  `X-Goog-Signature` values are redacted, as the semantic conventions require.

### Added

- `NamedFileBuilder::use_content_type_options` and `NamedFile::use_content_type_options` to
  control the `X-Content-Type-Options` header.
- `StaticDir::disposition_type`, `StaticDir::use_content_type_options`,
  `StaticFile::disposition_type`, `StaticFile::attached_name` and
  `StaticFile::use_content_type_options`, so applications serving trusted assets can opt back
  into inline rendering, which previously had no public API on either handler.
- `NamedFileBuilder::disposition_name`, which sets the name `Content-Disposition` reports
  without forcing the disposition to `attachment` the way `attached_name` does.
- Cookie extractors now support structured JSON values. `CookieParam<T>` falls back to JSON when
  scalar conversion fails, while `#[derive(Extractible)]` accepts explicit cookie field sources
  such as `#[salvo(extract(source(from = "cookie", parse = "json")))]`.
- Changelog established for upcoming releases.
- Opt-in RFC 9457 Problem Details responses with typed extension members and OpenAPI integration
  via the `rfc9457` feature.
- Initial opt-in OpenAPI 3.2 document support in `salvo-oapi`, including the `$self` field.
- OpenAPI 3.2 object model in `salvo-oapi`:
  - Tag Object `summary`, `parent` and `kind`.
  - Server Object `name`.
  - Path Item Object `query` (via `PathItemType::Query`) and `additionalOperations`.
  - Parameter Object `in: querystring` (`ParameterIn::QueryString`) and `style: cookie`
    (`ParameterStyle::Cookie`).
  - Media Type Object `itemSchema`, `prefixEncoding` and `itemEncoding`.
  - Encoding Object nested `encoding`, `prefixEncoding` and `itemEncoding`.
  - Example Object `dataValue` and `serializedValue`.
  - Response Object `summary`, and `description` is now optional on input.
  - Components Object `mediaTypes`.
  - Discriminator Object `defaultMapping`.
  - XML Object `nodeType` (new `XmlNodeType` enum).
  - Security Scheme Object `deprecated` and `oauth2MetadataUrl`.
  - OAuth Flows Object `deviceAuthorization` (new `DeviceAuthorization` flow).
  - Info Object `summary`.
- Header Object gained the rest of its spec surface: `required`, `deprecated`, `style`,
  `explode`, `example`, `examples`, `content` and extensions, plus `Header::with_content`.
- Route discovery emits `QUERY` operations and custom HTTP methods (via
  `additionalOperations`) when the document declares OpenAPI 3.2. Documents that still declare
  3.1 skip such routes and log a warning explaining how to opt in.
- `#[endpoint]` parameters accept the `QueryString` location and the `Cookie` style.
- `Content::from_ref` / `Content::ref_location`, so a `content` map entry can reference a
  reusable Media Type Object without changing those maps to `RefOr<Content>`.
- `examples/oapi-3-2` demonstrates emitting a 3.2 document with a `QUERY` route.
- `salvo_core::fs::extension_content_encoding`, which reports the content coding a file
  extension implies, so a handler choosing a file to serve can tell that it already carries one.
- `salvo-otel` records `http.server.active_requests`, `http.server.request.body.size` and
  `http.server.response.body.size`, which its documentation already described. The in-flight
  gauge is decremented from a drop guard, so a cancelled request — a client disconnect or a
  timeout — does not leave it drifting upwards.
- `Metrics::with_known_methods` and `Tracing::with_known_methods` widen the set of request
  methods reported verbatim in `http.request.method`, for applications serving methods beyond
  RFC 9110 and RFC 5789 (WebDAV's `PROPFIND`, say).
- `salvo-otel` gained a `matched-path` feature, on by default, which supplies `http.route`.
  Through the `salvo` crate it follows that crate's own `matched-path` feature.

### Changed

- **Breaking.** Several public enums and structs gained variants or fields, which can break
  downstream exhaustive matches and struct literals even though the wire format change is
  purely additive:
  - `PathItemType::Query`, `ParameterIn::QueryString`, `ParameterStyle::Cookie` and
    `Flow::DeviceAuthorization` are new variants.
  - `SecurityScheme::MutualTls` gained a `deprecated` field.
  - `Header::schema` is now `Option<RefOr<Schema>>` so a header can describe its value with
    `content` instead. `Header::default()` still yields a `String` schema, so serialized output
    is unchanged.
- `OpenApi` still defaults to emitting OpenAPI 3.1; 3.2 remains opt-in via
  `OpenApi::openapi_version`.
- **Breaking.** `salvo-otel`'s `Metrics` middleware now emits the instruments the
  OpenTelemetry HTTP semantic conventions define, replacing names it had invented. Dashboards
  and alerts built on the old names have to be repointed:
  - `salvo_request_duration_ms` became `http.server.request.duration`, and it measures
    **seconds** rather than milliseconds, with the bucket boundaries the conventions
    recommend. A Prometheus exporter renders it as `http_server_request_duration_seconds`.
  - `salvo_request_count` and `salvo_error_count` are gone. The conventions define no such
    instruments: the duration histogram already carries the request count
    (`http_server_request_duration_seconds_count`), and failures are selected by filtering on
    `error.type` or `http.response.status_code`.
  - The instruments are registered under the `salvo-otel` instrumentation scope, versioned
    and carrying the semantic conventions' schema URL, instead of a bare `salvo` meter.
- **Breaking.** `salvo-otel`'s metric dimensions changed to the ones the conventions list, so
  the number of time series is now proportional to the number of routes instead of growing
  with traffic:
  - `url.full` — one series per distinct URI, query string included — was replaced by
    `http.route`, the matched route template (`/users/{id}`). Requests that matched no route
    carry no `http.route` at all rather than a fallback, except a request for `/`, which is
    reported as the root route because salvo reports a root-mounted goal and a request that
    matched nothing with the same empty matched path.
  - `exception.message` was replaced by `error.type`, which carries the status code of a
    server error. The old attribute was unbounded, and it was appended to the labels of
    `salvo_request_count` and `salvo_request_duration_ms` but not of `salvo_error_count`, so
    a failed request produced a different label set than a successful one on the same metric.
  - A request method outside RFC 9110 and RFC 5789 is now reported as `_OTHER`, so a client
    cannot open a time series per made-up method name. See `Metrics::with_known_methods`.
  - `url.scheme` and `network.protocol.version` were added. A request target given in absolute
    form lets the client choose the scheme, so a scheme other than `http` or `https` is
    reported as `_OTHER` for the same reason an unknown method is; spans still record it.
- **Breaking.** `salvo-otel`'s `Tracing` middleware follows the same conventions:
  - Spans are named `{method} {route}` — `GET /users/{id}` — instead of `{method} {uri}`,
    which made every distinct URI its own span name.
  - `url.full` gave way to `url.path`, a redacted `url.query` and `url.scheme`, and
    `http.route` was added.
  - `network.protocol.version` reports `1.1` and `2` rather than the `HTTP/1.1` form
    `Version`'s `Debug` output produces.
  - An unknown request method is reported as `_OTHER`, with the value the client sent kept in
    `http.request.method_original`.
  - A `5xx` response now sets `error.type` and an error span status. A `4xx` does not: it is a
    valid outcome for a server span.
  - `http.response.header.content-length`, which was not an attribute the conventions define,
    became `http.response.body.size`, and it is left out for a response whose body the
    catcher fills in after middleware returns rather than reported as zero.
  - `client.address` reports the peer's IP (`198.51.100.4`) with the port split out into
    `client.port`, instead of salvo's `socket://198.51.100.4:54321` display form.
  - The `telemetry.sdk.name`, `telemetry.sdk.version` and `telemetry.sdk.language` attributes
    were removed from every span. They describe the resource, and the SDK already reports them
    there.
- **Behavior change.** Serving an SVG or XML file through `StaticDir`, `StaticFile` or
  `NamedFile` now sends `Content-Disposition: attachment` by default, so following a link to
  one downloads it instead of rendering it, and `<object>`/`<embed>`/`<iframe>` no longer
  display it. Referencing the same file from `<img>`, CSS `url()` or `<use>` is unaffected,
  because browsers ignore `Content-Disposition` on subresource loads. Directories holding only
  trusted assets can restore the old behavior with
  `StaticDir::new(..).disposition_type("inline")`. `text/html` still defaults to `inline`,
  since serving HTML documents is the point of a static file server.

### Notes

- Documentation UI compatibility with a 3.2 document, smoke-tested against the bundled Swagger
  UI v5.32.11 and the current CDN builds of the others: all four load the document, but only
  Swagger UI renders `query` operations. Scalar, RapiDoc and ReDoc display the rest of the
  document and silently omit them. Their CDN URLs are deliberately left unpinned, since no
  released build of those three renders `query` yet.

### Fixed

- `StaticDir` names a download after the file that was requested rather than the precompressed
  sidecar it was served from, so a request for `logo.svg` answered out of `logo.svg.br` no
  longer offers `Content-Disposition: attachment; filename="logo.svg.br"`.
- `PathItem` no longer duplicates operations into `extensions` when deserialized, which
  previously made a parsed path item re-serialize with repeated keys.
- OAuth2 flows are now resolved by their flow name instead of by shape, so a
  `clientCredentials` flow no longer deserializes as `Flow::Password`.
- Objects whose optional fields are skipped during serialization can now be deserialized
  again; previously round-tripping a generated document failed with `missing field` errors.
- `.svgz` files are now served with `Content-Encoding: gzip`. Their extension names both a
  media type and the coding applied to it, but an extension resolves to a single media type,
  so the response described the gzip stream as `image/svg+xml` and no client could render it.
  The gzipped X3D forms `.x3dz`, `.x3dvz` and `.x3dbz` are handled the same way. `.gz` and
  `.tgz` are unaffected, since there the gzip stream is the representation being served.
  `StaticDir` also stops serving a precompressed sidecar for such a file — a `logo.svgz.br`
  stacks a second coding on the gzip, and only the outer one can be reported.

## Historical Releases

Releases published before this changelog was introduced are available in the GitHub Releases page:
<https://github.com/salvo-rs/salvo/releases>
