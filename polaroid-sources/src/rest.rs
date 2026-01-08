//! REST API data source with pagination strategies

use crate::error::{Result, SourceError};
use crate::traits::DataSource;
use arrow::record_batch::RecordBatch;
use arrow_schema::SchemaRef;
use async_stream::stream;
use futures::stream::Stream;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::pin::Pin;
use std::time::Duration;
use tracing::{debug, info};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PaginationStrategy {
    /// Offset-based pagination (e.g., ?offset=0&limit=100)
    Offset {
        limit: usize,
        offset_param: String,
        limit_param: String,
    },
    /// Cursor-based pagination (e.g., ?cursor=abc123)
    Cursor {
        cursor_param: String,
        next_cursor_field: String,
    },
    /// Link header pagination (RFC 5988)
    LinkHeader {
        rel: String,
    },
    /// Page number pagination (e.g., ?page=1&size=100)
    PageNumber {
        page_param: String,
        size_param: String,
        size: usize,
    },
}

impl Default for PaginationStrategy {
    fn default() -> Self {
        Self::Offset {
            limit: 100,
            offset_param: "offset".to_string(),
            limit_param: "limit".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RestApiConfig {
    /// Base URL
    pub base_url: String,
    /// Endpoint path
    pub endpoint: String,
    /// HTTP method (GET, POST)
    pub method: String,
    /// Request headers
    pub headers: HashMap<String, String>,
    /// Request body (for POST requests)
    pub body: Option<String>,
    /// Query parameters
    pub query_params: HashMap<String, String>,
    /// Pagination strategy
    pub pagination: PaginationStrategy,
    /// Timeout for each request (seconds)
    pub timeout_secs: u64,
    /// Maximum pages to fetch (0 = unlimited)
    pub max_pages: usize,
    /// Response data JSON path (e.g., "data.items")
    pub data_path: String,
}

impl Default for RestApiConfig {
    fn default() -> Self {
        Self {
            base_url: String::new(),
            endpoint: String::new(),
            method: "GET".to_string(),
            headers: HashMap::new(),
            body: None,
            query_params: HashMap::new(),
            pagination: PaginationStrategy::default(),
            timeout_secs: 30,
            max_pages: 0,
            data_path: "data".to_string(),
        }
    }
}

pub struct RestApiSource {
    config: RestApiConfig,
    schema: SchemaRef,
    client: Client,
}

impl RestApiSource {
    pub fn new(config: RestApiConfig, schema: SchemaRef) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .map_err(|e| SourceError::ConfigError(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self {
            config,
            schema,
            client,
        })
    }

    fn build_url(&self, params: &HashMap<String, String>) -> String {
        let mut url = format!("{}/{}", self.config.base_url.trim_end_matches('/'), self.config.endpoint.trim_start_matches('/'));

        if !params.is_empty() {
            let query_string: Vec<String> = params
                .iter()
                .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
                .collect();
            url.push_str("?");
            url.push_str(&query_string.join("&"));
        }

        url
    }

    async fn fetch_page(&self, params: HashMap<String, String>) -> Result<serde_json::Value> {
        let url = self.build_url(&params);
        debug!("Fetching: {}", url);

        let mut request = match self.config.method.to_uppercase().as_str() {
            "GET" => self.client.get(&url),
            "POST" => {
                let mut req = self.client.post(&url);
                if let Some(body) = &self.config.body {
                    req = req.body(body.clone());
                }
                req
            }
            method => return Err(SourceError::ConfigError(format!("Unsupported HTTP method: {}", method))),
        };

        // Add headers
        for (key, value) in &self.config.headers {
            request = request.header(key, value);
        }

        let response = request.send().await?;

        if !response.status().is_success() {
            return Err(SourceError::HttpError(format!(
                "HTTP {} - {}",
                response.status(),
                response.text().await.unwrap_or_default()
            )));
        }

        let json: serde_json::Value = response.json().await?;
        Ok(json)
    }

    fn extract_data<'a>(&self, json: &'a serde_json::Value) -> Result<&'a serde_json::Value> {
        let path_parts: Vec<&str> = self.config.data_path.split('.').collect();
        let mut current = json;

        for part in path_parts {
            current = current.get(part).ok_or_else(|| {
                SourceError::SerializationError(format!("Data path '{}' not found in response", self.config.data_path))
            })?;
        }

        Ok(current)
    }

    fn json_to_record_batch(&self, _json: &serde_json::Value) -> Result<RecordBatch> {
        // TODO: Implement JSON array to RecordBatch conversion
        // This requires parsing JSON according to schema
        Err(SourceError::SerializationError(
            "JSON to RecordBatch conversion not yet implemented".to_string(),
        ))
    }

    fn extract_next_cursor(&self, json: &serde_json::Value, cursor_field: &str) -> Option<String> {
        let path_parts: Vec<&str> = cursor_field.split('.').collect();
        let mut current = json;

        for part in path_parts {
            current = current.get(part)?;
        }

        current.as_str().map(|s| s.to_string())
    }

    fn extract_link_header(&self, headers: &reqwest::header::HeaderMap, rel: &str) -> Option<String> {
        // Parse Link header (RFC 5988)
        // Example: Link: <https://api.example.com/data?page=2>; rel="next"
        let link_header = headers.get("Link")?.to_str().ok()?;

        for link in link_header.split(',') {
            let parts: Vec<&str> = link.split(';').collect();
            if parts.len() != 2 {
                continue;
            }

            let url = parts[0].trim().trim_start_matches('<').trim_end_matches('>');
            let rel_part = parts[1].trim();

            if rel_part.contains(&format!("rel=\"{}\"", rel)) {
                return Some(url.to_string());
            }
        }

        None
    }
}

impl DataSource for RestApiSource {
    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }

    fn stream(&self) -> Pin<Box<dyn Stream<Item = Result<RecordBatch>> + Send + '_>> {
        let config = self.config.clone();

        let s = stream! {
            let mut page_count = 0;
            let mut params = config.query_params.clone();

            match &config.pagination {
                PaginationStrategy::Offset { limit, offset_param, limit_param } => {
                    let mut offset = 0;
                    params.insert(limit_param.clone(), limit.to_string());

                    loop {
                        if config.max_pages > 0 && page_count >= config.max_pages {
                            info!("Reached max pages: {}", config.max_pages);
                            break;
                        }

                        params.insert(offset_param.clone(), offset.to_string());

                        match self.fetch_page(params.clone()).await {
                            Ok(json) => {
                                match self.extract_data(&json) {
                                    Ok(data) => {
                                        // Convert to RecordBatch and yield
                                        match self.json_to_record_batch(data) {
                                            Ok(batch) => {
                                                let num_rows = batch.num_rows();
                                                yield Ok(batch);

                                                if num_rows < *limit {
                                                    debug!("Last page - got {} rows < {}", num_rows, limit);
                                                    break;
                                                }

                                                offset += limit;
                                                page_count += 1;
                                            }
                                            Err(e) => {
                                                yield Err(e);
                                                break;
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        yield Err(e);
                                        break;
                                    }
                                }
                            }
                            Err(e) => {
                                yield Err(e);
                                break;
                            }
                        }
                    }
                }

                PaginationStrategy::Cursor { cursor_param, next_cursor_field } => {
                    let mut cursor: Option<String> = None;

                    loop {
                        if config.max_pages > 0 && page_count >= config.max_pages {
                            break;
                        }

                        if let Some(c) = &cursor {
                            params.insert(cursor_param.clone(), c.clone());
                        }

                        match self.fetch_page(params.clone()).await {
                            Ok(json) => {
                                match self.extract_data(&json) {
                                    Ok(data) => {
                                        match self.json_to_record_batch(data) {
                                            Ok(batch) => {
                                                yield Ok(batch);

                                                // Get next cursor
                                                cursor = self.extract_next_cursor(&json, next_cursor_field);
                                                if cursor.is_none() {
                                                    debug!("No more pages - cursor is None");
                                                    break;
                                                }

                                                page_count += 1;
                                            }
                                            Err(e) => {
                                                yield Err(e);
                                                break;
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        yield Err(e);
                                        break;
                                    }
                                }
                            }
                            Err(e) => {
                                yield Err(e);
                                break;
                            }
                        }
                    }
                }

                _ => {
                    yield Err(SourceError::ConfigError("Pagination strategy not yet implemented".to_string()));
                }
            }
        };

        Box::pin(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::datatypes::{DataType, Field, Schema};
    use std::sync::Arc;

    #[test]
    fn test_pagination_strategy_default() {
        let strategy = PaginationStrategy::default();
        match strategy {
            PaginationStrategy::Offset { limit, .. } => {
                assert_eq!(limit, 100);
            }
            _ => panic!("Expected Offset strategy"),
        }
    }

    #[test]
    fn test_build_url() {
        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));

        let config = RestApiConfig {
            base_url: "https://api.example.com".to_string(),
            endpoint: "/data".to_string(),
            ..Default::default()
        };

        let source = RestApiSource::new(config, schema).unwrap();

        let mut params = HashMap::new();
        params.insert("page".to_string(), "1".to_string());
        params.insert("size".to_string(), "100".to_string());

        let url = source.build_url(&params);
        assert!(url.contains("https://api.example.com/data?"));
        assert!(url.contains("page=1"));
        assert!(url.contains("size=100"));
    }
}
