//! Generic adaptive streaming sources
//! 
//! This module provides a trait-based architecture for pluggable data sources
//! with adaptive streaming capabilities. All sources implement the `StreamingSource`
//! trait, allowing consistent API and behavior across different backends.

use std::sync::Arc;
use polars::prelude::*;

pub mod csv;
pub mod cloud;
pub mod database;
pub mod filesystem;
pub mod http;
pub mod kafka;

mod config;
mod error;
mod traits;

pub use config::*;
pub use error::{SourceError, SourceResult};
pub use traits::*;

/// Registry for creating sources by type
pub struct SourceRegistry {
    factories: std::collections::HashMap<String, Box<dyn SourceFactory>>,
}

impl SourceRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            factories: std::collections::HashMap::new(),
        };
        
        // Register built-in sources
        registry.register("csv", Box::new(csv::CsvSourceFactory));
        registry.register("filesystem", Box::new(filesystem::FilesystemSourceFactory));
        registry.register("http", Box::new(http::HttpSourceFactory));
        registry.register("s3", Box::new(cloud::s3::S3SourceFactory));
        registry.register("azure", Box::new(cloud::azure::AzureSourceFactory));
        registry.register("gcs", Box::new(cloud::gcs::GcsSourceFactory));
        registry.register("dynamodb", Box::new(database::dynamodb::DynamoDbSourceFactory));
        registry.register("kafka", Box::new(kafka::KafkaSourceFactory));
        
        registry
    }
    
    pub fn register(&mut self, name: &str, factory: Box<dyn SourceFactory>) {
        self.factories.insert(name.to_string(), factory);
    }
    
    pub fn create(&self, source_type: &str, config: SourceConfig) -> SourceResult<Box<dyn StreamingSource>> {
        self.factories
            .get(source_type)
            .ok_or_else(|| SourceError::UnsupportedSource(source_type.to_string()))?
            .create(config)
    }
}

impl Default for SourceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_registry_creation() {
        let registry = SourceRegistry::new();
        assert!(registry.factories.contains_key("csv"));
        assert!(registry.factories.contains_key("s3"));
        assert!(registry.factories.contains_key("kafka"));
    }
}
