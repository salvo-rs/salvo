//! Ways to cache account data and certificates.
//! A default implementation for `AsRef<Path>` (`Sting`, `OsString`, `PathBuf`, ...)
//! allows the use of a local directory as cache.
//!
//! **Note**: The files contain private keys.


use std::error::Error as StdError;
use std::io::{Error as IoError, ErrorKind, Result as IoResult};
use std::path::Path;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::engine::Engine;
use ring::digest::{Context, SHA256};
use tokio::fs::{create_dir_all, read, OpenOptions};
use tokio::io::AsyncWriteExt;

/// An error that can be returned from an [`AcmeCache`].
pub trait CacheError: StdError + Send + Sync + 'static {}

impl<T> CacheError for T where T: StdError + Send + Sync + 'static {}

/// Trait to define a custom location/mechanism to cache account data and certificates.
pub trait AcmeCache {
    /// The error type returned from the functions on this trait.
    type Error: CacheError;

    /// Returns the previously written private key retrieved from `Acme`. The parameters are:
    ///
    /// ## Parameters
    ///
    /// * `directory_name`: the name of the `Acme` directory that this private key.
    /// * `domains`: the list of domains included in the private key was issued form.
    ///
    /// ## Errors
    ///
    /// Returns an error when the private key was unable to be written
    /// successfully.
    fn read_key(
        &self,
        directory_name: &str,
        domains: &[String],
    ) -> impl Future<Output = Result<Option<Vec<u8>>, Self::Error>> + Send;

    /// Writes a certificate retrieved from `Acme`. The parameters are:
    ///
    /// ## Parameters
    ///
    /// * `directory_name`: the name of the `Acme` directory that this private key.
    /// * `domains`: the list of domains included in the private key was issued form.
    /// * `data`: the private key, encoded in PEM format.
    ///
    /// ## Errors
    ///
    /// Returns an error when the certificate was unable to be written
    /// successfully.
    fn write_key(
        &self,
        directory_name: &str,
        domains: &[String],
        data: &[u8],
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;

    /// Returns the previously written certificate retrieved from `Acme`. The parameters are:
    ///
    /// ## Parameters
    ///
    /// * `directory_name`: the name of the `Acme` directory that this certificate
    /// * `domains`: the list of domains included in the certificate was issued form.
    ///
    /// ## Errors
    ///
    /// Returns an error when the certificate was unable to be written
    /// successfully.
    fn read_cert(
        &self,
        directory_name: &str,
        domains: &[String],
    ) -> impl Future<Output = Result<Option<Vec<u8>>, Self::Error>> + Send;

    /// Writes a certificate retrieved from `Acme`. The parameters are:
    ///
    /// ## Parameters
    ///
    /// * `directory_name`: the name of the `Acme` directory that this certificate
    /// * `domains`: the list of domains included in the certificate was issued form.
    /// * `data`: the private key, encoded in PEM format.
    ///
    /// ## Errors
    ///
    /// Returns an error when the certificate was unable to be written
    /// successfully.
    fn write_cert(
        &self,
        directory_name: &str,
        domains: &[String],
        data: &[u8],
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

static KEY_PEM_PREFIX: &str = "key-";
static CERT_PEM_PREFIX: &str = "cert-";

impl<P> AcmeCache for P
where
    P: AsRef<Path> + Send + Sync,
{
    type Error = IoError;

    async fn read_key(&self, directory_name: &str, domains: &[String]) -> Result<Option<Vec<u8>>, Self::Error> {
        let mut path = self.as_ref().to_path_buf();
        path.push(format!(
            "{}{}-{}",
            KEY_PEM_PREFIX,
            directory_name,
            file_hash_part(domains)
        ));
        match read(path).await {
            Ok(data) => Ok(Some(data)),
            Err(e) => match e.kind() {
                ErrorKind::NotFound => Ok(None),
                _ => Err(e),
            },
        }
    }
    async fn write_key(&self, directory_name: &str, domains: &[String], data: &[u8]) -> Result<(), Self::Error> {
        let mut path = self.as_ref().to_path_buf();
        create_dir_all(&path).await?;
        path.push(format!(
            "{}{}-{}",
            KEY_PEM_PREFIX,
            directory_name,
            file_hash_part(domains)
        ));
        write_data(path, data).await
    }

    async fn read_cert(&self, directory_name: &str, domains: &[String]) -> Result<Option<Vec<u8>>, Self::Error> {
        let mut path = self.as_ref().to_path_buf();
        path.push(format!(
            "{}{}-{}",
            CERT_PEM_PREFIX,
            directory_name,
            file_hash_part(domains)
        ));
        match read(path).await {
            Ok(data) => Ok(Some(data)),
            Err(e) => match e.kind() {
                ErrorKind::NotFound => Ok(None),
                _ => Err(e),
            },
        }
    }
    async fn write_cert(&self, directory_name: &str, domains: &[String], data: &[u8]) -> Result<(), Self::Error> {
        let mut path = self.as_ref().to_path_buf();
        create_dir_all(&path).await?;
        path.push(format!(
            "{}{}-{}",
            CERT_PEM_PREFIX,
            directory_name,
            file_hash_part(domains)
        ));
        write_data(path, data).await
    }
}
async fn write_data(file_path: impl AsRef<Path> + Send, data: impl AsRef<[u8]> + Send) -> IoResult<()> {
    let mut file = OpenOptions::new();
    file.write(true).create(true).truncate(true);
    #[cfg(unix)]
    file.mode(0o600); //user: R+W
    let file_path = file_path.as_ref();
    let data = data.as_ref();
    let mut buffer = file.open(file_path).await?;
    buffer.write_all(data).await?;
    Ok(())
}

fn file_hash_part(data: &[String]) -> String {
    let mut ctx = Context::new(&SHA256);
    for el in data {
        ctx.update(el.as_ref());
        ctx.update(&[0])
    }
    URL_SAFE_NO_PAD.encode(ctx.finish())
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use std::path::PathBuf;

//     #[tokio::test]
//     async fn test_acme_cache() {
//         let cache_path = PathBuf::from("temp/test_cache");
//         let directory_name = "test_directory";
//         let domains = vec!["example.com".to_string(), "www.example.com".to_string()];
//         let key_data = b"test_key_data";
//         let cert_data = b"test_cert_data";

//         // Test write_key
//         let result = AcmeCache::write_key(&cache_path, directory_name, &domains, key_data).await;
//         assert!(result.is_ok());

//         // Test read_key
//         let result = AcmeCache::read_key(&cache_path, directory_name, &domains).await;
//         assert!(result.is_ok());
//         assert_eq!(result.unwrap().unwrap(), key_data);

//         // Test write_cert
//         let result = AcmeCache::write_cert(&cache_path, directory_name, &domains, cert_data).await;
//         assert!(result.is_ok());

//         // Test read_cert
//         let result = AcmeCache::read_cert(&cache_path, directory_name, &domains).await;
//         assert!(result.is_ok());
//         assert_eq!(result.unwrap().unwrap(), cert_data);
//     }
// }
