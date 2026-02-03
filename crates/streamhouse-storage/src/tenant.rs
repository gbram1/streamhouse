//! Tenant-Aware Object Store
//!
//! This module provides a tenant-isolated wrapper around any ObjectStore implementation.
//! All paths are automatically prefixed with the organization's unique prefix, ensuring
//! complete data isolation between tenants.
//!
//! ## Path Isolation Pattern
//!
//! ```text
//! Original path:     data/orders/0/00000000000000000000.seg
//! Tenant path:       org-{uuid}/data/orders/0/00000000000000000000.seg
//!
//! Bucket structure:
//! ├── org-11111111-1111-1111-1111-111111111111/
//! │   └── data/
//! │       └── orders/
//! │           └── 0/
//! │               ├── 00000000000000000000.seg
//! │               └── 00000000000000010000.seg
//! └── org-22222222-2222-2222-2222-222222222222/
//!     └── data/
//!         └── events/
//!             └── 0/
//!                 └── 00000000000000000000.seg
//! ```
//!
//! ## Usage
//!
//! ```ignore
//! use streamhouse_storage::TenantObjectStore;
//! use streamhouse_metadata::TenantContext;
//!
//! // Create tenant-aware storage
//! let base_store = Arc::new(AmazonS3Builder::new()...build()?);
//! let tenant_store = Arc::new(TenantObjectStore::new(
//!     base_store,
//!     "org-11111111-1111-1111-1111-111111111111",
//! ));
//!
//! // All operations are now isolated to this tenant's prefix
//! let writer = PartitionWriter::new(
//!     "orders".to_string(),
//!     0,
//!     tenant_store,  // Uses TenantObjectStore
//!     metadata,
//!     config,
//! ).await?;
//! ```
//!
//! ## Security Benefits
//!
//! - **Complete isolation**: One tenant cannot access another's data even with path manipulation
//! - **Defense in depth**: Works alongside metadata store isolation
//! - **Audit-friendly**: Clear path structure for compliance
//! - **Cost allocation**: Easy to track storage usage per tenant

use async_trait::async_trait;
use bytes::Bytes;
use futures::stream::BoxStream;
use object_store::{
    path::Path, GetOptions, GetResult, ListResult, MultipartId, ObjectMeta, ObjectStore,
    PutOptions, PutResult, Result,
};
use std::fmt::{Debug, Display, Formatter};
use std::ops::Range;
use std::sync::Arc;
use tokio::io::AsyncWrite;

/// A tenant-isolated wrapper around any ObjectStore implementation.
///
/// All paths are automatically prefixed with the tenant's unique prefix,
/// ensuring complete data isolation between tenants without requiring
/// changes to the underlying storage logic.
pub struct TenantObjectStore {
    /// The underlying object store
    inner: Arc<dyn ObjectStore>,

    /// The tenant's unique prefix (e.g., "org-{uuid}")
    prefix: String,
}

impl TenantObjectStore {
    /// Create a new tenant-isolated object store.
    ///
    /// # Arguments
    ///
    /// * `inner` - The underlying object store (S3, GCS, local, etc.)
    /// * `prefix` - The tenant's unique prefix (typically "org-{organization_id}")
    ///
    /// # Example
    ///
    /// ```ignore
    /// let tenant_store = TenantObjectStore::new(
    ///     s3_store,
    ///     "org-11111111-1111-1111-1111-111111111111",
    /// );
    /// ```
    pub fn new(inner: Arc<dyn ObjectStore>, prefix: impl Into<String>) -> Self {
        Self {
            inner,
            prefix: prefix.into(),
        }
    }

    /// Create a tenant object store from an organization ID.
    ///
    /// Uses the organization ID to create the prefix.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let tenant_store = TenantObjectStore::from_organization_id(
    ///     s3_store,
    ///     "11111111-1111-1111-1111-111111111111",
    /// );
    /// ```
    pub fn from_organization_id(inner: Arc<dyn ObjectStore>, organization_id: &str) -> Self {
        Self::new(inner, format!("org-{}", organization_id))
    }

    /// Get the tenant prefix.
    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    /// Get the underlying object store.
    pub fn inner(&self) -> &Arc<dyn ObjectStore> {
        &self.inner
    }

    /// Convert a relative path to a tenant-prefixed path.
    fn tenant_path(&self, location: &Path) -> Path {
        // Combine prefix with the original path
        Path::from(format!("{}/{}", self.prefix, location))
    }

    /// Remove the tenant prefix from a path (for results returned to caller).
    fn strip_prefix(&self, location: &Path) -> Path {
        let path_str = location.to_string();
        let prefix_with_slash = format!("{}/", self.prefix);

        if path_str.starts_with(&prefix_with_slash) {
            Path::from(&path_str[prefix_with_slash.len()..])
        } else {
            location.clone()
        }
    }

    /// Transform ObjectMeta to remove tenant prefix from path.
    fn transform_meta(&self, meta: ObjectMeta) -> ObjectMeta {
        ObjectMeta {
            location: self.strip_prefix(&meta.location),
            last_modified: meta.last_modified,
            size: meta.size,
            e_tag: meta.e_tag,
            version: meta.version,
        }
    }
}

impl Debug for TenantObjectStore {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TenantObjectStore")
            .field("prefix", &self.prefix)
            .field("inner", &self.inner)
            .finish()
    }
}

impl Display for TenantObjectStore {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "TenantObjectStore(prefix={})", self.prefix)
    }
}

#[async_trait]
impl ObjectStore for TenantObjectStore {
    /// Upload data to the tenant's storage space.
    async fn put(&self, location: &Path, bytes: Bytes) -> Result<PutResult> {
        let tenant_location = self.tenant_path(location);
        tracing::trace!(
            original = %location,
            tenant = %tenant_location,
            "TenantObjectStore::put"
        );
        self.inner.put(&tenant_location, bytes).await
    }

    /// Upload data with options to the tenant's storage space.
    async fn put_opts(&self, location: &Path, bytes: Bytes, opts: PutOptions) -> Result<PutResult> {
        let tenant_location = self.tenant_path(location);
        tracing::trace!(
            original = %location,
            tenant = %tenant_location,
            "TenantObjectStore::put_opts"
        );
        self.inner.put_opts(&tenant_location, bytes, opts).await
    }

    /// Start a multipart upload to the tenant's storage space.
    async fn put_multipart(
        &self,
        location: &Path,
    ) -> Result<(MultipartId, Box<dyn AsyncWrite + Unpin + Send>)> {
        let tenant_location = self.tenant_path(location);
        tracing::trace!(
            original = %location,
            tenant = %tenant_location,
            "TenantObjectStore::put_multipart"
        );
        self.inner.put_multipart(&tenant_location).await
    }

    /// Abort a multipart upload.
    async fn abort_multipart(&self, location: &Path, multipart_id: &MultipartId) -> Result<()> {
        let tenant_location = self.tenant_path(location);
        tracing::trace!(
            original = %location,
            tenant = %tenant_location,
            "TenantObjectStore::abort_multipart"
        );
        self.inner
            .abort_multipart(&tenant_location, multipart_id)
            .await
    }

    /// Download data from the tenant's storage space.
    async fn get(&self, location: &Path) -> Result<GetResult> {
        let tenant_location = self.tenant_path(location);
        tracing::trace!(
            original = %location,
            tenant = %tenant_location,
            "TenantObjectStore::get"
        );
        self.inner.get(&tenant_location).await
    }

    /// Download data with options from the tenant's storage space.
    async fn get_opts(&self, location: &Path, options: GetOptions) -> Result<GetResult> {
        let tenant_location = self.tenant_path(location);
        tracing::trace!(
            original = %location,
            tenant = %tenant_location,
            "TenantObjectStore::get_opts"
        );
        self.inner.get_opts(&tenant_location, options).await
    }

    /// Download a range of data from the tenant's storage space.
    async fn get_range(&self, location: &Path, range: Range<usize>) -> Result<Bytes> {
        let tenant_location = self.tenant_path(location);
        tracing::trace!(
            original = %location,
            tenant = %tenant_location,
            "TenantObjectStore::get_range"
        );
        self.inner.get_range(&tenant_location, range).await
    }

    /// Get ranges of data from the tenant's storage space.
    async fn get_ranges(&self, location: &Path, ranges: &[Range<usize>]) -> Result<Vec<Bytes>> {
        let tenant_location = self.tenant_path(location);
        tracing::trace!(
            original = %location,
            tenant = %tenant_location,
            "TenantObjectStore::get_ranges"
        );
        self.inner.get_ranges(&tenant_location, ranges).await
    }

    /// Get object metadata from the tenant's storage space.
    async fn head(&self, location: &Path) -> Result<ObjectMeta> {
        let tenant_location = self.tenant_path(location);
        tracing::trace!(
            original = %location,
            tenant = %tenant_location,
            "TenantObjectStore::head"
        );
        let meta = self.inner.head(&tenant_location).await?;
        Ok(self.transform_meta(meta))
    }

    /// Delete an object from the tenant's storage space.
    async fn delete(&self, location: &Path) -> Result<()> {
        let tenant_location = self.tenant_path(location);
        tracing::trace!(
            original = %location,
            tenant = %tenant_location,
            "TenantObjectStore::delete"
        );
        self.inner.delete(&tenant_location).await
    }

    /// List objects in the tenant's storage space.
    ///
    /// Results have the tenant prefix stripped, so callers see paths
    /// as if they were in their own isolated namespace.
    fn list(&self, prefix: Option<&Path>) -> BoxStream<'_, Result<ObjectMeta>> {
        let tenant_prefix = match prefix {
            Some(p) => self.tenant_path(p),
            None => Path::from(self.prefix.as_str()),
        };

        tracing::trace!(
            original = ?prefix,
            tenant = %tenant_prefix,
            "TenantObjectStore::list"
        );

        // Transform the stream to strip prefix from all results
        let prefix_clone = self.prefix.clone();
        Box::pin(futures::stream::unfold(
            (self.inner.list(Some(&tenant_prefix)), prefix_clone),
            |(mut stream, prefix)| async move {
                use futures::StreamExt;
                match stream.next().await {
                    Some(Ok(meta)) => {
                        let path_str = meta.location.to_string();
                        let prefix_with_slash = format!("{}/", prefix);
                        let stripped = if path_str.starts_with(&prefix_with_slash) {
                            Path::from(&path_str[prefix_with_slash.len()..])
                        } else {
                            meta.location.clone()
                        };
                        let transformed = ObjectMeta {
                            location: stripped,
                            last_modified: meta.last_modified,
                            size: meta.size,
                            e_tag: meta.e_tag,
                            version: meta.version,
                        };
                        Some((Ok(transformed), (stream, prefix)))
                    }
                    Some(Err(e)) => Some((Err(e), (stream, prefix))),
                    None => None,
                }
            },
        ))
    }

    /// List objects with delimiter in the tenant's storage space.
    async fn list_with_delimiter(&self, prefix: Option<&Path>) -> Result<ListResult> {
        let tenant_prefix = match prefix {
            Some(p) => self.tenant_path(p),
            None => Path::from(self.prefix.as_str()),
        };

        tracing::trace!(
            original = ?prefix,
            tenant = %tenant_prefix,
            "TenantObjectStore::list_with_delimiter"
        );

        let result = self.inner.list_with_delimiter(Some(&tenant_prefix)).await?;

        // Strip prefix from all objects and common prefixes
        let prefix_with_slash = format!("{}/", self.prefix);

        let objects = result
            .objects
            .into_iter()
            .map(|meta| {
                let path_str = meta.location.to_string();
                let stripped = if path_str.starts_with(&prefix_with_slash) {
                    Path::from(&path_str[prefix_with_slash.len()..])
                } else {
                    meta.location.clone()
                };
                ObjectMeta {
                    location: stripped,
                    last_modified: meta.last_modified,
                    size: meta.size,
                    e_tag: meta.e_tag,
                    version: meta.version,
                }
            })
            .collect();

        let common_prefixes = result
            .common_prefixes
            .into_iter()
            .map(|p| {
                let path_str = p.to_string();
                if path_str.starts_with(&prefix_with_slash) {
                    Path::from(&path_str[prefix_with_slash.len()..])
                } else {
                    p
                }
            })
            .collect();

        Ok(ListResult {
            objects,
            common_prefixes,
        })
    }

    /// Copy an object within the tenant's storage space.
    async fn copy(&self, from: &Path, to: &Path) -> Result<()> {
        let tenant_from = self.tenant_path(from);
        let tenant_to = self.tenant_path(to);
        tracing::trace!(
            from = %from,
            to = %to,
            tenant_from = %tenant_from,
            tenant_to = %tenant_to,
            "TenantObjectStore::copy"
        );
        self.inner.copy(&tenant_from, &tenant_to).await
    }

    /// Copy an object if not exists within the tenant's storage space.
    async fn copy_if_not_exists(&self, from: &Path, to: &Path) -> Result<()> {
        let tenant_from = self.tenant_path(from);
        let tenant_to = self.tenant_path(to);
        tracing::trace!(
            from = %from,
            to = %to,
            tenant_from = %tenant_from,
            tenant_to = %tenant_to,
            "TenantObjectStore::copy_if_not_exists"
        );
        self.inner
            .copy_if_not_exists(&tenant_from, &tenant_to)
            .await
    }

    /// Rename an object within the tenant's storage space.
    async fn rename(&self, from: &Path, to: &Path) -> Result<()> {
        let tenant_from = self.tenant_path(from);
        let tenant_to = self.tenant_path(to);
        tracing::trace!(
            from = %from,
            to = %to,
            tenant_from = %tenant_from,
            tenant_to = %tenant_to,
            "TenantObjectStore::rename"
        );
        self.inner.rename(&tenant_from, &tenant_to).await
    }

    /// Rename an object if not exists within the tenant's storage space.
    async fn rename_if_not_exists(&self, from: &Path, to: &Path) -> Result<()> {
        let tenant_from = self.tenant_path(from);
        let tenant_to = self.tenant_path(to);
        tracing::trace!(
            from = %from,
            to = %to,
            tenant_from = %tenant_from,
            tenant_to = %tenant_to,
            "TenantObjectStore::rename_if_not_exists"
        );
        self.inner
            .rename_if_not_exists(&tenant_from, &tenant_to)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use object_store::memory::InMemory;

    #[tokio::test]
    async fn test_tenant_path_prefixing() {
        let store = TenantObjectStore::new(
            Arc::new(InMemory::new()),
            "org-11111111-1111-1111-1111-111111111111",
        );

        let original = Path::from("data/orders/0/00000000000000000000.seg");
        let tenant_path = store.tenant_path(&original);

        assert_eq!(
            tenant_path.to_string(),
            "org-11111111-1111-1111-1111-111111111111/data/orders/0/00000000000000000000.seg"
        );
    }

    #[tokio::test]
    async fn test_strip_prefix() {
        let store = TenantObjectStore::new(
            Arc::new(InMemory::new()),
            "org-11111111-1111-1111-1111-111111111111",
        );

        let tenant_path = Path::from(
            "org-11111111-1111-1111-1111-111111111111/data/orders/0/00000000000000000000.seg",
        );
        let stripped = store.strip_prefix(&tenant_path);

        assert_eq!(
            stripped.to_string(),
            "data/orders/0/00000000000000000000.seg"
        );
    }

    #[tokio::test]
    async fn test_put_and_get() {
        let inner = Arc::new(InMemory::new());
        let store = TenantObjectStore::new(inner.clone(), "org-test");

        // Put data using tenant store
        let location = Path::from("data/test.seg");
        let data = Bytes::from("test data");
        store
            .put(&location, data.clone())
            .await
            .expect("put failed");

        // Get data using tenant store
        let result = store.get(&location).await.expect("get failed");
        let retrieved = result.bytes().await.expect("bytes failed");
        assert_eq!(retrieved, data);

        // Verify data is stored with prefix in inner store
        let inner_location = Path::from("org-test/data/test.seg");
        let inner_result = inner.get(&inner_location).await.expect("inner get failed");
        let inner_data = inner_result.bytes().await.expect("inner bytes failed");
        assert_eq!(inner_data, data);
    }

    #[tokio::test]
    async fn test_tenant_isolation() {
        let inner = Arc::new(InMemory::new());

        let tenant1 = TenantObjectStore::new(inner.clone(), "org-tenant1");
        let tenant2 = TenantObjectStore::new(inner.clone(), "org-tenant2");

        // Tenant 1 writes data
        let location = Path::from("data/shared-name.seg");
        tenant1
            .put(&location, Bytes::from("tenant1 data"))
            .await
            .expect("tenant1 put failed");

        // Tenant 2 writes data with same path
        tenant2
            .put(&location, Bytes::from("tenant2 data"))
            .await
            .expect("tenant2 put failed");

        // Each tenant sees only their own data
        let t1_result = tenant1.get(&location).await.expect("tenant1 get failed");
        let t1_data = t1_result.bytes().await.expect("tenant1 bytes failed");
        assert_eq!(t1_data, Bytes::from("tenant1 data"));

        let t2_result = tenant2.get(&location).await.expect("tenant2 get failed");
        let t2_data = t2_result.bytes().await.expect("tenant2 bytes failed");
        assert_eq!(t2_data, Bytes::from("tenant2 data"));
    }

    #[tokio::test]
    async fn test_from_organization_id() {
        let store = TenantObjectStore::from_organization_id(
            Arc::new(InMemory::new()),
            "11111111-1111-1111-1111-111111111111",
        );

        assert_eq!(store.prefix(), "org-11111111-1111-1111-1111-111111111111");
    }
}
