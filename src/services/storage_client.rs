use crate::errors::StorageError;
use crate::models::StorageConfig;
use bytes::Bytes;
use object_store::{aws::AmazonS3Builder, ObjectStore, ObjectStoreExt};
use std::sync::Arc;

#[derive(Clone)]
pub struct StorageClient {
    store: Arc<dyn ObjectStore>,
}

impl StorageClient {
    pub fn new(config: StorageConfig) -> Result<Self, StorageError> {
        let mut builder = AmazonS3Builder::new()
            .with_bucket_name(&config.bucket_name)
            .with_region(&config.region)
            .with_access_key_id(&config.access_key_id)
            .with_secret_access_key(&config.secret_access_key);

        if let Some(endpoint) = &config.endpoint_url {
            builder = builder.with_endpoint(endpoint).with_allow_http(true); // Allow HTTP for local MinIO
        }

        let store = builder
            .build()
            .map_err(|e| StorageError::ConfigError(e.to_string()))?;

        Ok(Self {
            store: Arc::new(store),
        })
    }

    /// Upload a file to S3
    pub async fn put(&self, path: &str, data: Bytes) -> Result<(), StorageError> {
        let location = object_store::path::Path::from(path);

        self.store
            .put(&location, data.into())
            .await
            .map_err(|e| StorageError::UploadFailed(e.to_string()))?;

        Ok(())
    }

    /// Download a file from S3. Returns `FileNotFound` for missing objects.
    pub async fn get(&self, path: &str) -> Result<Bytes, StorageError> {
        let location = object_store::path::Path::from(path);

        let result = self.store.get(&location).await.map_err(|e| match e {
            object_store::Error::NotFound { .. } => StorageError::FileNotFound(path.to_string()),
            other => StorageError::DownloadFailed(other.to_string()),
        })?;

        let bytes = result
            .bytes()
            .await
            .map_err(|e| StorageError::DownloadFailed(e.to_string()))?;

        Ok(bytes)
    }

    /// Check if a file exists in S3
    pub async fn exists(&self, path: &str) -> Result<bool, StorageError> {
        let location = object_store::path::Path::from(path);

        match self.store.head(&location).await {
            Ok(_) => Ok(true),
            Err(object_store::Error::NotFound { .. }) => Ok(false),
            Err(e) => Err(StorageError::S3Error(e)),
        }
    }

    /// Delete a file from S3. Idempotent — deleting a missing object is not an error.
    pub async fn delete(&self, path: &str) -> Result<(), StorageError> {
        let location = object_store::path::Path::from(path);
        match self.store.delete(&location).await {
            Ok(()) | Err(object_store::Error::NotFound { .. }) => Ok(()),
            Err(e) => Err(StorageError::S3Error(e)),
        }
    }

    /// Construct a `StorageClient` from a pre-built `ObjectStore`. Used by tests
    /// that want to back the client with an in-memory implementation.
    pub fn from_store(store: Arc<dyn ObjectStore>) -> Self {
        Self { store }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use object_store::memory::InMemory;

    fn mem_client() -> StorageClient {
        StorageClient::from_store(Arc::new(InMemory::new()))
    }

    #[tokio::test]
    async fn put_then_get_round_trips_bytes() {
        let client = mem_client();
        client
            .put("a/b.txt", Bytes::from_static(b"hello"))
            .await
            .unwrap();
        let got = client.get("a/b.txt").await.unwrap();
        assert_eq!(got.as_ref(), b"hello");
    }

    #[tokio::test]
    async fn get_missing_file_returns_file_not_found() {
        let client = mem_client();
        let err = client.get("does/not/exist.txt").await.unwrap_err();
        assert!(matches!(err, StorageError::FileNotFound(_)));
    }

    #[tokio::test]
    async fn exists_returns_false_for_missing() {
        let client = mem_client();
        let exists = client.exists("nope.txt").await.unwrap();
        assert!(!exists);
    }

    #[tokio::test]
    async fn exists_returns_true_after_put() {
        let client = mem_client();
        client
            .put("yes.txt", Bytes::from_static(b"x"))
            .await
            .unwrap();
        assert!(client.exists("yes.txt").await.unwrap());
    }

    #[tokio::test]
    async fn delete_removes_object() {
        let client = mem_client();
        client.put("d.txt", Bytes::from_static(b"x")).await.unwrap();
        client.delete("d.txt").await.unwrap();
        assert!(!client.exists("d.txt").await.unwrap());
    }

    #[tokio::test]
    async fn delete_missing_is_idempotent() {
        let client = mem_client();
        client.delete("missing.txt").await.unwrap();
    }

    #[test]
    fn new_with_endpoint_succeeds() {
        let cfg = crate::models::StorageConfig {
            bucket_name: "bucket".into(),
            endpoint_url: Some("http://localhost:9000".into()),
            access_key_id: "k".into(),
            secret_access_key: "s".into(),
            region: "us-east-1".into(),
        };
        StorageClient::new(cfg).unwrap();
    }

    #[test]
    fn new_without_endpoint_succeeds() {
        let cfg = crate::models::StorageConfig {
            bucket_name: "bucket".into(),
            endpoint_url: None,
            access_key_id: "k".into(),
            secret_access_key: "s".into(),
            region: "us-east-1".into(),
        };
        StorageClient::new(cfg).unwrap();
    }
}
