use rendrr::models::storage::StorageConfig;

#[test]
fn from_env_with_all_vars_set() {
    // Use a unique prefix to avoid conflicts with other tests
    let prefix = "SCTEST1";
    std::env::set_var(format!("{}_BUCKET_NAME", prefix), "my-bucket");
    std::env::set_var(format!("{}_BUCKET_ACCESS_KEY_ID", prefix), "key123");
    std::env::set_var(format!("{}_BUCKET_SECRET_ACCESS_KEY", prefix), "secret456");
    std::env::set_var(format!("{}_BUCKET_REGION", prefix), "eu-west-1");
    std::env::set_var(
        format!("{}_BUCKET_ENDPOINT", prefix),
        "http://localhost:9000",
    );

    let config = StorageConfig::from_env(prefix).unwrap();
    assert_eq!(config.bucket_name, "my-bucket");
    assert_eq!(config.access_key_id, "key123");
    assert_eq!(config.secret_access_key, "secret456");
    assert_eq!(config.region, "eu-west-1");
    assert_eq!(
        config.endpoint_url,
        Some("http://localhost:9000".to_string())
    );

    // Cleanup
    std::env::remove_var(format!("{}_BUCKET_NAME", prefix));
    std::env::remove_var(format!("{}_BUCKET_ACCESS_KEY_ID", prefix));
    std::env::remove_var(format!("{}_BUCKET_SECRET_ACCESS_KEY", prefix));
    std::env::remove_var(format!("{}_BUCKET_REGION", prefix));
    std::env::remove_var(format!("{}_BUCKET_ENDPOINT", prefix));
}

#[test]
fn from_env_defaults_region() {
    let prefix = "SCTEST2";
    std::env::set_var(format!("{}_BUCKET_NAME", prefix), "bucket");
    std::env::set_var(format!("{}_BUCKET_ACCESS_KEY_ID", prefix), "key");
    std::env::set_var(format!("{}_BUCKET_SECRET_ACCESS_KEY", prefix), "secret");
    // No region set — should default to us-east-1

    let config = StorageConfig::from_env(prefix).unwrap();
    assert_eq!(config.region, "us-east-1");

    std::env::remove_var(format!("{}_BUCKET_NAME", prefix));
    std::env::remove_var(format!("{}_BUCKET_ACCESS_KEY_ID", prefix));
    std::env::remove_var(format!("{}_BUCKET_SECRET_ACCESS_KEY", prefix));
}

#[test]
fn from_env_endpoint_is_optional() {
    let prefix = "SCTEST3";
    std::env::set_var(format!("{}_BUCKET_NAME", prefix), "bucket");
    std::env::set_var(format!("{}_BUCKET_ACCESS_KEY_ID", prefix), "key");
    std::env::set_var(format!("{}_BUCKET_SECRET_ACCESS_KEY", prefix), "secret");
    // No endpoint set

    let config = StorageConfig::from_env(prefix).unwrap();
    assert!(config.endpoint_url.is_none());

    std::env::remove_var(format!("{}_BUCKET_NAME", prefix));
    std::env::remove_var(format!("{}_BUCKET_ACCESS_KEY_ID", prefix));
    std::env::remove_var(format!("{}_BUCKET_SECRET_ACCESS_KEY", prefix));
}

#[test]
fn from_env_missing_bucket_name_fails() {
    let prefix = "SCTEST4";
    // Don't set BUCKET_NAME
    std::env::set_var(format!("{}_BUCKET_ACCESS_KEY_ID", prefix), "key");
    std::env::set_var(format!("{}_BUCKET_SECRET_ACCESS_KEY", prefix), "secret");

    let result = StorageConfig::from_env(prefix);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("BUCKET_NAME"));

    std::env::remove_var(format!("{}_BUCKET_ACCESS_KEY_ID", prefix));
    std::env::remove_var(format!("{}_BUCKET_SECRET_ACCESS_KEY", prefix));
}

#[test]
fn from_env_missing_access_key_fails() {
    let prefix = "SCTEST5";
    std::env::set_var(format!("{}_BUCKET_NAME", prefix), "bucket");
    // Don't set ACCESS_KEY_ID
    std::env::set_var(format!("{}_BUCKET_SECRET_ACCESS_KEY", prefix), "secret");

    let result = StorageConfig::from_env(prefix);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("ACCESS_KEY_ID"));

    std::env::remove_var(format!("{}_BUCKET_NAME", prefix));
    std::env::remove_var(format!("{}_BUCKET_SECRET_ACCESS_KEY", prefix));
}

#[test]
fn from_env_missing_secret_key_fails() {
    let prefix = "SCTEST6";
    std::env::set_var(format!("{}_BUCKET_NAME", prefix), "bucket");
    std::env::set_var(format!("{}_BUCKET_ACCESS_KEY_ID", prefix), "key");
    // Don't set SECRET_ACCESS_KEY

    let result = StorageConfig::from_env(prefix);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("SECRET_ACCESS_KEY"));

    std::env::remove_var(format!("{}_BUCKET_NAME", prefix));
    std::env::remove_var(format!("{}_BUCKET_ACCESS_KEY_ID", prefix));
}

#[test]
fn secret_key_not_serialized() {
    let config = StorageConfig {
        bucket_name: "b".to_string(),
        endpoint_url: None,
        access_key_id: "k".to_string(),
        secret_access_key: "SUPER_SECRET".to_string(),
        region: "us-east-1".to_string(),
    };

    let json = serde_json::to_string(&config).unwrap();
    assert!(!json.contains("SUPER_SECRET"));
    assert!(!json.contains("secret_access_key"));
}
