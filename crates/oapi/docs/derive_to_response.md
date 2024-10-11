Generate reusable [OpenApi][openapi] response.

This is `#[derive]` implementation for [`ToResponse`][to_response] trait.


_`#[salvo(response(...))]`_ attribute can be used to alter and add [response attributes](#toresponse-response-attributes).

_`#[salvo(content(...))]`_ attributes is used to make enum variant a content of a specific type for the
response.

_`#[salvo(schema(...))]`_ attribute is used to inline a schema for a response in unnamed structs or
enum variants with `#[content]` attribute. **Note!** [`ToSchema`] need to be implemented for
the field or variant type.

Type derived with _`ToResponse`_ uses provided doc comment as a description for the response. It
can alternatively be overridden with _`description = ...`_ attribute.

_`ToResponse`_ can be used in four different ways to generate OpenAPI response component.

1. By decorating `struct` or `enum` with [`derive@ToResponse`] derive macro. This will create a
   response with inlined schema resolved from the fields of the `struct` or `variants` of the
   enum.

   ```rust
    # use salvo_oapi::ToResponse;
    #[derive(ToResponse)]
    #[salvo(response(description = "Person response returns single Person entity"))]
    struct Person {
        name: String,
    }
   ```

2. By decorating unnamed field `struct` with [`derive@ToResponse`] derive macro. Unnamed field struct
   allows users to use new type pattern to define one inner field which is used as a schema for
   the generated response. This allows users to define `Vec` and `Option` response types.
   Additionally these types can also be used with `#[salvo(schema(...))]` attribute to inline the
   field's type schema if it implements [`ToSchema`] derive macro.

   ```rust
    # #[derive(salvo_oapi::ToSchema)]
    # struct Person {
    #     name: String,
    # }
    /// Person list response
    #[derive(salvo_oapi::ToResponse)]
    struct PersonList(Vec<Person>);
   ```

3. By decorating unit struct with [`derive@ToResponse`] derive macro. Unit structs will produce a
   response without body.

   ```rust
    /// Success response which does not have body.
    #[derive(salvo_oapi::ToResponse)]
    struct SuccessResponse;
   ```

4. By decorating `enum` with variants having `#[salvo(content(...))]` attribute. This allows users to
   define multiple response content schemas to single response according to OpenAPI spec.
   **Note**: Enum with _`content`_ attribute in variants cannot have enum level _`example`_ or
   _`examples`_ defined. Instead examples need to be defined per variant basis. Additionally
   these variants can also be used with `#[salvo(schema(...))]` attribute to inline the variant's type schema
   if it implements [`ToSchema`] derive macro.

   ```rust
    #[derive(salvo_oapi::ToSchema)]
    struct Admin {
        name: String,
    }
    #[derive(salvo_oapi::ToSchema)]
    struct Admin2 {
        name: String,
        id: i32,
    }

    #[derive(salvo_oapi::ToResponse)]
    enum Person {
        #[salvo(
            response(examples(
                ("Person1" = (value = json!({"name": "name1"}))),
                ("Person2" = (value = json!({"name": "name2"})))
            ))
        )]
        Admin(#[salvo(content("application/vnd-custom-v1+json"))] Admin),

        #[salvo(response(example = json!({"name": "name3", "id": 1})))]
        Admin2(#[salvo(content("application/vnd-custom-v2+json"))] #[salvo(schema(inline))] Admin2),
    }
   ```

# ToResponse `#[salvo(response(...))]` attributes

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
  the Swagger UI. Swagger UI will use the first _`content_type`_ value as a default example.

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

_**Use reusable response in operation handler.**_
```
use salvo_core::http::{header::CONTENT_TYPE, HeaderValue};
use salvo_core::prelude::*;
use salvo_oapi::{ToSchema, ToResponse, endpoint};

#[derive(ToResponse, ToSchema)]
struct PersonResponse {
   value: String
}
impl Scribe for PersonResponse {
    fn render(self, res: &mut Response) {
        res.headers_mut()
            .insert(CONTENT_TYPE, HeaderValue::from_static("text/plain; charset=utf-8"));
        let _ = res.write_body(self.value);
    }
}

#[endpoint(
    responses(
        (status_code = 200, response = PersonResponse)
    )
)]
fn get_person() -> PersonResponse {
    PersonResponse { value: "person".to_string() }
}
```

_**Create a response from named struct.**_
```
use salvo_oapi::{ToSchema, ToResponse};

 /// This is description
 ///
 /// It will also be used in `ToSchema` if present
 #[derive(ToSchema, ToResponse)]
 #[salvo(
    response(
        description = "Override description for response",
        content_type = "text/xml"
    )
 )]
 #[salvo(
    response(
        example = json!({"name": "the name"}),
        headers(
            ("csrf-token", description = "response csrf token"),
            ("random-id" = i32)
        )
    )
 )]
 struct Person {
     name: String,
 }
```

_**Create inlined person list response.**_
```
 # #[derive(salvo_oapi::ToSchema)]
 # struct Person {
 #     name: String,
 # }
 /// Person list response
 #[derive(salvo_oapi::ToResponse)]
 struct PersonList(#[salvo(schema(inline))] Vec<Person>);
```

_**Create enum response from variants.**_
```
 #[derive(salvo_oapi::ToResponse)]
 enum PersonType {
     Value(String),
     Foobar,
 }
```

[to_response]: trait.ToResponse.html
[primitive]: https://doc.rust-lang.org/std/primitive/index.html
[openapi]: derive.OpenApi.html