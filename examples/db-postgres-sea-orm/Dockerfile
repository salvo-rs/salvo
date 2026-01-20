FROM rust:1.90 AS builder

WORKDIR /app

COPY . .

# Install Sea-orm CLI
RUN cargo install sea-orm-cli@^2.0.0-rc

RUN cargo build --release --target-dir target

FROM scratch

WORKDIR /app

# ENV DATABASE_URL=postgres://darix:6775212952@localhost:5432/salvo_postgres_seaorm

COPY --from=builder /app/target/release/salvo_postgres_seaorm /app/salvo_postgres_seaorm

# Copy diesel CLI (from builder)
COPY --from=builder /usr/local/cargo/bin/sea-orm-cli /usr/local/bin/sea-orm-cli

COPY --from=builder . /app/

RUN chmod +x /app/salvo_postgres_seaorm

EXPOSE 5800

CMD ["/app/salvo_postgres_seaorm"]