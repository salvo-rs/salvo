mod config;

use config::{SslVerifyMode, build_client_config};

use tokio_postgres::Config;
use tokio_postgres_rustls::MakeRustlsConnect;
use url::Url;

pub(crate) fn configure_tls(
    url: &Url,
    config: &mut Config,
) -> Result<Option<MakeRustlsConnect>, toasty_core::Error> {
    let mut sslmode = SslVerifyMode::Prefer;
    let mut sslrootcert: Option<String> = None;
    let mut sslcert: Option<String> = None;
    let mut sslkey: Option<String> = None;

    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "sslmode" => {
                sslmode = match value.as_ref() {
                    "disable" => SslVerifyMode::Disable,
                    "prefer" => SslVerifyMode::Prefer,
                    "require" => SslVerifyMode::Require,
                    "verify-ca" => SslVerifyMode::VerifyCa,
                    "verify-full" => SslVerifyMode::VerifyFull,
                    other => {
                        return Err(toasty_core::Error::invalid_connection_url(format!(
                            "unsupported sslmode: {other}"
                        )));
                    }
                };
            }
            "sslrootcert" => {
                sslrootcert = Some(value.into_owned());
            }
            "sslcert" => {
                sslcert = Some(value.into_owned());
            }
            "sslkey" => {
                sslkey = Some(value.into_owned());
            }
            "channel_binding" => {
                let cb = match value.as_ref() {
                    "disable" => tokio_postgres::config::ChannelBinding::Disable,
                    "prefer" => tokio_postgres::config::ChannelBinding::Prefer,
                    "require" => tokio_postgres::config::ChannelBinding::Require,
                    other => {
                        return Err(toasty_core::Error::invalid_connection_url(format!(
                            "unsupported channel_binding: {other}"
                        )));
                    }
                };
                config.channel_binding(cb);
            }
            "sslnegotiation" => {
                let neg = match value.as_ref() {
                    "postgres" => tokio_postgres::config::SslNegotiation::Postgres,
                    "direct" => tokio_postgres::config::SslNegotiation::Direct,
                    other => {
                        return Err(toasty_core::Error::invalid_connection_url(format!(
                            "unsupported sslnegotiation: {other}"
                        )));
                    }
                };
                config.ssl_negotiation(neg);
            }
            _ => {}
        }
    }

    let ssl_mode = match sslmode {
        SslVerifyMode::Disable => tokio_postgres::config::SslMode::Disable,
        SslVerifyMode::Prefer => tokio_postgres::config::SslMode::Prefer,
        SslVerifyMode::Require | SslVerifyMode::VerifyCa | SslVerifyMode::VerifyFull => {
            tokio_postgres::config::SslMode::Require
        }
    };
    config.ssl_mode(ssl_mode);

    if sslmode != SslVerifyMode::Disable {
        let rustls_config = build_client_config(
            sslmode,
            sslrootcert.as_deref(),
            sslcert.as_deref(),
            sslkey.as_deref(),
        )?;
        Ok(Some(MakeRustlsConnect::new(rustls_config)))
    } else {
        Ok(None)
    }
}
