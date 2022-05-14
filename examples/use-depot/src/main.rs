use salvo::prelude::*;

#[fn_handler]
async fn set_user(req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
    depot.insert("user", "client");
    ctrl.call_next(req, depot, res).await;
}
#[fn_handler]
async fn hello_world(depot: &mut Depot) -> String {
    format!("Hello {}", depot.get::<&str>("user").copied().unwrap_or_default())
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    tracing::info!("Listening on http://0.0.0.0:7878");
    Server::new(TcpListener::bind("0.0.0.0:7878")).serve(route()).await;
}

fn route() -> Router {
    Router::new().hoop(set_user).handle(hello_world)
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_hello_world() {
        use salvo::hyper;
        use salvo::prelude::*;

        let service = Service::new(super::route());

        async fn access(service: &Service) -> String {
            let req = hyper::Request::builder().method("GET").uri("http://127.0.0.1:7878");
            let req: Request = req.body(hyper::Body::empty()).unwrap().into();
            service.handle(req).await.take_text().await.unwrap()
        }

        assert_eq!(access(&service).await, "Hello client");
    }
}
