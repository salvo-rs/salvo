use std::fs::Metadata;
use std::path::{Path, PathBuf};

use salvo_core::{Depot, Request};

/// A resolved filesystem resource presented to a [`StaticResourceAuthorizer`].
///
/// The root and target paths are canonical, and the target is contained within
/// the configured root. Metadata is a snapshot taken while resolving the
/// resource; it is not refreshed by the getters and may become stale.
///
/// Filesystem topology must remain trusted while requests are resolved and
/// served. This descriptor does not hold an open file handle or provide atomic
/// pathname resolution, and does not prevent concurrent filesystem changes.
#[derive(Debug)]
pub struct StaticResource {
    root: PathBuf,
    path: PathBuf,
    metadata: Metadata,
}

impl StaticResource {
    /// Creates a descriptor from canonical paths after verifying containment.
    pub(crate) fn new(root: PathBuf, path: PathBuf, metadata: Metadata) -> Self {
        debug_assert!(path.starts_with(&root));
        Self {
            root,
            path,
            metadata,
        }
    }

    /// Returns the canonical path of the configured static root.
    #[inline]
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Returns the canonical path of the resolved target within the root.
    #[inline]
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the target path relative to the configured root.
    ///
    /// Containment is verified before constructing this descriptor. The path is
    /// empty when the target is the configured root itself.
    #[inline]
    #[must_use]
    pub fn relative_path(&self) -> &Path {
        self.path
            .strip_prefix(&self.root)
            .expect("a static resource must be contained within its root")
    }

    /// Returns the metadata snapshot captured when this resource was resolved.
    #[inline]
    #[must_use]
    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    /// Returns whether the metadata snapshot describes a directory.
    #[inline]
    #[must_use]
    pub fn is_dir(&self) -> bool {
        self.metadata.is_dir()
    }

    /// Returns whether the metadata snapshot describes a regular file.
    #[inline]
    #[must_use]
    pub fn is_file(&self) -> bool {
        self.metadata.is_file()
    }
}

/// Decides whether a request may access a resolved static resource.
///
/// Implement this trait to perform asynchronous authorization. Synchronous
/// closures and functions with the same arguments are also supported.
/// Returning `false` denies access and causes the static directory handler to
/// return a terminal `404 Not Found` response for the selected resource.
#[salvo_core::async_trait]
pub trait StaticResourceAuthorizer: Send + Sync {
    /// Returns whether the request may access the resolved resource.
    async fn authorize(&self, req: &Request, depot: &Depot, resource: &StaticResource) -> bool;
}

#[salvo_core::async_trait]
impl<F> StaticResourceAuthorizer for F
where
    F: Fn(&Request, &Depot, &StaticResource) -> bool + Send + Sync,
{
    async fn authorize(&self, req: &Request, depot: &Depot, resource: &StaticResource) -> bool {
        self(req, depot, resource)
    }
}
