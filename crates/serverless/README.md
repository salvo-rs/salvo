# salvo-serverless

Salvo 无服务器（Serverless）运行时适配层。

`salvo-serverless` is the serverless runtime adapter layer for the Salvo web
framework. It lets a Salvo [`Service`](https://docs.rs/salvo-core) run where
there is no long-lived TCP listener — specifically on AWS Lambda and on
Cloudflare Workers — by converting each platform event into a Salvo request,
dispatching it through the service, and converting the result back.

## 特性 Feature flags

| Feature    | 说明 Description                                          | 默认 Default |
| ---------- | --------------------------------------------------------- | :----------: |
| `rt_lambda` | AWS Lambda HTTP 运行时适配器与 `#[service]` 入口            | ❌            |
| `rt_worker` | Cloudflare Workers 运行时适配器与 `#[service]` 入口（仅 wasm32/wasm64 目标） | ❌ |

两个 feature 可同时启用；`#[service]` 宏会按目标架构生成各自独立的入口。未启用任何
运行时 feature 时，宏会展开为 `compile_error!`。

Both features can be enabled at once; the generated entry points are gated by
target architecture. When neither runtime feature is enabled the macro expands
to a `compile_error!`.

## 模块结构 Module structure

```
crates/serverless/src
├── lib.rs          # crate 根，prelude 与 feature 门控
├── server.rs       # GeneralServer 传输无关抽象
├── log.rs          # LogConfig / TracingLog 运行时日志
├── lambda
│   ├── mod.rs      # LambdaServer
│   ├── request.rs  # Lambda 事件 → Salvo 请求（LambdaRequestExt）
│   └── response.rs # Salvo 响应 → Lambda 响应（缓冲 / 流式）
└── worker
    ├── mod.rs      # Worker 适配器入口与 WorkerWebSocketHandle
    ├── server.rs   # WorkerServer
    ├── request.rs  # Workers 请求 → Salvo 请求（流式透传）
    ├── response.rs # Salvo 响应 → Workers 响应（流式输出）
    └── bindings.rs # WorkerRequestExt 绑定访问
```

## 公共 API Public API

### `server::GeneralServer`

传输无关的服务抽象，运行时适配器都实现该 trait，使生成入口能以同一方式分发请求：

- `fn serve(&self, request: Request) -> impl Future<Output = Response>`

`Service` 本身实现了该 trait。

### `lambda::LambdaServer`

AWS Lambda HTTP 运行时适配器：

- `LambdaServer::new(service)` / `From<Service>` / `From<Router>`
- `handle(req)` — 缓冲响应体（适合小响应）
- `handle_streaming(req)` — 流式响应体，返回 `lambda_http::Response<ResBody>`，配合
  `lambda_http::run_with_streaming_response` 使用

### `worker::WorkerServer`

Cloudflare Workers 运行时适配器：

- `WorkerServer::new(service)` / `From<Service>` / `From<Router>`
- `handle(req, env, ctx)` — 一次请求处理，请求/响应体均流式转发

### 扩展 trait Extension traits

- `lambda::LambdaRequestExt` — 读取 Lambda 执行上下文、API Gateway 请求上下文、
  查询/路径参数与 stage 变量。
- `worker::WorkerRequestExt` — 按名称读取 Workers 绑定：KV、D1、R2、Queue、Durable
  Object、Service、Assets、Secret、Var、Secret Store、Rate Limiter、AI、SendEmail、
  Hyperdrive、Dynamic Dispatcher、Analytics Engine。

### `worker::WorkerWebSocketHandle`

携带 Workers WebSocket 句柄，用于把 Salvo 响应转换为 WebSocket 升级响应，避免将握手
响应缓冲为字节。

### `log::{LogConfig, TracingLog}`

运行时构建并安装全局 `tracing` subscriber，支持 level、filter、ANSI、JSON 与线程信息
配置。

### `#[service]` 宏

`salvo_serverless_macros::service`，通过 `salvo::serverless::prelude::*` 或
`salvo::prelude::*` 使用。作用于一个零参数函数，将其重命名为 `<name>_service` 并生成
平台入口。

## 本次修改说明 Change notes

### 1. 统一 Lambda 与 Worker 入口方式 Unified entry point

两个运行时此前入口方式不一致：Lambda 侧需要手写 `main` 调用
`run_with_streaming_response`，Worker 侧使用 `#[service]` 宏。现统一为 **仅使用
`#[service]` 宏**：

- `rt_lambda` 下自动生成 `async fn main()`，内部构建 `LambdaServer` 并默认走
  **流式**入口 `run_with_streaming_response` + `handle_streaming`；
- `rt_worker` 下生成 `_salvo_worker_entry` 模块，含 `#[worker::event(start)]` 与
  `#[worker::event(fetch)]`，用 `OnceLock<WorkerServer>` 缓存服务实例。

两个示例 `examples/serverless-lambda` 与 `examples/serverless-worker` 均已统一为
`#[service(start = init)]` 形式。

### 2. Worker 请求体流式透传 Streaming request body

新增 `StreamedWorkerBody`（`request.rs`），把 `worker::Body` 包装为 Salvo 的
`ReqBody::Boxed` 流式体，错误统一映射为 Salvo 的 `BoxedError`。**未使用任何
`unsafe`**：因 `worker::Body` 为 `Unpin`，直接 `self.get_mut()` 重借用内部字段即可，
避免大文件上传一次性读入内存。

### 3. Worker 响应体流式输出 Streaming response body

`into_worker_response` 将 Salvo `ResBody` 通过 `into_data_stream()` 交给 Workers 运行时
逐块转发，而不是整体缓冲，支持大文件/无限流式响应。

### 4. Lambda 流式响应 Lambda streaming response

`LambdaServer` 新增 `handle_streaming`，配合 `from_salvo_response_streaming` 保留
`ResBody`（不缓冲），供 `run_with_streaming_response` 增量转发响应块。

### 5. 绑定访问与请求扩展 Bindings and request extensions

- `WorkerRequestExt` 覆盖 worker-rs 0.9.0 提供的全部绑定类型。
- `LambdaRequestExt` 暴露 `lambda_http` 预解析的事件数据。
- `Env` 与 `Arc<Context>` 作为请求扩展注入，供处理器按需获取绑定。

### 6. 运行时日志 Runtime logging

`LogConfig` / `TracingLog` 在运行时构建全局 `tracing` subscriber，无编译期读取。

### 相关 crate Related crates

- `salvo-serverless-macros` — `#[service]` 过程宏。
- `salvo-serverless-extra` — Workers 兼容层（如 `WebSocketUpgrade`，镜像 `salvo_extra`
  的同名 API）。

## 使用示例 Usage

### AWS Lambda

```rust,ignore
use salvo::prelude::*;

#[service(start = init)]
fn build() -> Router {
    Router::new().push(Router::with_path("hello").get(|| async { "hello" }))
}

fn init() {
    // 一次性初始化
}
```

### Cloudflare Workers

```rust,ignore
use salvo::prelude::*;

#[service(start = init)]
fn build() -> Router {
    Router::new().push(Router::with_path("hello").get(|| async { "hello" }))
}

fn init() {}
```

Workers 下游 crate 必须显式声明 `worker` 为直接依赖，因为 `#[worker::event]` 展开为
硬编码的 `::worker::` 路径，无法重定向。