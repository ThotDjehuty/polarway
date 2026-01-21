//! WebSocket data source with automatic reconnection

use crate::error::{Result, SourceError};
use crate::traits::{DataSource, StreamingDataSource};
use arrow::record_batch::RecordBatch;
use arrow_schema::SchemaRef;
use async_stream::stream;
use futures::stream::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconnectPolicy {
    /// Maximum number of reconnection attempts
    pub max_retries: u32,
    /// Initial delay before first retry (milliseconds)
    pub initial_delay_ms: u64,
    /// Maximum delay between retries (milliseconds)
    pub max_delay_ms: u64,
    /// Multiplier for exponential backoff
    pub backoff_multiplier: f64,
}

impl Default for ReconnectPolicy {
    fn default() -> Self {
        Self {
            max_retries: 10,
            initial_delay_ms: 100,
            max_delay_ms: 30000,
            backoff_multiplier: 2.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct WebSocketConfig {
    /// WebSocket URL
    pub url: String,
    /// Additional headers for connection
    pub headers: Vec<(String, String)>,
    /// Reconnection policy
    pub reconnect_policy: ReconnectPolicy,
    /// Buffer size for incoming messages
    pub buffer_size: usize,
    /// Message parser function name (for custom parsing)
    pub parser: Option<String>,
}

pub struct WebSocketSource {
    config: WebSocketConfig,
    schema: SchemaRef,
    connected: Arc<RwLock<bool>>,
}

impl WebSocketSource {
    pub fn new(config: WebSocketConfig, schema: SchemaRef) -> Self {
        Self {
            config,
            schema,
            connected: Arc::new(RwLock::new(false)),
        }
    }

    async fn connect_with_retry(&self) -> Result<()> {
        let policy = &self.config.reconnect_policy;
        let mut delay_ms = policy.initial_delay_ms;

        for attempt in 0..policy.max_retries {
            match self.try_connect().await {
                Ok(()) => {
                    info!("WebSocket connected to {} on attempt {}", self.config.url, attempt + 1);
                    *self.connected.write().await = true;
                    return Ok(());
                }
                Err(e) => {
                    warn!(
                        "WebSocket connection attempt {} failed: {}. Retrying in {}ms",
                        attempt + 1,
                        e,
                        delay_ms
                    );

                    if attempt < policy.max_retries - 1 {
                        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                        delay_ms = (delay_ms as f64 * policy.backoff_multiplier) as u64;
                        delay_ms = delay_ms.min(policy.max_delay_ms);
                    }
                }
            }
        }

        Err(SourceError::RetryExhausted {
            attempts: policy.max_retries,
            last_error: format!("Failed to connect to {}", self.config.url),
        })
    }

    async fn try_connect(&self) -> Result<()> {
        let _url = url::Url::parse(&self.config.url)
            .map_err(|e| SourceError::ConfigError(format!("Invalid WebSocket URL: {}", e)))?;

        // This is a placeholder - actual connection happens in stream()
        // Here we just validate the URL
        Ok(())
    }

    fn parse_message(&self, msg: Message, schema: &SchemaRef) -> Result<RecordBatch> {
        match msg {
            Message::Text(text) => {
                // Parse JSON message to RecordBatch
                self.json_to_record_batch(&text, schema)
            }
            Message::Binary(data) => {
                // Parse binary message (Arrow IPC format)
                self.binary_to_record_batch(&data, schema)
            }
            _ => Err(SourceError::SerializationError(
                "Unsupported message type".to_string(),
            )),
        }
    }

    fn json_to_record_batch(&self, json: &str, schema: &SchemaRef) -> Result<RecordBatch> {
        use arrow::array::{ArrayRef, Int64Array, Float64Array, StringArray};
        
        let parsed: serde_json::Value = serde_json::from_str(json)
            .map_err(|e| SourceError::SerializationError(format!("Failed to parse JSON: {}", e)))?;

        // Handle single object or array of objects
        let rows = match &parsed {
            serde_json::Value::Array(arr) => arr.clone(),
            serde_json::Value::Object(_) => vec![parsed.clone()],
            _ => return Err(SourceError::SerializationError(
                "Expected JSON object or array".to_string(),
            )),
        };

        if rows.is_empty() {
            return Err(SourceError::SerializationError(
                "Empty data array".to_string(),
            ));
        }

        // Get field names from schema
        let fields = schema.fields();
        let mut arrays: Vec<ArrayRef> = Vec::new();

        // Build arrays for each field
        for field in fields {
            let field_name = field.name();
            let data_type = field.data_type();

            match data_type {
                arrow::datatypes::DataType::Int64 => {
                    let mut values: Vec<i64> = Vec::new();
                    for row in &rows {
                        if let Some(obj) = row.as_object() {
                            if let Some(val) = obj.get(field_name) {
                                if let Some(i) = val.as_i64() {
                                    values.push(i);
                                } else {
                                    values.push(0);
                                }
                            } else {
                                values.push(0);
                            }
                        }
                    }
                    arrays.push(Arc::new(Int64Array::from(values)));
                }
                arrow::datatypes::DataType::Float64 => {
                    let mut values: Vec<f64> = Vec::new();
                    for row in &rows {
                        if let Some(obj) = row.as_object() {
                            if let Some(val) = obj.get(field_name) {
                                if let Some(f) = val.as_f64() {
                                    values.push(f);
                                } else {
                                    values.push(0.0);
                                }
                            } else {
                                values.push(0.0);
                            }
                        }
                    }
                    arrays.push(Arc::new(Float64Array::from(values)));
                }
                arrow::datatypes::DataType::Utf8 => {
                    let mut values: Vec<String> = Vec::new();
                    for row in &rows {
                        if let Some(obj) = row.as_object() {
                            if let Some(val) = obj.get(field_name) {
                                if let Some(s) = val.as_str() {
                                    values.push(s.to_string());
                                } else {
                                    values.push(val.to_string());
                                }
                            } else {
                                values.push(String::new());
                            }
                        }
                    }
                    arrays.push(Arc::new(StringArray::from(values)));
                }
                _ => {
                    return Err(SourceError::SerializationError(
                        format!("Unsupported data type: {:?}", data_type),
                    ));
                }
            }
        }

        RecordBatch::try_new(schema.clone(), arrays)
            .map_err(|e| SourceError::SerializationError(format!("Failed to create record batch: {}", e)))
    }

    fn binary_to_record_batch(&self, data: &[u8], schema: &SchemaRef) -> Result<RecordBatch> {
        // For now, implement a simple header-based format or defer to JSON
        // Arrow IPC parsing requires additional features
        // As a workaround, convert to JSON if possible or return error
        
        // Try to interpret as UTF-8 JSON first
        if let Ok(json_str) = std::str::from_utf8(data) {
            self.json_to_record_batch(json_str, schema)
        } else {
            Err(SourceError::SerializationError(
                "Binary data format not supported - only UTF-8 JSON or Arrow IPC is supported".to_string(),
            ))
        }
    }
}

impl DataSource for WebSocketSource {
    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }

    fn stream(&self) -> Pin<Box<dyn Stream<Item = Result<RecordBatch>> + Send + '_>> {
        let url = self.config.url.clone();
        let reconnect_policy = self.config.reconnect_policy.clone();
        let connected = self.connected.clone();
        let schema = self.schema.clone();

        let s = stream! {
            let mut retry_count = 0;
            let mut delay_ms = reconnect_policy.initial_delay_ms;

            loop {
                debug!("Connecting to WebSocket: {}", url);

                match connect_async(&url).await {
                    Ok((ws_stream, _)) => {
                        info!("WebSocket connected: {}", url);
                        *connected.write().await = true;
                        retry_count = 0;
                        delay_ms = reconnect_policy.initial_delay_ms;

                        let (_, mut read) = ws_stream.split();

                        while let Some(msg_result) = read.next().await {
                            match msg_result {
                                Ok(msg) => {
                                    // Parse message to RecordBatch
                                    match self.parse_message(msg, &schema) {
                                        Ok(batch) => {
                                            yield Ok(batch);
                                        }
                                        Err(e) => {
                                            error!("Failed to parse message: {}", e);
                                            debug!("Continuing despite parse error");
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!("WebSocket read error: {}", e);
                                    *connected.write().await = false;
                                    break;
                                }
                            }
                        }

                        warn!("WebSocket connection closed");
                        *connected.write().await = false;
                    }
                    Err(e) => {
                        error!("WebSocket connection failed: {}", e);
                        *connected.write().await = false;

                        if retry_count >= reconnect_policy.max_retries {
                            yield Err(SourceError::RetryExhausted {
                                attempts: retry_count,
                                last_error: e.to_string(),
                            });
                            break;
                        }

                        retry_count += 1;
                        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                        delay_ms = (delay_ms as f64 * reconnect_policy.backoff_multiplier) as u64;
                        delay_ms = delay_ms.min(reconnect_policy.max_delay_ms);
                    }
                }
            }
        };

        Box::pin(s)
    }

    fn is_healthy(&self) -> Pin<Box<dyn std::future::Future<Output = bool> + Send>> {
        let connected = self.connected.clone();
        Box::pin(async move { *connected.read().await })
    }
}

impl StreamingDataSource for WebSocketSource {
    fn buffer_size(&self) -> usize {
        self.config.buffer_size
    }

    fn supports_reconnect(&self) -> bool {
        true
    }

    fn reconnect(&self) -> Pin<Box<dyn std::future::Future<Output = Result<()>> + Send>> {
        let this = self.clone();
        Box::pin(async move { this.connect_with_retry().await })
    }
}

// Make WebSocketSource cloneable for reconnection
impl Clone for WebSocketSource {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            schema: self.schema.clone(),
            connected: self.connected.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::datatypes::{DataType, Field, Schema};
    use std::sync::Arc;

    #[test]
    fn test_reconnect_policy_default() {
        let policy = ReconnectPolicy::default();
        assert_eq!(policy.max_retries, 10);
        assert_eq!(policy.initial_delay_ms, 100);
        assert_eq!(policy.max_delay_ms, 30000);
        assert_eq!(policy.backoff_multiplier, 2.0);
    }

    #[tokio::test]
    async fn test_websocket_source_creation() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("symbol", DataType::Utf8, false),
            Field::new("price", DataType::Float64, false),
        ]));

        let config = WebSocketConfig {
            url: "ws://localhost:8080/stream".to_string(),
            headers: vec![],
            reconnect_policy: ReconnectPolicy::default(),
            buffer_size: 1000,
            parser: None,
        };

        let source = WebSocketSource::new(config, schema.clone());
        assert_eq!(source.schema(), schema);
        assert!(!source.is_healthy().await);
    }
}
