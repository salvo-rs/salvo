# 🚀 Salvo Postgres Boilerplate

[![Rust](https://img.shields.io/badge/Rust-%23DEA584.svg?style=flat&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Salvo](https://img.shields.io/badge/Framework-Salvo-orange?style=flat&logo=rust)](https://salvo.rs/)
[![Database-PostgreSQL](https://img.shields.io/badge/Database-PostgreSQL-336791?style=flat&logo=postgresql&logoColor=white)](https://www.postgresql.org/)
[![Diesel](https://img.shields.io/badge/ORM-Diesel-4B8BBE?style=flat&logo=rust)](https://diesel.rs/)
[![Docker](https://img.shields.io/badge/Docker-ready-blue?style=flat&logo=docker&logoColor=white)](https://www.docker.com/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

A clean and ready-to-use boilerplate for building high-performance asynchronous web services using **Salvo** 🦀 and **PostgreSQL** 🐘. This template helps you quickly start developing Rust-based APIs with database integration and migrations powered by **Diesel**.

---

## ⚙️ Prerequisites

Before you begin, make sure you have the following installed:

- 🧰 **Rust & Cargo** → [Install Rust](https://www.rust-lang.org/tools/install)  
- 🐋 **Docker** → [Install Docker](https://docs.docker.com/get-docker/)  
- 🛠️ **Diesel CLI** → for managing database migrations

---

## 🧩 Install Diesel CLI

Install the Diesel command-line tool with PostgreSQL support:

```bash
# Install libpq-dev for Diesel Postgres
sudo apt-get update && apt-get install -y libpq-dev
cargo install diesel_cli --no-default-features --features postgres
```

---

## 🐘 Run PostgreSQL Instance (via Docker)

Start a local PostgreSQL instance with Docker:


```bash
docker run -d \
  --name salvo_postgres \
  -e POSTGRES_USER=darix \
  -e POSTGRES_PASSWORD=6775212952 \
  -e POSTGRES_DB=salvo_postgres \
  -p 5432:5432 \
  postgres:latest
```

---

## 🔧 Run Database Migrations


Run all Diesel migrations to set up your database schema:

```bash
diesel migration run
```

---

## 🏎️ Run Application with Cargo

Before running the application locally, make sure your environment variables are correctly set.

Start by copying the example environment file:

```bash
cp .env.example .env
```

Next, update the DATABASE_URL inside .env to match your local PostgreSQL setup. For example:

```
echo DATABASE_URL=postgres://darix:6775212952@localhost:5432/salvo_postgres > .env
```


Once your environment is configured, start the server using Cargo:

```bash
cargo run
```

The application will compile and launch your Salvo server, making it instantly available for local development and testing.

---

## 🐳 Run with Docker (without Docker Compose)

If you prefer running the application directly with Docker, update the environment variable to match your PostgreSQL connection URL:

```bash

docker build -t salvo_postgres_app .

docker run -d -p 5800:5800 \
  -e DATABASE_URL="postgres://darix:6775212952@localhost:5432/salvo_postgres" \
  --name salvo_postgres_app \
  salvo_postgres_app

```

---


## 🚢 Deploy with Docker Compose

Easily build and deploy your application using Docker Compose 🧱🐳:

```bash
docker compose up -d --build
```

---


### 🌐 Access the API Docs

Open the OpenAPI Salvo interactive docs at:

👉 [http://0.0.0.0:5800/docs](http://0.0.0.0:5800/docs)

---

### 🧩 Example Preview Docs

 ![IMAGE](./img/image.png)