![screenshot](Screenshot.png)

# Salvo with SeaORM example app

[Modify from (github.com/SeaQL/sea-orm/examples/salvo_example](https://github.com/SeaQL/sea-orm/tree/master/examples/salvo_example))

1. Modify the `DATABASE_URL` var in `.env` to point to your chosen database

2. Turn on the appropriate database feature for your chosen db in `Cargo.toml` (the `"sqlx-sqlite",` line)

3. Execute `cargo run` to start the server

4. Visit [localhost:7878](http://localhost:7878) in browser after seeing the `server started` line
