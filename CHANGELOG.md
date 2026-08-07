# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- Changelog established for upcoming releases.
- Opt-in RFC 9457 Problem Details responses and OpenAPI integration via the `rfc9457`
  feature.
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

### Notes

- Documentation UI compatibility with a 3.2 document, smoke-tested against the bundled Swagger
  UI v5.32.11 and the current CDN builds of the others: all four load the document, but only
  Swagger UI renders `query` operations. Scalar, RapiDoc and ReDoc display the rest of the
  document and silently omit them. Their CDN URLs are deliberately left unpinned, since no
  released build of those three renders `query` yet.

### Fixed

- `PathItem` no longer duplicates operations into `extensions` when deserialized, which
  previously made a parsed path item re-serialize with repeated keys.
- OAuth2 flows are now resolved by their flow name instead of by shape, so a
  `clientCredentials` flow no longer deserializes as `Flow::Password`.
- Objects whose optional fields are skipped during serialization can now be deserialized
  again; previously round-tripping a generated document failed with `missing field` errors.

## Historical Releases

Releases published before this changelog was introduced are available in the GitHub Releases page:
<https://github.com/salvo-rs/salvo/releases>
