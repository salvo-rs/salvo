/*! Ways to cache account data and certificates.
A default implementation for `AsRef<Path>` (`Sting`, `OsString`, `PathBuf`, ...)
allows the use of a local directory as cache.
Note that the files contain private keys.
*/

use std::error::Error as StdError;
use std::io::{Error as IoError, ErrorKind};
use std::path::Path;

use async_trait::async_trait;
use ring::digest::{Context, SHA256};
use tokio::fs::{create_dir_all, read, OpenOptions};
use tokio::io::AsyncWriteExt;

/// An error that can be returned from an [`AcmeCache`].
pub trait CacheError: StdError + Send + Sync + 'static {}

impl<T> CacheError for T where T: StdError + Send + Sync + 'static {}
/// Trait to define a custom location/mechanism to cache account data and certificates.
#[async_trait]
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
    /// sucessfully.
    async fn read_key(&self, directory_name: &str, domains: &[String]) -> Result<Option<Vec<u8>>, Self::Error>;

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
    /// sucessfully.
    async fn write_key(&self, directory_name: &str, domains: &[String], data: &[u8]) -> Result<(), Self::Error>;

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
    /// sucessfully.
    async fn read_cert(&self, directory_name: &str, domains: &[String]) -> Result<Option<Vec<u8>>, Self::Error>;

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
    /// sucessfully.
    async fn write_cert(&self, directory_name: &str, domains: &[String], data: &[u8]) -> Result<(), Self::Error>;
}

static KEY_PEM_PREFIX: &str = "key-";
static CERT_PEM_PREFIX: &str = "cert-";
#[async_trait]
impl<P> AcmeCache for P
where
    P: AsRef<Path> + Send + Sync,
{
    type Error = IoError;

    #[inline]
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
    #[inline]
    async fn write_key(&self, directory_name: &str, domains: &[String], data: &[u8]) -> Result<(), Self::Error> {
        let mut path = self.as_ref().to_path_buf();
        create_dir_all(&path).await?;
        path.push(format!(
            "{}{}-{}",
            KEY_PEM_PREFIX,
            directory_name,
            file_hash_part(domains)
        ));
        Ok(write_data(path, data).await?)
    }

    #[inline]
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
    #[inline]
    async fn write_cert(&self, directory_name: &str, domains: &[String], data: &[u8]) -> Result<(), Self::Error> {
        let mut path = self.as_ref().to_path_buf();
        create_dir_all(&path).await?;
        path.push(format!(
            "{}{}-{}",
            CERT_PEM_PREFIX,
            directory_name,
            file_hash_part(domains)
        ));
        Ok(write_data(path, data).await?)
    }
}
#[inline]
async fn write_data(file_path: impl AsRef<Path>, data: impl AsRef<[u8]>) -> Result<(), IoError> {
    let mut file = OpenOptions::new();
    file.write(true).create(true).truncate(true);
    #[cfg(unix)]
    file.mode(0o600); //user: R+W
    let mut buffer = file.open(file_path.as_ref()).await?;
    buffer.write_all(data.as_ref()).await?;
    Ok(())
}

#[inline]
fn file_hash_part(data: &[String]) -> String {
    let mut ctx = Context::new(&SHA256);
    for el in data {
        ctx.update(el.as_ref());
        ctx.update(&[0])
    }
    let engine = base64::engine::fast_portable::FastPortable::from(
        &base64::alphabet::URL_SAFE,
        base64::engine::fast_portable::NO_PAD,
    );
    base64::encode_engine(ctx.finish(), &engine)
}
