Create OpenAPI Schema from arbitrary type.

This macro provides a quick way to render arbitrary types as OpenAPI Schema Objects. It
supports two call formats.
1. With type only
2. With _`#[inline]`_ attribute to inline the referenced schemas.

By default the macro will create references `($ref)` for non primitive types like _`Pet`_.
However when used with _`#[inline]`_ the non [`primitive`][primitive] type schemas will
be inlined to the schema output.

```
# #[derive(salvo_oapi::AsSchema)]
# struct Pet {id: i32};
let schema = salvo_oapi::schema!(Vec<Pet>);

// with inline
let schema = salvo_oapi::schema!(#[inline] Vec<Pet>);
```

# Examples

_**Create vec of pets schema.**_
```
# use salvo_core::prelude::*;
# use salvo_oapi::schema::{Schema, Array, Object, SchemaFormat, KnownFormat, SchemaType};
# use salvo_oapi::RefOr;
#[derive(salvo_oapi::AsSchema)]
struct Pet {
    id: i32,
    name: String,
}

let schema: RefOr<Schema> = salvo_oapi::schema!(#[inline] Vec<Pet>).into();
// will output
let generated = RefOr::T(Schema::Array(
    Array::new(
        Object::new()
            .property("id", Object::new()
                .schema_type(SchemaType::Integer)
                .format(SchemaFormat::KnownFormat(KnownFormat::Int32)))
            .required("id")
            .property("name", Object::with_type(SchemaType::String))
            .required("name")
    )
));
# assert_json_diff::assert_json_eq!(serde_json::to_value(&schema).unwrap(), serde_json::to_value(&generated).unwrap());
```

[primitive]: https://doc.rust-lang.org/std/primitive/index.html