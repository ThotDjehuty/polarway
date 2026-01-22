//! Configuration types for streaming sources

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Generic source configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceConfig {
    /// Source location (path, URL, connection string, etc.)
    pub location: String,
    
    /// Optional authentication credentials
    pub credentials: Option<Credentials>,
    
    /// Memory limit for adaptive streaming (bytes)
    pub memory_limit: Option<usize>,
    
    /// Initial chunk size
    pub chunk_size: Option<usize>,
    
    /// Enable parallel reading
    pub parallel: bool,
    
    /// Enable prefetching
    pub prefetch: bool,
    
    /// Additional provider-specific options
    pub options: HashMap<String, String>,
}

impl SourceConfig {
    pub fn new(location: impl Into<String>) -> Self {
        Self {
            location: location.into(),
            credentials: None,
            memory_limit: None,
            chunk_size: None,
            parallel: false,
            prefetch: true,
            options: HashMap::new(),
        }
    }
    
    pub fn with_credentials(mut self, credentials: Credentials) -> Self {
        self.credentials = Some(credentials);
        self
    }
    
    pub fn with_memory_limit(mut self, bytes: usize) -> Self {
        self.memory_limit = Some(bytes);
        self
    }
    
    pub fn with_chunk_size(mut self, size: usize) -> Self {
        self.chunk_size = Some(size);
        self
    }
    
    pub fn with_parallel(mut self, enable: bool) -> Self {
        self.parallel = enable;
        self
    }
    
    pub fn with_option(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.options.insert(key.into(), value.into());
        self
    }
}

/// Authentication credentials for various sources
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Credentials {
    /// AWS credentials
    Aws {
        access_key_id: String,
        secret_access_key: String,
        region: Option<String>,
        session_token: Option<String>,
    },
    
    /// Azure credentials
    Azure {
        account_name: String,
        account_key: String,
    },
    
    /// Google Cloud credentials
    Gcs {
        project_id: String,
        credentials_json: String,
    },
    
    /// Basic HTTP authentication
    Basic {
        username: String,
        password: String,
    },
    
    /// Bearer token authentication
    Bearer {
        token: String,
    },
    
    /// API key authentication
    ApiKey {
        key: String,
        header_name: Option<String>,
    },
    
    /// OAuth2 credentials
    OAuth2 {
        token: String,
        refresh_token: Option<String>,
    },
    
    /// DynamoDB credentials
    DynamoDB {
        access_key_id: String,
        secret_access_key: String,
        region: String,
    },
    
    /// Kafka credentials
    Kafka {
        username: Option<String>,
        password: Option<String>,
        sasl_mechanism: Option<String>,
    },
}

/// CSV-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CsvConfig {
    pub delimiter: u8,
    pub has_header: bool,
    pub skip_rows: usize,
    pub null_values: Option<Vec<String>>,
    pub comment_char: Option<u8>,
    pub quote_char: Option<u8>,
    pub encoding: String,
}

impl Default for CsvConfig {
    fn default() -> Self {
        Self {
            delimiter: b',',
            has_header: true,
            skip_rows: 0,
            null_values: None,
            comment_char: None,
            quote_char: Some(b'"'),
            encoding: "utf-8".to_string(),
        }
    }
}

/// HTTP-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpConfig {
    pub method: String,
    pub headers: HashMap<String, String>,
    pub timeout_ms: u64,
    pub retry_attempts: usize,
    pub retry_delay_ms: u64,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            method: "GET".to_string(),
            headers: HashMap::new(),
            timeout_ms: 30000,
            retry_attempts: 3,
            retry_delay_ms: 1000,
        }
    }
}

/// Kafka-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KafkaConfig {
    pub brokers: Vec<String>,
    pub topic: String,
    pub group_id: String,
    pub auto_offset_reset: String,
    pub max_poll_records: usize,
}

impl Default for KafkaConfig {
    fn default() -> Self {
        Self {
            brokers: vec!["localhost:9092".to_string()],
            topic: String::new(),
            group_id: "polaroid-consumer".to_string(),
            auto_offset_reset: "earliest".to_string(),
            max_poll_records: 500,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_source_config_builder() {
        let config = SourceConfig::new("s3://bucket/data.parquet")
            .with_memory_limit(4_000_000_000)
            .with_chunk_size(100_000)
            .with_parallel(true)
            .with_option("compression", "snappy");
        
        assert_eq!(config.location, "s3://bucket/data.parquet");
        assert_eq!(config.memory_limit, Some(4_000_000_000));
        assert!(config.parallel);
        assert_eq!(config.options.get("compression"), Some(&"snappy".to_string()));
    }
    
    #[test]
    fn test_csv_config_default() {
        let config = CsvConfig::default();
        assert_eq!(config.delimiter, b',');
        assert!(config.has_header);
        assert_eq!(config.skip_rows, 0);
    }
}
