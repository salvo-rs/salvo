# Introduction

Salvo is an extremely simple and powerful Rust web backend framework. Only basic Rust knowledge is required to develop backend services.

- Built with [Hyper](https://crates.io/crates/hyper) and [Tokio](https://crates.io/crates/tokio);
- Http1, Http2 and **Http3**;
- Unified middleware and handle interface;
- Limitless routers nesting;
- Every router can have one or many middlewares;
- Integrated Multipart form processing;
- Support WebSocket;
- Acme support, automatically get TLS certificate from [let's encrypt](https://letsencrypt.org/).