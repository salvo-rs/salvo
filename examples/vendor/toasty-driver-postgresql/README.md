<div align="center">
  <h1>Toasty</h1>
</div>

<div align="center">
  <h3>The cozy, easy ORM for Rust</h3>
  <a href="https://tokio-rs.github.io/toasty/0.8.0/guide/">Guide</a> •
  <a href="https://docs.rs/toasty">API Docs</a> •
  <a href="https://crates.io/crates/toasty">Crates.io</a> •
  <a href="https://discord.gg/tokio">Discord</a>
</div>

<br/>

<div align="center">

[![Crates.io][crates-badge]][crates-url]
[![MIT licensed][mit-badge]][mit-url]
[![Build Status][actions-badge]][actions-url]
[![Discord chat][discord-badge]][discord-url]

</div>

[crates-badge]: https://img.shields.io/crates/v/toasty.svg
[crates-url]: https://crates.io/crates/toasty
[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[mit-url]: https://github.com/tokio-rs/toasty/blob/main/LICENSE
[actions-badge]: https://github.com/tokio-rs/toasty/workflows/Cargo%20Build%20%26%20Test/badge.svg
[actions-url]: https://github.com/tokio-rs/toasty/actions?query=workflow%3A%22Cargo+Build+%26+Test%22+branch%3Amain
[discord-badge]: https://img.shields.io/discord/500028886025895936.svg?logo=discord&style=flat-square
[discord-url]: https://discord.gg/tokio

Toasty supports SQL databases (SQLite/Turso, PostgreSQL, MySQL) and DynamoDB.
It does not hide database capabilities — it exposes features based on the
target database.

## Using Toasty

You will define your data model using Rust structs annotated with the
`#[derive(toasty::Model)]` derive macro. Runnable examples live in the
[`examples/`](examples/) directory; the snippets below illustrate the basics.

```rust
#[derive(Debug, toasty::Model)]
struct User {
    #[key]
    #[auto]
    id: u64,

    name: String,

    #[unique]
    email: String,

    #[has_many]
    todos: toasty::Deferred<Vec<Todo>>,
}

#[derive(Debug, toasty::Model)]
struct Todo {
    #[key]
    #[auto]
    id: u64,

    #[index]
    user_id: u64,

    #[belongs_to]
    user: toasty::Deferred<User>,

    title: String,
}
```

Then, you can easily work with the data model:

```rust
// Create a new user and give them some todos.
let user = toasty::create!(User {
    name: "John Doe",
    email: "john@example.com",
    todos: [
        { title: "Make pizza" },
        { title: "Finish Toasty" },
        { title: "Sleep" },
    ],
}).exec(&mut db).await?;

// Load the user from the database
let user = User::get_by_id(&mut db, &user.id).await?;

// Load and iterate the user's todos
let todos = user.todos().exec(&mut db).await?;

for todo in &todos {
    println!("{:#?}", todo);
}
```

## SQL and NoSQL

Toasty supports both SQL and NoSQL databases. Current drivers are SQLite,
Turso, PostgreSQL, MySQL, and DynamoDB. However, it does not aim to abstract
the database. Instead, Toasty leans into the target database's capabilities
and aims to help the user avoid issuing inefficient queries for that database.

When targeting both SQL and NoSQL databases, Toasty generates query methods
(e.g. `get_by_id` only for access patterns that are indexed). When targeting a
SQL database, Toasty might allow arbitrary additional query constraints. When
targeting a NoSQL database, Toasty will only allow constraints that the
specific target database can execute. For example, with DynamoDB, query methods
might be generated based on the table's primary key, and additional constraints
may be set for the sort key.

## Application data model vs. database schema

Toasty decouples the application data model from the database's schema. By
default, a toasty application schema will map 1-1 with a database schema.
However, additional annotations may be specified to customize how the
application data model maps to the database schema.

## Roadmap

Development priorities are based on feedback and contributions. If you run into
missing features or rough edges, please open an issue or submit a pull request.
Planned work lives under [`docs/dev/roadmap.md`](docs/dev/roadmap.md).

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md). Small fixes can go straight to a PR;
larger changes follow a lightweight propose → roadmap + design doc → implement
flow.

## License

This project is licensed under the [MIT license].

[MIT license]: LICENSE

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in Toasty by you, shall be licensed as MIT, without any additional
terms or conditions.
