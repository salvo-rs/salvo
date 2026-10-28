use std::path::PathBuf;

use salvo::conn::rustls::{Keycert, RustlsConfig};
use salvo::prelude::*;
use x509_parser::prelude::*;

#[handler]
async fn protected(req: &mut Request, res: &mut Response) {
    let Some(cert) = req
        .tls_info()
        .and_then(|tls| tls.peer_certificates().first())
    else {
        res.status_code(StatusCode::UNAUTHORIZED);
        res.render("A client certificate is required");
        return;
    };
    let Ok((_, cert)) = parse_x509_certificate(cert) else {
        res.status_code(StatusCode::FORBIDDEN);
        return;
    };
    // TLS has authenticated the certificate. This handler separately authorizes
    // the application identity encoded in its subject alternative name.
    let allowed =
        cert.subject_alternative_name()
            .ok()
            .flatten()
            .is_some_and(|san| {
                san.value.general_names.iter().any(|name| {
                    matches!(name, GeneralName::URI("spiffe://salvo.local/client/reader"))
                })
            });
    if !allowed {
        res.status_code(StatusCode::FORBIDDEN);
        res.render("This certificate cannot access the resource");
        return;
    }
    res.render(Text::Plain(format!("Hello, {}", cert.subject())));
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt().init();
    let cert_dir = PathBuf::from(std::env::var("MTLS_CERT_DIR")?);
    let keycert = Keycert::new()
        .cert_from_path(cert_dir.join("server.pem"))?
        .key_from_path(cert_dir.join("server.key"))?;
    let config = RustlsConfig::new(keycert);
    let config = if std::env::args().any(|arg| arg == "--optional") {
        config.client_auth_optional_path(cert_dir.join("ca.pem"))?
    } else {
        config.client_auth_required_path(cert_dir.join("ca.pem"))?
    };
    let acceptor = TcpListener::new("127.0.0.1:8698")
        .rustls(config)
        .try_bind()
        .await?;
    Server::new(acceptor)
        .serve(Router::new().get(protected))
        .await;
    Ok(())
}
