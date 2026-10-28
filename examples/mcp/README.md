# MCP over Streamable HTTP

This example mounts the [official MCP Rust SDK (`rmcp`)](https://github.com/modelcontextprotocol/rust-sdk)
in Salvo using the `tower-compat` feature. It exposes a `hello` tool that returns
`Hello from Salvo!`.

## Run

From the repository root:

```sh
cargo run --manifest-path examples/Cargo.toml -p example-mcp
```

The MCP endpoint is `http://127.0.0.1:7878/mcp`.

## Connect a client

Start the [MCP Inspector](https://github.com/modelcontextprotocol/inspector) in another terminal:

```sh
npx @modelcontextprotocol/inspector
```

Open the URL printed by the Inspector, select **Streamable HTTP** as the transport,
enter `http://127.0.0.1:7878/mcp` as the server URL, and connect. List the tools and
call `hello` with no arguments. The result contains `Hello from Salvo!`.

Other MCP clients can use the same endpoint with the Streamable HTTP transport.
The endpoint speaks MCP; opening it as an ordinary web page does not call a tool.

## Integrate into an application

For a standalone application, enable these features:

```toml
[dependencies]
salvo = { version = "1.0.1", features = ["tower-compat"] }
rmcp = { version = "3.5.1", features = ["server", "transport-streamable-http-server"] }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

See [`src/main.rs`](src/main.rs) for the complete server. The `#[tool_router(server_handler)]`
macro registers the tool and implements the SDK's server handler. `StreamableHttpService`
provides the Tower service, and `LocalSessionManager` manages its in-memory MCP sessions.

`TowerServiceCompat::<salvo::http::ReqBody, _, _, _>::compat(mcp)` selects the request
body type explicitly because the SDK service supports multiple body types. Mount the
adapted handler with `Router::with_path("mcp").goal(handler)` so that POST, GET, and
DELETE requests all reach the SDK, which handles the MCP protocol and sessions.

The example binds to loopback for local development. Before exposing it to other
hosts, configure authentication and the SDK's host/origin validation for your deployment.
