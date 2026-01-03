FROM rust:1.90 AS builder

WORKDIR /app

# Install libpq-dev for Diesel Postgres
RUN apt-get update && apt-get install -y libpq-dev

COPY . .

RUN cargo install diesel_cli --no-default-features --features postgres --force

RUN cargo build --release --target-dir target



FROM scratch

WORKDIR /app

# ENV DATABASE_URL=postgres://darix:6775212952@localhost:5432/salvo_postgres

COPY --from=builder /app/target/release/salvo_postgres /app/salvo_postgres

# Copy diesel CLI (from builder)
COPY --from=builder /usr/local/cargo/bin/diesel /usr/local/bin/diesel

COPY --from=builder . /app/

EXPOSE 8698

CMD ["/app/salvo_postgres"]