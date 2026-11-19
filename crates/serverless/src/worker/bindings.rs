//! Access to Cloudflare Workers bindings from a Salvo request.

use std::sync::Arc;

use salvo_core::Request;

/// Extension trait that exposes the Workers environment, execution context and
/// bindings attached by the Workers adapter to a Salvo [`Request`].
///
/// The values are inserted as request extensions during request conversion, so
/// these accessors only succeed for requests dispatched through a
/// [`WorkerServer`](crate::worker::WorkerServer).
///
/// # Errors
///
/// Every binding accessor returns a [`worker::Result`]. The error is either a
/// missing [`worker::Env`] (when the request was not produced by
/// [`WorkerServer`](crate::worker::WorkerServer)) or a Workers runtime failure
/// while looking up the named binding.
// 各绑定查询方法共享同一错误语义，在 trait 级别统一说明并关闭 missing_errors_doc，
// 避免在每个方法上重复相同的 # Errors 章节。
// All binding accessors share the same error semantics documented at the trait
// level; the lint is disabled here to avoid duplicating identical `# Errors`
// sections on every method.
#[allow(clippy::missing_errors_doc)]
pub trait WorkerRequestExt {
    /// Returns a clone of the [`worker::Env`] attached to this request.
    fn worker_env(&self) -> worker::Result<worker::Env>;

    /// Returns the [`worker::Context`] attached to this request.
    fn worker_ctx(&self) -> worker::Result<Arc<worker::Context>>;

    /// Looks up a KV namespace binding by name.
    fn worker_kv(&self, name: &str) -> worker::Result<worker::kv::KvStore>;

    /// Looks up a D1 database binding by name.
    fn worker_d1(&self, name: &str) -> worker::Result<worker::d1::D1Database>;

    /// Looks up an R2 bucket binding by name.
    fn worker_bucket(&self, name: &str) -> worker::Result<worker::Bucket>;

    /// Looks up a Queue binding by name.
    fn worker_queue(&self, name: &str) -> worker::Result<worker::Queue>;

    /// Looks up a Durable Object namespace binding by name.
    fn worker_durable_object(&self, name: &str) -> worker::Result<worker::ObjectNamespace>;

    /// Looks up a service binding (Worker-to-Worker communication) by name.
    fn worker_service(&self, name: &str) -> worker::Result<worker::Fetcher>;

    /// Looks up the Workers static assets binding by name.
    fn worker_assets(&self, name: &str) -> worker::Result<worker::Fetcher>;

    /// Looks up a secret binding by name.
    fn worker_secret(&self, name: &str) -> worker::Result<worker::Secret>;

    /// Looks up a plaintext environment variable binding by name.
    fn worker_var(&self, name: &str) -> worker::Result<worker::Var>;

    /// Looks up a Secret Store binding by name.
    fn worker_secret_store(&self, name: &str) -> worker::Result<worker::SecretStore>;

    /// Looks up a Rate Limiter binding by name.
    fn worker_rate_limiter(&self, name: &str) -> worker::Result<worker::RateLimiter>;

    /// Looks up a Workers AI binding by name.
    fn worker_ai(&self, name: &str) -> worker::Result<worker::Ai>;

    /// Looks up a `send_email` binding by name.
    fn worker_send_email(&self, name: &str) -> worker::Result<worker::SendEmail>;

    /// Looks up a Hyperdrive binding by name.
    fn worker_hyperdrive(&self, name: &str) -> worker::Result<worker::Hyperdrive>;

    /// Looks up a Dynamic Dispatcher binding by name.
    fn worker_dynamic_dispatcher(&self, name: &str) -> worker::Result<worker::DynamicDispatcher>;

    /// Looks up an Analytics Engine dataset binding by name.
    fn worker_analytics_engine(&self, name: &str)
    -> worker::Result<worker::AnalyticsEngineDataset>;
}

impl WorkerRequestExt for Request {
    fn worker_env(&self) -> worker::Result<worker::Env> {
        self.extensions()
            .get::<worker::Env>()
            .cloned()
            .ok_or_else(|| {
                worker::Error::RustError(
                    "Workers Env is not attached to this request; is it dispatched by WorkerServer?"
                        .into(),
                )
            })
    }

    fn worker_ctx(&self) -> worker::Result<Arc<worker::Context>> {
        self.extensions()
            .get::<Arc<worker::Context>>()
            .cloned()
            .ok_or_else(|| {
                worker::Error::RustError(
                    "Workers Context is not attached to this request; is it dispatched by WorkerServer?"
                        .into(),
                )
            })
    }

    fn worker_kv(&self, name: &str) -> worker::Result<worker::kv::KvStore> {
        self.worker_env()?.kv(name)
    }

    fn worker_d1(&self, name: &str) -> worker::Result<worker::d1::D1Database> {
        self.worker_env()?.d1(name)
    }

    fn worker_bucket(&self, name: &str) -> worker::Result<worker::Bucket> {
        self.worker_env()?.bucket(name)
    }

    fn worker_queue(&self, name: &str) -> worker::Result<worker::Queue> {
        self.worker_env()?.queue(name)
    }

    fn worker_durable_object(&self, name: &str) -> worker::Result<worker::ObjectNamespace> {
        self.worker_env()?.durable_object(name)
    }

    fn worker_service(&self, name: &str) -> worker::Result<worker::Fetcher> {
        self.worker_env()?.service(name)
    }

    fn worker_assets(&self, name: &str) -> worker::Result<worker::Fetcher> {
        self.worker_env()?.assets(name)
    }

    fn worker_secret(&self, name: &str) -> worker::Result<worker::Secret> {
        self.worker_env()?.secret(name)
    }

    fn worker_var(&self, name: &str) -> worker::Result<worker::Var> {
        self.worker_env()?.var(name)
    }

    fn worker_secret_store(&self, name: &str) -> worker::Result<worker::SecretStore> {
        self.worker_env()?.secret_store(name)
    }

    fn worker_rate_limiter(&self, name: &str) -> worker::Result<worker::RateLimiter> {
        self.worker_env()?.rate_limiter(name)
    }

    fn worker_ai(&self, name: &str) -> worker::Result<worker::Ai> {
        self.worker_env()?.ai(name)
    }

    fn worker_send_email(&self, name: &str) -> worker::Result<worker::SendEmail> {
        self.worker_env()?.send_email(name)
    }

    fn worker_hyperdrive(&self, name: &str) -> worker::Result<worker::Hyperdrive> {
        self.worker_env()?.hyperdrive(name)
    }

    fn worker_dynamic_dispatcher(&self, name: &str) -> worker::Result<worker::DynamicDispatcher> {
        self.worker_env()?.dynamic_dispatcher(name)
    }

    fn worker_analytics_engine(
        &self,
        name: &str,
    ) -> worker::Result<worker::AnalyticsEngineDataset> {
        self.worker_env()?.analytics_engine(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[wasm_bindgen_test::wasm_bindgen_test]
    fn missing_env_and_ctx_return_errors() {
        let req = Request::new();
        assert!(req.worker_env().is_err());
        assert!(req.worker_ctx().is_err());
    }
}
