# Request

For web applications it’s crucial to react to the data a client sends to the server. In Salvo this information is provided by the request:

```rust
#[handler]
async fn hello(req: &mut Request) -> String {
    req.params().get("id").cloned().unwrap_or_default()
}
```

## About query string

We can get query string from request object:

```rust
req.query::<String>("id");
```

## About form


```rust
req.form::<String>("id").await;
```


## About json payload

```rust
req.parse_json::<User>().await;
```

## Extract Data

Request can be parsed into strongly typed structures by providing several functions through ```Request```.

* ```parse_params```: parse the requested router params into a specific data type;
* ```parse_queries```: parse the requested URL queries into a specific data type;
* ```parse_headers```: parse the requested HTTP haders into a specific data type;
* ```parse_json```: Parse the data in the HTTP body part of the request as JSON format to a specific type;
* ```parse_form```: Parse the data in the HTTP body part of the request as a Form form to a specific type;
* ```parse_body```: Parse the data in the HTTP body section to a specific type according to the type of the requested ```content-type```.
* ```extract```: can combine different data sources to parse a specific type.

## Parsing principle

The customized ```serde::Deserializer``` will be extract data similar to ```HashMap<String, String>``` and ```HashMap<String, Vec<String>>``` into a specific data type.

For example: ```URL queries``` is actually extracted as a [MultiMap](https://docs.rs/multimap/latest/multimap/struct.MultiMap.html) type, ```MultiMap``` can think of it as a data structure like ```HashMap<String, Vec<String>>```. If the requested URL is ```http://localhost/users?id=123&id=234```, we provide The target type is:

```rust
#[derive(Deserialize)]
struct User {
  id: i64
}
```

Then the first ```id=123``` will be parsed, and ```id=234``` will be discarded:

```rust
let user: User = req.parse_queries().unwrap();
assert_eq!(user.id, 123);
```

If the type we provide is:

```rust
#[derive(Deserialize)]
struct Users {
  id: Vec<i64>
}
```

Then ```id=123&id=234``` will be parsed:

```rust
let users: Users = req.parse_queries().unwrap();
assert_eq!(user.ids, vec![123, 234]);
```

Multiple data sources can be merged to parse out a specific type. You can define a custom type first, for example:

```rust
#[derive(Serialize, Deserialize, Extractible, Debug)]
/// Get the data field value from the body by default.
#[extract(default_source(from = "body"))]
struct GoodMan<'a> {
    /// The id number is obtained from the request path parameter, and the data is automatically parsed as i64 type.
    #[extract(source(from = "param"))]
    id: i64,
    /// Reference types can be used to avoid memory copying.
    username: &'a str,
    first_name: String,
    last_name: String,
}
```

Then in ```Handler``` you can get the data like this:

```rust
#[handler]
async fn edit(req: &mut Request) -> String {
    let good_man: GoodMan<'_> = req.extract().await.unwrap();
}
```

You can even pass the type directly to the function as a parameter, like this:

```rust
#[handler]
async fn edit<'a>(good_man: GoodMan<'a>) -> String {
    res.render(Json(good_man));
}
```

There is considerable flexibility in the definition of data types, and can even be resolved into nested structures as needed:

```rust
#[derive(Serialize, Deserialize, Extractible, Debug)]
#[extract(default_source(from = "body", format = "json"))]
struct GoodMan<'a> {
    #[extract(source(from = "param"))]
    id: i64,
    #[extract(source(from = "query"))]
    username: &'a str,
    first_name: String,
    last_name: String,
    lovers: Vec<String>,
    /// The nested field is completely reparsed from Request.
    #[extract(source(from = "request"))]
    nested: Nested<'a>,
}

#[derive(Serialize, Deserialize, Extractible, Debug)]
#[extract(default_source(from = "body", format = "json"))]
struct Nested<'a> {
    #[extract(source(from = "param"))]
    id: i64,
    #[extract(source(from = "query"))]
    username: &'a str,
    first_name: String,
    last_name: String,
    #[extract(rename = "lovers")]
    #[serde(default)]
    pets: Vec<String>,
}
```

For specific examples, see: [extract-nested](https://github.com/salvo-rs/salvo/blob/main/examples/extract-nested/src/main.rs).