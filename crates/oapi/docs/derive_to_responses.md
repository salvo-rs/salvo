Generate responses with status codes what
can be attached to the [`salvo_oapi::endpoint`][path_to_responses].

This is `#[derive]` implementation of [`ToResponses`][to_responses] trait. [`derive@ToResponses`]
can be used to decorate _`structs`_ and _`enums`_ to generate response maps that can be used in
[`salvo_oapi::endpoint`][path_to_responses]. If _`struct`_ is decorated with [`derive@ToResponses`] it will be
used to create a map of responses containing single response. Decorating _`enum`_ with
[`derive@ToResponses`] will create a map of responses with a response for each variant of the _`enum`_.

Named field _`struct`_ decorated with [`derive@ToResponses`] will create a response with inlined schema
generated from the body of the struct. This is a conveniency which allows users to directly
create responses with schemas without first creating a separate [response][to_response] type.

Unit _`struct`_ behaves similarly to then named field struct. Only difference is that it will create
a response without content since there is no inner fields.

Unnamed field _`struct`_ decorated with [`derive@ToResponses`] will by default create a response with
referenced [schema][to_schema] if field is object or schema if type is [primitive
type][primitive]. _`#[salvo(schema(...))]`_ attribute at field of unnamed _`struct`_ can be used to inline
the schema if type of the field implements [`ToSchema`][to_schema] trait. Alternatively
_`#[to_response]`_ and _`#[ref_response]`_ can be used at field to either reference a reusable
[response][to_response] or inline a reusable [response][to_response]. In both cases the field
type is expected to implement [`ToResponse`][to_response] trait.


Enum decorated with [`derive@ToResponses`] will create a response for each variant of the _`enum`_.
Each variant must have it's own _`#[salvo(response(...))]`_ definition. Unit variant will behave same
as unit _`struct`_ by creating a response without content. Similarly named field variant and
unnamed field variant behaves the same as it was named field _`struct`_ and unnamed field
_`struct`_.

_`#[response]`_ attribute can be used at named structs, unnamed structs, unit structs and enum
variants to alter [response attributes](#intoresponses-response-attributes) of responses.

Doc comment on a _`struct`_ or _`enum`_ variant will be used as a description for the response.
It can also be overridden with _`description = "..."`_ attribute.

# ToResponses `#[salvo(response(...))]` attributes

* `status = ...` Must be provided. Is either a valid http status code integer. E.g. _`200`_ or a
  string value representing a range such as _`"4XX"`_ or `"default"` or a valid _`http::status::StatusCode`_.
  _`StatusCode`_ can either be use path to the status code or _status code_ constant directly.

* `description = "..."` Define description for the response as str. This can be used to
  override the default description resolved from doc comments if present.

* `content_type = "..." | content_type = [...]` Can be used to override the default behavior of auto resolving the content type
  from the `body` attribute. If defined the value should be valid content type such as
  _`application/json`_. By default the content type is _`text/plain`_ for
  [primitive Rust types][primitive], `application/octet-stream` for _`[u8]`_ and
  _`application/json`_ for struct and complex enum types.
  Content type can also be slice of **content_type** values if the endpoint support returning multiple
 response content types. E.g _`["application/json", "text/xml"]`_ would indicate that endpoint can return both
 _`json`_ and _`xml`_ formats. **The order** of the content types define the default example show first in
 the Swagger UI. Swagger UI wil use the first _`content_type`_ value as a default example.

* `headers(...)` Slice of response headers that are returned back to a caller.

* `example = ...` Can be _`json!(...)`_. _`json!(...)`_ should be something that
  _`serde_json::json!`_ can parse as a _`serde_json::Value`_.

* `examples(...)` Define multiple examples for single response. This attribute is mutually
  exclusive to the _`example`_ attribute and if both are defined this will override the _`example`_.
    * `name = ...` This is first attribute and value must be literal string.
    * `summary = ...` Short description of example. Value must be literal string.
    * `description = ...` Long description of example. Attribute supports markdown for rich text
      representation. Value must be literal string.
    * `value = ...` Example value. It must be _`json!(...)`_. _`json!(...)`_ should be something that
      _`serde_json::json!`_ can parse as a _`serde_json::Value`_.
    * `external_value = ...` Define URI to literal example value. This is mutually exclusive to
      the _`value`_ attribute. Value must be literal string.

     _**Example of example definition.**_
    ```text
     ("John" = (summary = "This is John", value = json!({"name": "John"})))
    ```

# Examples

_**Use `ToResponses` to define [`salvo_oapi::endpoint`][path] responses.**_
```
# use salvo_core::http::{header::CONTENT_TYPE, HeaderValue};
# use salvo_core::prelude::*;
#[derive(salvo_oapi::ToSchema, Debug)]
struct BadRequest {
    message: String,
}

#[derive(salvo_oapi::ToResponses, Debug)]
enum UserResponses {
    /// Success response
    #[salvo(response(status = 200))]
    Success { value: String },

    #[salvo(response(status = 404))]
    NotFound,

    #[salvo(response(status = 400))]
    BadRequest(BadRequest),
}

impl Piece for UserResponses {
    fn render(self, res: &mut Response) {
        res.headers_mut()
            .insert(CONTENT_TYPE, HeaderValue::from_static("text/plain; charset=utf-8"));
        res.write_body(format!("{self:#?}")).ok();
    }
}

#[salvo_oapi::endpoint(
    responses(
        UserResponses
    )
)]
async fn get_user() -> UserResponses {
   UserResponses::NotFound
}
```
_**Named struct response with inlined schema.**_
```
/// This is success response
#[derive(salvo_oapi::ToResponses)]
#[salvo(response(status = 200))]
struct SuccessResponse {
    value: String,
}
```

_**Unit struct response without content.**_
```
#[derive(salvo_oapi::ToResponses)]
#[salvo(response(status = NOT_FOUND))]
struct NotFound;
```

_**Unnamed struct response with inlined response schema.**_
```
# #[derive(salvo_oapi::ToSchema)]
# struct Foo;
#[derive(salvo_oapi::ToResponses)]
#[salvo(response(status = 201))]
struct CreatedResponse(#[salvo(schema(...))] Foo);
```

_**Enum with multiple responses.**_
```
# #[derive(salvo_oapi::ToResponse, salvo_oapi::ToSchema)]
# struct Response {
#     message: String,
# }
# #[derive(salvo_oapi::ToSchema, Debug)]
# struct BadRequest {}
#[derive(salvo_oapi::ToResponses)]
enum UserResponses {
    /// Success response description.
    #[salvo(response(status = 200))]
    Success { value: String },

    #[salvo(response(status = 404))]
    NotFound,

    #[salvo(response(status = 400))]
    BadRequest(BadRequest),

    #[salvo(response(status = 500))]
    ServerError(Response),

    #[salvo(response(status = 418))]
    TeaPot(Response),
}
```

[to_responses]: trait.ToResponses.html
[to_schema]: trait.ToSchema.html
[to_response]: trait.ToResponse.html
[path_to_responses]: attr.path.html#responses-from-intoresponses
[primitive]: https://doc.rust-lang.org/std/primitive/index.html