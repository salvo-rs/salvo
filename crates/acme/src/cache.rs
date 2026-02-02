//! Ways to cache account data and certificates.
//! A default implementation for `AsRef<Path>` (`Sting`, `OsString`, `PathBuf`, ...)
//! allows the use of a local directory as cache.
//!
//! **Note**: The files contain private keys.

use std::error::Error as StdError;
use std::io::{Error as IoError, ErrorKind, Result as IoResult};
use std::path::Path;

use base64::engine::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use tokio::fs::{OpenOptions, create_dir_all, read};
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

    async fn read_key(
        &self,
        directory_name: &str,
        domains: &[String],
    ) -> Result<Option<Vec<u8>>, Self::Error> {
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
    async fn write_key(
        &self,
        directory_name: &str,
        domains: &[String],
        data: &[u8],
    ) -> Result<(), Self::Error> {
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

    async fn read_cert(
        &self,
        directory_name: &str,
        domains: &[String],
    ) -> Result<Option<Vec<u8>>, Self::Error> {
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
    async fn write_cert(
        &self,
        directory_name: &str,
        domains: &[String],
        data: &[u8],
    ) -> Result<(), Self::Error> {
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
async fn write_data(
    file_path: impl AsRef<Path> + Send,
    data: impl AsRef<[u8]> + Send,
) -> IoResult<()> {
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
#[cfg(any(feature = "aws-lc-rs", not(feature = "ring")))]
fn file_hash_part(data: &[String]) -> String {
    use aws_lc_rs::digest::{Context, SHA256};
    let mut ctx = Context::new(&SHA256);
    for el in data {
        ctx.update(el.as_ref());
        ctx.update(&[0])
    }
    URL_SAFE_NO_PAD.encode(ctx.finish())
}
#[cfg(all(not(feature = "aws-lc-rs"), feature = "ring"))]
fn file_hash_part(data: &[String]) -> String {
    use ring::digest::{Context, SHA256};
    let mut ctx = Context::new(&SHA256);
    for el in data {
        ctx.update(el.as_ref());
        ctx.update(&[0])
    }
    URL_SAFE_NO_PAD.encode(ctx.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_acme_cache_key_write_read() {
        let temp_dir = TempDir::new().unwrap();
        let cache_path: PathBuf = temp_dir.path().to_path_buf();
        let directory_name = "test_directory";
        let domains = vec!["example.com".to_string(), "www.example.com".to_string()];
        let key_data = b"test_key_data_content";

        // Test write_key
        let result: Result<(), IoError> =
            cache_path.write_key(directory_name, &domains, key_data).await;
        assert!(result.is_ok());

        // Test read_key
        let result: Result<Option<Vec<u8>>, IoError> =
            cache_path.read_key(directory_name, &domains).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().unwrap(), key_data.to_vec());
    }

    #[tokio::test]
    async fn test_acme_cache_cert_write_read() {
        let temp_dir = TempDir::new().unwrap();
        let cache_path: PathBuf = temp_dir.path().to_path_buf();
        let directory_name = "test_directory";
        let domains = vec!["example.com".to_string()];
        let cert_data = b"test_cert_data_content";

        // Test write_cert
        let result: Result<(), IoError> =
            cache_path.write_cert(directory_name, &domains, cert_data).await;
        assert!(result.is_ok());

        // Test read_cert
        let result: Result<Option<Vec<u8>>, IoError> =
            cache_path.read_cert(directory_name, &domains).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().unwrap(), cert_data.to_vec());
    }

    #[tokio::test]
    async fn test_acme_cache_read_key_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let cache_path: PathBuf = temp_dir.path().to_path_buf();
        let directory_name = "nonexistent";
        let domains = vec!["nonexistent.com".to_string()];

        let result: Result<Option<Vec<u8>>, IoError> =
            cache_path.read_key(directory_name, &domains).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_acme_cache_read_cert_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let cache_path: PathBuf = temp_dir.path().to_path_buf();
        let directory_name = "nonexistent";
        let domains = vec!["nonexistent.com".to_string()];

        let result: Result<Option<Vec<u8>>, IoError> =
            cache_path.read_cert(directory_name, &domains).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_acme_cache_different_domains_different_files() {
        let temp_dir = TempDir::new().unwrap();
        let cache_path: PathBuf = temp_dir.path().to_path_buf();
        let directory_name = "test";
        let domains1 = vec!["example1.com".to_string()];
        let domains2 = vec!["example2.com".to_string()];
        let key_data1 = b"key_data_1";
        let key_data2 = b"key_data_2";

        // Write keys for different domains
        let _: Result<(), IoError> = cache_path
            .write_key(directory_name, &domains1, key_data1)
            .await;
        let _: Result<(), IoError> = cache_path
            .write_key(directory_name, &domains2, key_data2)
            .await;

        // Read back and verify they are different
        let result1: Result<Option<Vec<u8>>, IoError> =
            cache_path.read_key(directory_name, &domains1).await;
        let result2: Result<Option<Vec<u8>>, IoError> =
            cache_path.read_key(directory_name, &domains2).await;

        assert_eq!(result1.unwrap().unwrap(), key_data1.to_vec());
        assert_eq!(result2.unwrap().unwrap(), key_data2.to_vec());
    }

    #[tokio::test]
    async fn test_acme_cache_overwrite_key() {
        let temp_dir = TempDir::new().unwrap();
        let cache_path: PathBuf = temp_dir.path().to_path_buf();
        let directory_name = "test";
        let domains = vec!["example.com".to_string()];

        // Write initial key
        let _: Result<(), IoError> = cache_path
            .write_key(directory_name, &domains, b"initial_key")
            .await;

        // Overwrite with new key
        let _: Result<(), IoError> = cache_path
            .write_key(directory_name, &domains, b"updated_key")
            .await;

        // Read and verify it's the updated key
        let result: Result<Option<Vec<u8>>, IoError> =
            cache_path.read_key(directory_name, &domains).await;
        assert_eq!(result.unwrap().unwrap(), b"updated_key".to_vec());
    }

    #[test]
    fn test_file_hash_part_deterministic() {
        let domains1 = vec!["example.com".to_string(), "www.example.com".to_string()];
        let domains2 = vec!["example.com".to_string(), "www.example.com".to_string()];

        let hash1 = file_hash_part(&domains1);
        let hash2 = file_hash_part(&domains2);

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_file_hash_part_different_domains() {
        let domains1 = vec!["example.com".to_string()];
        let domains2 = vec!["different.com".to_string()];

        let hash1 = file_hash_part(&domains1);
        let hash2 = file_hash_part(&domains2);

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_file_hash_part_order_matters() {
        let domains1 = vec!["a.com".to_string(), "b.com".to_string()];
        let domains2 = vec!["b.com".to_string(), "a.com".to_string()];

        let hash1 = file_hash_part(&domains1);
        let hash2 = file_hash_part(&domains2);

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_file_hash_part_empty() {
        let domains: Vec<String> = vec![];
        let hash = file_hash_part(&domains);
        // Should produce a valid hash even for empty input
        assert!(!hash.is_empty());
    }

    #[tokio::test]
    async fn test_acme_cache_with_string_path() {
        let temp_dir = TempDir::new().unwrap();
        let cache_path: String = temp_dir.path().to_str().unwrap().to_string();
        let directory_name = "test";
        let domains = vec!["example.com".to_string()];
        let key_data = b"test_key";

        // Test with String path
        let result: Result<(), IoError> =
            cache_path.write_key(directory_name, &domains, key_data).await;
        assert!(result.is_ok());

        let result: Result<Option<Vec<u8>>, IoError> =
            cache_path.read_key(directory_name, &domains).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().unwrap(), key_data.to_vec());
    }

    #[tokio::test]
    async fn test_acme_cache_creates_directory() {
        let temp_dir = TempDir::new().unwrap();
        let cache_path: PathBuf = temp_dir.path().join("nested").join("cache").join("dir");
        let directory_name = "test";
        let domains = vec!["example.com".to_string()];
        let key_data = b"test_key";

        // Directory doesn't exist yet
        assert!(!cache_path.exists());

        // Write should create the directory
        let result: Result<(), IoError> =
            cache_path.write_key(directory_name, &domains, key_data).await;
        assert!(result.is_ok());

        // Directory should now exist
        assert!(cache_path.exists());
    }
}
