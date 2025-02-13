OpenAPI support for salvo, modified from [utoipa](https://github.com/juhaku/utoipa), It uses simple proc macros which
you can use to annotate your code to have items documented.

# Crate Features

- **`yaml`** Enables **serde_norway** serialization of OpenAPI objects.

- **`chrono`** Add support for [chrono](https://crates.io/crates/chrono) `DateTime`, `Date`, `NaiveDate` and `Duration`
  types. By default these types are parsed to `string` types with additional `format` information.
  `format: date-time` for `DateTime` and `format: date` for `Date` and `NaiveDate` according
  [RFC3339](https://xml2rfc.ietf.org/public/rfc/html/rfc3339.html#anchor14) as `ISO-8601`. To
  override default `string` representation users have to use `value_type` attribute to override the type.
  See [docs](https://docs.rs/salvo_oapi/latest/salvo_oapi/derive.ToSchema.html) for more details.

- **`time`** Add support for [time](https://crates.io/crates/time) `OffsetDateTime`, `PrimitiveDateTime`, `Date`, and `Duration` types. By default these types are parsed as `string`. `OffsetDateTime` and `PrimitiveDateTime` will use `date-time` format. `Date` will use `date` format and `Duration` will not have any format. To override default `string` representation users have to use `value_type` attribute to override the type. See [docs](https://docs.rs/salvo_oapi/latest/salvo_oapi/derive.ToSchema.html) for more details.

- **`decimal`** Add support for [rust_decimal](https://crates.io/crates/rust_decimal) `Decimal` type. **By default** it is interpreted as `String`. If you wish to change the format you need to override the type. See the `value_type` in [`ToSchema` derive docs][to_schema_derive].

- **`decimal-float`** Add support for [rust_decimal](https://crates.io/crates/rust_decimal) `Decimal` type. **By default** it is interpreted as `Number`. This feature is mutually exclusive with **decimal** and allow to change the default type used in your documentation for `Decimal` much like `serde_with_float` feature exposed by rust_decimal.

- **`uuid`** Add support for [uuid](https://github.com/uuid-rs/uuid). `Uuid` type will be presented as `String` with format `uuid` in OpenAPI spec.

- **`ulid`** Add support for [ulid](https://github.com/dylanhart/ulid-rs). `Ulid` type will be presented as `String` with format `ulid` in OpenAPI spec.

- **`url`** Add support for [url](https://github.com/servo/rust-url). `Url` type will be presented as `String` with format `uri` in OpenAPI spec.

- **`smallvec`** Add support for [smallvec](https://crates.io/crates/smallvec). `SmallVec` will be treated as `Vec`.

- **`indexmap`** Add support for [indexmap](https://crates.io/crates/indexmap). When enabled `IndexMap` will be rendered as a map similar to `BTreeMap` and `HashMap`.

# Go beyond the surface

- Browse to [examples](https://github.com/salvo-rs/salvo/tree/master/examples) for more comprehensive examples.
- Check [`derive@ToResponses`] and [`derive@ToResponse`] for examples on deriving responses.
- More about OpenAPI security in [security documentation][security].

[path]: attr.path.html
[serde]: derive.ToSchema.html#partial-serde-attributes-support
[security]: openapi/security/index.html
[to_schema_derive]: derive.ToSchema.html
