use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub bucket_name: String,
    pub endpoint_url: Option<String>,
    pub access_key_id: String,
    #[serde(skip_serializing)] // Never serialize secrets
    pub secret_access_key: String,
    pub region: String,
}

impl StorageConfig {
    /// Load storage configuration from environment variables
    pub fn from_env(prefix: &str) -> Result<Self, String> {
        let bucket_name = std::env::var(format!("{}_BUCKET_NAME", prefix))
            .map_err(|_| format!("{}_BUCKET_NAME not set", prefix))?;

        let endpoint_url = std::env::var(format!("{}_BUCKET_ENDPOINT", prefix)).ok();

        let access_key_id = std::env::var(format!("{}_BUCKET_ACCESS_KEY_ID", prefix))
            .map_err(|_| format!("{}_BUCKET_ACCESS_KEY_ID not set", prefix))?;

        let secret_access_key = std::env::var(format!("{}_BUCKET_SECRET_ACCESS_KEY", prefix))
            .map_err(|_| format!("{}_BUCKET_SECRET_ACCESS_KEY not set", prefix))?;

        let region = std::env::var(format!("{}_BUCKET_REGION", prefix))
            .unwrap_or_else(|_| "us-east-1".to_string());

        Ok(StorageConfig {
            bucket_name,
            endpoint_url,
            access_key_id,
            secret_access_key,
            region,
        })
    }
}
