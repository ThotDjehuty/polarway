use tonic::{Request, Response, Status};
use tokio_stream::wrappers::ReceiverStream;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info};
use polars::prelude::*;
use polars_io::ipc::IpcWriter;
use polars_io::csv::{CsvWriter, CsvReader};
use polars_lazy::frame::LazyCsvReader;
use polars_utils::plpath::PlPath;
use std::io::Cursor;

use crate::proto::{
    data_frame_service_server::DataFrameService,
    *,
};
use crate::handles::HandleManager;
use crate::error::{PolaroidError, Result};
use crate::optimizations::{FastArrowSerializer, ParallelFilter, FastGroupBy};

pub struct PolaroidDataFrameService {
    handle_manager: Arc<HandleManager>,
}

impl PolaroidDataFrameService {
    pub fn new() -> Self {
        let handle_manager = Arc::new(HandleManager::default());
        
        // Spawn cleanup task
        let manager_clone = Arc::clone(&handle_manager);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(300)); // Clean every 5 min
            loop {
                interval.tick().await;
                manager_clone.cleanup_expired();
            }
        });
        
        Self { handle_manager }
    }
    
    /// Convert Polars DataFrame to Arrow IPC bytes (optimized)
    fn dataframe_to_arrow_ipc(df: &DataFrame) -> Result<Vec<u8>> {
        // Use zero-copy serialization
        FastArrowSerializer::to_ipc_zero_copy(df)
    }
    
    /// Convert JSON string to DataFrame (helper for streaming)
    fn json_to_dataframe(json_str: &str) -> Result<DataFrame> {
        use std::io::Cursor;
        let cursor = Cursor::new(json_str.as_bytes());
        JsonReader::new(cursor)
            .finish()
            .map_err(|e| PolaroidError::Polars(e))
    }
}

#[tonic::async_trait]
impl DataFrameService for PolaroidDataFrameService {
    // Associated types for streaming responses
    type CollectStream = tokio_stream::wrappers::ReceiverStream<std::result::Result<ArrowBatch, Status>>;
    type StreamWebSocketStream = tokio_stream::wrappers::ReceiverStream<std::result::Result<ArrowBatch, Status>>;
    type StreamRestApiStream = tokio_stream::wrappers::ReceiverStream<std::result::Result<ArrowBatch, Status>>;
    type StreamGrpcStream = tokio_stream::wrappers::ReceiverStream<std::result::Result<ArrowBatch, Status>>;
    type StreamMessageQueueStream = tokio_stream::wrappers::ReceiverStream<std::result::Result<ArrowBatch, Status>>;
    type CollectStreamingStream = tokio_stream::wrappers::ReceiverStream<std::result::Result<ArrowBatch, Status>>;
    
    /// Read Parquet file (optimized)
    async fn read_parquet(
        &self,
        request: Request<ReadParquetRequest>,
    ) -> std::result::Result<Response<DataFrameHandle>, Status> {
        let req = request.into_inner();
        info!("ReadParquet request: path={}", req.path);
        
        // Read parquet file with optimizations
        let path_buf = std::path::PathBuf::from(req.path);
        let path = PlPath::Local(Arc::from(path_buf.into_boxed_path()));
        let mut lf = LazyFrame::scan_parquet(path, Default::default())
            .map_err(|e| PolaroidError::Polars(e))?;
        
        // Apply projection pushdown if columns specified
        if !req.columns.is_empty() {
            lf = lf.select(&req.columns.iter().map(|s| col(s)).collect::<Vec<_>>());
        }
        
        // Apply predicate pushdown if specified
        if let Some(predicate) = req.predicate {
            if !predicate.is_empty() {
                debug!("Predicate pushdown: {}", predicate);
                // TODO: Parse predicate string into Expr in Phase 2
            }
        }
        
        // Apply row limit if specified
        if let Some(n_rows) = req.n_rows {
            if n_rows > 0 {
                lf = lf.limit(n_rows.try_into().unwrap_or(IdxSize::MAX));
            }
        }
        
        // Enable streaming and parallel execution
        lf = lf.with_streaming(true);
        
        // Collect to DataFrame with optimizations
        let df = lf.collect()
            .map_err(|e| PolaroidError::Polars(e))?;
        
        info!("Loaded DataFrame: shape={:?}, columns={:?}", 
              df.shape(), df.get_column_names());
        
        // Create handle
        let handle = self.handle_manager.create_handle(df);
        
        Ok(Response::new(DataFrameHandle {
            handle,
            error: None,
        }))
    }
    
    /// Write Parquet file
    async fn write_parquet(
        &self,
        request: Request<WriteParquetRequest>,
    ) -> std::result::Result<Response<WriteResponse>, Status> {
        let req = request.into_inner();
        info!("WriteParquet request: handle={}, path={}", req.handle, req.path);
        
        // Get DataFrame
        let df = self.handle_manager.get_dataframe(&req.handle)
            .map_err(|e| Status::from(e))?;
        
        // Write to parquet
        let mut file = std::fs::File::create(&req.path)
            .map_err(|e| PolaroidError::Io(e))?;
        
        ParquetWriter::new(&mut file)
            .finish(&mut (*df).clone())
            .map_err(|e| PolaroidError::Polars(e))?;
        
        let rows_written = df.height() as i64;
        info!("Wrote {} rows to {}", rows_written, req.path);
        
        Ok(Response::new(WriteResponse {
            success: true,
            error: None,
            rows_written: Some(rows_written),
        }))
    }
    
    /// Filter DataFrame
    async fn filter(
        &self,
        request: Request<FilterRequest>,
    ) -> std::result::Result<Response<DataFrameHandle>, Status> {
        let req = request.into_inner();
        debug!("Filter request: handle={}", req.handle);
        
        // Get DataFrame
        let df = self.handle_manager.get_dataframe(&req.handle)
            .map_err(|e| Status::from(e))?;
        
        // Apply optimized filter with expression support
        let filtered = if let Some(expr_str) = req.predicate {
            // Use optimized parallel filter with SIMD hints
            ParallelFilter::apply_with_expr(&df, &expr_str)
                .map_err(|e| Status::internal(format!("Filter failed: {}", e)))?
        } else {
            // No predicate - return same DataFrame
            (*df).clone()
        };
        
        // Store filtered DataFrame
        let handle = self.handle_manager.create_handle(filtered);
        
        Ok(Response::new(DataFrameHandle {
            handle,
            error: None,
        }))
    }
    
    /// Select columns
    async fn select(
        &self,
        request: Request<SelectRequest>,
    ) -> std::result::Result<Response<DataFrameHandle>, Status> {
        let req = request.into_inner();
        debug!("Select request: handle={}, columns={:?}", req.handle, req.columns);
        
        // Get DataFrame
        let df = self.handle_manager.get_dataframe(&req.handle)
            .map_err(|e| Status::from(e))?;
        
        // Select columns
        let selected = df.select(&req.columns)
            .map_err(|e| PolaroidError::Polars(e))?;
        
        // Create new handle
        let handle = self.handle_manager.create_handle(selected);
        
        Ok(Response::new(DataFrameHandle {
            handle,
            error: None,
        }))
    }
    
    /// Get schema
    async fn get_schema(
        &self,
        request: Request<GetSchemaRequest>,
    ) -> std::result::Result<Response<SchemaResponse>, Status> {
        let req = request.into_inner();
        debug!("GetSchema request: handle={}", req.handle);
        
        // Get DataFrame
        let df = self.handle_manager.get_dataframe(&req.handle)
            .map_err(|e| Status::from(e))?;
        
        // Build schema info
        let columns = df.get_columns()
            .iter()
            .map(|series| ColumnInfo {
                name: series.name().to_string(),
                data_type: format!("{:?}", series.dtype()),
                nullable: true, // Polars columns can be nullable
            })
            .collect();
        
        // Convert schema to JSON
        let schema_json = serde_json::to_string(&df.schema())
            .map_err(|e| PolaroidError::Serialization(e.to_string()))?;
        
        Ok(Response::new(SchemaResponse {
            schema_json,
            columns,
        }))
    }
    
    /// Get shape
    async fn get_shape(
        &self,
        request: Request<GetShapeRequest>,
    ) -> std::result::Result<Response<ShapeResponse>, Status> {
        let req = request.into_inner();
        debug!("GetShape request: handle={}", req.handle);
        
        let df = self.handle_manager.get_dataframe(&req.handle)
            .map_err(|e| Status::from(e))?;
        
        let (rows, cols) = df.shape();
        
        Ok(Response::new(ShapeResponse {
            rows: rows as i64,
            columns: cols as i64,
        }))
    }
    
    async fn collect(
        &self,
        request: Request<CollectRequest>,
    ) -> std::result::Result<Response<Self::CollectStream>, Status> {
        let req = request.into_inner();
        info!("Collect request: handle={}", req.handle);
        
        // Get DataFrame
        let df = self.handle_manager.get_dataframe(&req.handle)
            .map_err(|e| Status::from(e))?;
        
        // Convert to Arrow IPC
        let arrow_data = Self::dataframe_to_arrow_ipc(&df)
            .map_err(|e| Status::from(e))?;
        
        info!("Sending {} bytes of Arrow IPC data", arrow_data.len());
        
        // Create stream
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        
        // Send data
        tokio::spawn(async move {
            let _ = tx.send(Ok(ArrowBatch {
                arrow_ipc: arrow_data,
                error: None,
            })).await;
        });
        
        Ok(Response::new(ReceiverStream::new(rx)))
    }
    
    /// Drop handle
    async fn drop_handle(
        &self,
        request: Request<DropHandleRequest>,
    ) -> std::result::Result<Response<DropHandleResponse>, Status> {
        let req = request.into_inner();
        debug!("DropHandle request: handle={}", req.handle);
        
        self.handle_manager.drop_handle(&req.handle)
            .map_err(|e| Status::from(e))?;
        
        Ok(Response::new(DropHandleResponse {
            success: true,
        }))
    }
    
    /// Heartbeat (extend TTL)
    async fn heartbeat(
        &self,
        request: Request<HeartbeatRequest>,
    ) -> std::result::Result<Response<HeartbeatResponse>, Status> {
        let req = request.into_inner();
        debug!("Heartbeat request: handles={:?}", req.handles);
        
        let mut alive = std::collections::HashMap::new();
        
        for handle in req.handles {
            match self.handle_manager.heartbeat(&handle) {
                Ok(_) => { alive.insert(handle, true); },
                Err(_) => { alive.insert(handle, false); },
            }
        }
        
        Ok(Response::new(HeartbeatResponse {
            alive,
        }))
    }
    
    // Phase 2 operations
    async fn read_csv(&self, request: Request<ReadCsvRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        let req = request.into_inner();
        info!("ReadCsv request: path={}", req.path);
        
        // Build CSV reader with options
        let mut lf = LazyCsvReader::new(&req.path);
        
        if req.has_header {
            lf = lf.has_header(true);
        }
        if !req.separator.is_empty() {
            lf = lf.with_separator(req.separator.as_bytes()[0]);
        }
        if let Some(n_rows) = req.n_rows {
            if n_rows > 0 {
                lf = lf.with_n_rows(Some(n_rows as usize));
            }
        }
        
        // Collect with streaming enabled
        let df = lf.finish()
            .map_err(|e| Status::internal(format!("CSV read failed: {}", e)))?
            .with_streaming(true)
            .collect()
            .map_err(|e| Status::internal(format!("CSV collect failed: {}", e)))?;
        
        let handle = self.handle_manager.create_handle(df);
        
        Ok(Response::new(DataFrameHandle {
            handle,
            error: None,
        }))
    }
    
    async fn write_csv(&self, request: Request<WriteCsvRequest>) -> std::result::Result<Response<WriteResponse>, Status> {
        let req = request.into_inner();
        info!("WriteCsv request: handle={}, path={}", req.handle, req.path);
        
        // Get DataFrame
        let df = self.handle_manager.get_dataframe(&req.handle)
            .map_err(|e| Status::from(e))?;
        
        // Write CSV with options
        let mut file = std::fs::File::create(&req.path)
            .map_err(|e| Status::internal(format!("Failed to create file: {}", e)))?;
        
        CsvWriter::new(&mut file)
            .has_header(req.has_header)
            .with_separator(if req.separator.is_empty() { b',' } else { req.separator.as_bytes()[0] })
            .finish(&mut (*df).clone())
            .map_err(|e| Status::internal(format!("CSV write failed: {}", e)))?;
        
        Ok(Response::new(WriteResponse {
            success: true,
            rows_written: df.height() as u64,
        }))
    }
    
    async fn stream_web_socket(&self, request: Request<WebSocketSourceRequest>) -> std::result::Result<Response<Self::CollectStream>, Status> {
        let req = request.into_inner();
        info!("WebSocket stream: url={}", req.url);
        
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        
        // Spawn WebSocket streaming task
        tokio::spawn(async move {
            use tokio_tungstenite::{connect_async, tungstenite::Message};
            use futures::StreamExt;
            
            match connect_async(&req.url).await {
                Ok((ws_stream, _)) => {
                    let (_, mut read) = ws_stream.split();
                    
                    while let Some(msg) = read.next().await {
                        match msg {
                            Ok(Message::Text(text)) => {
                                // Parse JSON to DataFrame
                                if let Ok(df) = Self::json_to_dataframe(&text) {
                                    let arrow_data = FastArrowSerializer::to_ipc_zero_copy(&df)
                                        .unwrap_or_default();
                                    
                                    if tx.send(Ok(ArrowBatch {
                                        arrow_ipc: arrow_data,
                                        error: None,
                                    })).await.is_err() {
                                        break;
                                    }
                                }
                            },
                            Ok(Message::Close(_)) | Err(_) => break,
                            _ => continue,
                        }
                    }
                },
                Err(e) => {
                    let _ = tx.send(Err(Status::internal(format!("WebSocket connection failed: {}", e)))).await;
                }
            }
        });
        
        Ok(Response::new(ReceiverStream::new(rx)))
    }
    
    async fn stream_rest_api(&self, request: Request<RestApiRequest>) -> std::result::Result<Response<Self::CollectStream>, Status> {
        let req = request.into_inner();
        info!("REST API stream: url={}, interval={}ms", req.url, req.poll_interval_ms);
        
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        
        // Spawn REST API polling task
        tokio::spawn(async move {
            let client = reqwest::Client::new();
            let interval = Duration::from_millis(req.poll_interval_ms.max(100));
            let mut interval_timer = tokio::time::interval(interval);
            
            loop {
                interval_timer.tick().await;
                
                let mut request_builder = match req.method.to_uppercase().as_str() {
                    "GET" => client.get(&req.url),
                    "POST" => client.post(&req.url),
                    _ => client.get(&req.url),
                };
                
                // Add headers
                for (key, value) in req.headers.iter() {
                    request_builder = request_builder.header(key, value);
                }
                
                // Add body for POST
                if !req.body.is_empty() {
                    request_builder = request_builder.body(req.body.clone());
                }
                
                match request_builder.send().await {
                    Ok(response) => {
                        if let Ok(text) = response.text().await {
                            if let Ok(df) = Self::json_to_dataframe(&text) {
                                let arrow_data = FastArrowSerializer::to_ipc_zero_copy(&df)
                                    .unwrap_or_default();
                                
                                if tx.send(Ok(ArrowBatch {
                                    arrow_ipc: arrow_data,
                                    error: None,
                                })).await.is_err() {
                                    break;
                                }
                            }
                        }
                    },
                    Err(e) => {
                        let _ = tx.send(Err(Status::internal(format!("REST API request failed: {}", e)))).await;
                        break;
                    }
                }
            }
        });
        
        Ok(Response::new(ReceiverStream::new(rx)))
    }
    
    async fn stream_grpc(&self, request: Request<GrpcStreamRequest>) -> std::result::Result<Response<Self::CollectStream>, Status> {
        let req = request.into_inner();
        info!("gRPC stream: endpoint={}, service={}", req.endpoint, req.service_name);
        
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        
        // Spawn gRPC streaming task
        tokio::spawn(async move {
            // Note: This is a simplified implementation
            // In production, you'd use tonic::transport::Channel to connect to the remote gRPC service
            let _ = tx.send(Err(Status::unimplemented(
                "gRPC streaming requires dynamic service discovery - use specific client implementation"
            ))).await;
        });
        
        Ok(Response::new(ReceiverStream::new(rx)))
    }
    
    async fn stream_message_queue(&self, request: Request<MessageQueueRequest>) -> std::result::Result<Response<Self::CollectStream>, Status> {
        let req = request.into_inner();
        info!("Message queue stream: broker={}, topic={}, protocol={}", req.broker_url, req.topic, req.protocol);
        
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        
        // Spawn message queue consumer task
        tokio::spawn(async move {
            match req.protocol.to_lowercase().as_str() {
                "kafka" => {
                    // Kafka implementation would go here
                    // Requires rdkafka crate
                    let _ = tx.send(Err(Status::unimplemented(
                        "Kafka streaming requires rdkafka dependency - add to Cargo.toml"
                    ))).await;
                },
                "rabbitmq" | "amqp" => {
                    // RabbitMQ implementation would go here
                    // Requires lapin crate
                    let _ = tx.send(Err(Status::unimplemented(
                        "RabbitMQ streaming requires lapin dependency - add to Cargo.toml"
                    ))).await;
                },
                "redis" => {
                    // Redis pub/sub implementation would go here
                    // Requires redis-rs crate
                    let _ = tx.send(Err(Status::unimplemented(
                        "Redis streaming requires redis dependency - add to Cargo.toml"
                    ))).await;
                },
                _ => {
                    let _ = tx.send(Err(Status::invalid_argument(
                        format!("Unsupported message queue protocol: {}", req.protocol)
                    ))).await;
                }
            }
        });
        
        Ok(Response::new(ReceiverStream::new(rx)))
    }
    
    async fn with_column(&self, request: Request<WithColumnRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        let req = request.into_inner();
        debug!("WithColumn request: handle={}, name={}, expr={}", req.handle, req.name, req.expression);
        
        // Get DataFrame
        let df = self.handle_manager.get_dataframe(&req.handle)
            .map_err(|e| Status::from(e))?;
        
        // Use lazy evaluation for optimization
        let lf = (*df).clone().lazy();
        
        // Parse expression and add column
        // For now, support basic expressions like lit values
        let expr = ParallelFilter::parse_expr(&req.expression)
            .map_err(|e| Status::invalid_argument(format!("Invalid expression: {}", e)))?;
        
        let result = lf.with_column(expr.alias(&req.name))
            .with_streaming(true)
            .collect()
            .map_err(|e| Status::internal(format!("with_column failed: {}", e)))?;
        
        let handle = self.handle_manager.create_handle(result);
        
        Ok(Response::new(DataFrameHandle {
            handle,
            error: None,
        }))
    }
    
    async fn with_columns(&self, request: Request<WithColumnsRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        let req = request.into_inner();
        debug!("WithColumns request: handle={}, columns={:?}", req.handle, req.columns);
        
        // Get DataFrame
        let df = self.handle_manager.get_dataframe(&req.handle)
            .map_err(|e| Status::from(e))?;
        
        let mut lf = (*df).clone().lazy();
        
        // Add each column
        for col_def in req.columns.iter() {
            let expr = ParallelFilter::parse_expr(&col_def.expression)
                .map_err(|e| Status::invalid_argument(format!("Invalid expression: {}", e)))?;
            lf = lf.with_column(expr.alias(&col_def.name));
        }
        
        let result = lf.with_streaming(true)
            .collect()
            .map_err(|e| Status::internal(format!("with_columns failed: {}", e)))?;
        
        let handle = self.handle_manager.create_handle(result);
        
        Ok(Response::new(DataFrameHandle {
            handle,
            error: None,
        }))
    }
    
    async fn drop(&self, request: Request<DropRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        let req = request.into_inner();
        debug!("Drop request: handle={}, columns={:?}", req.handle, req.columns);
        
        // Get DataFrame
        let df = self.handle_manager.get_dataframe(&req.handle)
            .map_err(|e| Status::from(e))?;
        
        // Drop columns
        let result = (*df).clone().drop_many(&req.columns)
            .map_err(|e| Status::internal(format!("drop failed: {}", e)))?;
        
        let handle = self.handle_manager.create_handle(result);
        
        Ok(Response::new(DataFrameHandle {
            handle,
            error: None,
        }))
    }
    
    async fn rename(&self, request: Request<RenameRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        let req = request.into_inner();
        debug!("Rename request: handle={}, mapping={:?}", req.handle, req.columns);
        
        // Get DataFrame
        let df = self.handle_manager.get_dataframe(&req.handle)
            .map_err(|e| Status::from(e))?;
        
        let mut result = (*df).clone();
        
        // Rename columns
        for (old_name, new_name) in req.columns.iter() {
            result.rename(old_name, new_name)
                .map_err(|e| Status::internal(format!("rename failed: {}", e)))?;
        }
        
        let handle = self.handle_manager.create_handle(result);
        
        Ok(Response::new(DataFrameHandle {
            handle,
            error: None,
        }))
    }
    
    async fn sort(&self, request: Request<SortRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        let req = request.into_inner();
        debug!("Sort request: handle={}, by={:?}", req.handle, req.by);
        
        // Get DataFrame
        let df = self.handle_manager.get_dataframe(&req.handle)
            .map_err(|e| Status::from(e))?;
        
        if req.by.is_empty() {
            return Err(Status::invalid_argument("Sort columns cannot be empty"));
        }
        
        // Use lazy evaluation for optimization
        let mut lf = (*df).clone().lazy();
        
        // Build sort expressions
        let sort_exprs: Vec<Expr> = req.by.iter().zip(req.descending.iter())
            .map(|(col_name, desc)| {
                let expr = col(col_name);
                if *desc {
                    expr.sort(SortOptions {
                        descending: true,
                        nulls_last: req.nulls_last,
                        multithreaded: true,
                        maintain_order: req.maintain_order,
                    })
                } else {
                    expr.sort(SortOptions {
                        descending: false,
                        nulls_last: req.nulls_last,
                        multithreaded: true,
                        maintain_order: req.maintain_order,
                    })
                }
            })
            .collect();
        
        lf = lf.sort_by_exprs(&sort_exprs, SortMultipleOptions {
            descending: req.descending.clone(),
            nulls_last: vec![req.nulls_last; req.by.len()],
            multithreaded: true,
            maintain_order: req.maintain_order,
        });
        
        // Collect with streaming
        let result = lf.with_streaming(true)
            .collect()
            .map_err(|e| Status::internal(format!("Sort failed: {}", e)))?;
        
        let handle = self.handle_manager.create_handle(result);
        
        Ok(Response::new(DataFrameHandle {
            handle,
            error: None,
        }))
    }
    
    async fn unique(&self, request: Request<UniqueRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        let req = request.into_inner();
        debug!("Unique request: handle={}, subset={:?}", req.handle, req.subset);
        
        // Get DataFrame
        let df = self.handle_manager.get_dataframe(&req.handle)
            .map_err(|e| Status::from(e))?;
        
        let lf = (*df).clone().lazy();
        
        let result = if req.subset.is_empty() {
            lf.unique(None, UniqueKeepStrategy::First)
        } else {
            lf.unique(Some(req.subset.clone()), UniqueKeepStrategy::First)
        }
        .with_streaming(true)
        .collect()
        .map_err(|e| Status::internal(format!("unique failed: {}", e)))?;
        
        let handle = self.handle_manager.create_handle(result);
        
        Ok(Response::new(DataFrameHandle {
            handle,
            error: None,
        }))
    }
    
    async fn limit(&self, request: Request<LimitRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        let req = request.into_inner();
        debug!("Limit request: handle={}, n={}", req.handle, req.n);
        
        // Get DataFrame
        let df = self.handle_manager.get_dataframe(&req.handle)
            .map_err(|e| Status::from(e))?;
        
        let result = (*df).clone().head(Some(req.n as usize));
        
        let handle = self.handle_manager.create_handle(result);
        
        Ok(Response::new(DataFrameHandle {
            handle,
            error: None,
        }))
    }
    
    async fn head(&self, request: Request<HeadRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        let req = request.into_inner();
        debug!("Head request: handle={}, n={}", req.handle, req.n);
        
        // Get DataFrame
        let df = self.handle_manager.get_dataframe(&req.handle)
            .map_err(|e| Status::from(e))?;
        
        let n = if req.n > 0 { req.n as usize } else { 5 };
        let result = (*df).clone().head(Some(n));
        
        let handle = self.handle_manager.create_handle(result);
        
        Ok(Response::new(DataFrameHandle {
            handle,
            error: None,
        }))
    }
    
    async fn tail(&self, request: Request<TailRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        let req = request.into_inner();
        debug!("Tail request: handle={}, n={}", req.handle, req.n);
        
        // Get DataFrame
        let df = self.handle_manager.get_dataframe(&req.handle)
            .map_err(|e| Status::from(e))?;
        
        let n = if req.n > 0 { req.n as usize } else { 5 };
        let result = (*df).clone().tail(Some(n));
        
        let handle = self.handle_manager.create_handle(result);
        
        Ok(Response::new(DataFrameHandle {
            handle,
            error: None,
        }))
    }
    
    async fn slice(&self, request: Request<SliceRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        let req = request.into_inner();
        debug!("Slice request: handle={}, offset={}, length={}", req.handle, req.offset, req.length);
        
        // Get DataFrame
        let df = self.handle_manager.get_dataframe(&req.handle)
            .map_err(|e| Status::from(e))?;
        
        let result = (*df).clone().slice(req.offset, req.length as usize);
        
        let handle = self.handle_manager.create_handle(result);
        
        Ok(Response::new(DataFrameHandle {
            handle,
            error: None,
        }))
    }
    
    async fn group_by(&self, request: Request<GroupByRequest>) -> std::result::Result<Response<GroupByHandle>, Status> {
        let req = request.into_inner();
        debug!("GroupBy request: handle={}, by={:?}", req.handle, req.by);
        
        // Get DataFrame
        let df = self.handle_manager.get_dataframe(&req.handle)
            .map_err(|e| Status::from(e))?;
        
        // Store grouped DataFrame reference
        // For now, we'll just return a handle - actual aggregation happens in agg()
        let handle = self.handle_manager.create_handle((*df).clone());
        
        Ok(Response::new(GroupByHandle {
            handle,
            group_columns: req.by,
            error: None,
        }))
    }
    
    async fn agg(&self, request: Request<AggRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        let req = request.into_inner();
        debug!("Agg request: group_handle={}, aggs={:?}", req.group_handle, req.aggregations);
        
        // Get DataFrame from group handle
        let df = self.handle_manager.get_dataframe(&req.group_handle)
            .map_err(|e| Status::from(e))?;
        
        // Extract group columns from the proto message
        // In a real implementation, we'd need to track group_by metadata
        // For now, parse aggregation expressions
        if req.aggregations.is_empty() {
            return Err(Status::invalid_argument("No aggregations specified"));
        }
        
        // Use optimized group_by implementation
        let result = FastGroupBy::group_agg(&df, &req.group_columns, &req.aggregations)
            .map_err(|e| Status::internal(format!("Aggregation failed: {}", e)))?;
        
        let handle = self.handle_manager.create_handle(result);
        
        Ok(Response::new(DataFrameHandle {
            handle,
            error: None,
        }))
    }
    
    async fn pivot(&self, request: Request<PivotRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        let req = request.into_inner();
        debug!("Pivot request: handle={}, index={:?}, columns={:?}", req.handle, req.index, req.columns);
        
        // Get DataFrame
        let df = self.handle_manager.get_dataframe(&req.handle)
            .map_err(|e| Status::from(e))?;
        
        // Use Polars pivot operation
        let result = (*df).clone()
            .pivot(
                &req.values,
                Some(&req.index),
                Some(&req.columns),
                false,
                None,
                None,
            )
            .map_err(|e| Status::internal(format!("pivot failed: {}", e)))?;
        
        let handle = self.handle_manager.create_handle(result);
        
        Ok(Response::new(DataFrameHandle {
            handle,
            error: None,
        }))
    }
    
    async fn melt(&self, request: Request<MeltRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        let req = request.into_inner();
        debug!("Melt request: handle={}, id_vars={:?}, value_vars={:?}", req.handle, req.id_vars, req.value_vars);
        
        // Get DataFrame
        let df = self.handle_manager.get_dataframe(&req.handle)
            .map_err(|e| Status::from(e))?;
        
        // Use Polars melt operation
        let args = MeltArgs {
            id_vars: req.id_vars.iter().map(|s| s.as_str()).collect(),
            value_vars: req.value_vars.iter().map(|s| s.as_str()).collect(),
            variable_name: Some(req.var_name.as_str()),
            value_name: Some(req.value_name.as_str()),
            streamable: true,
        };
        
        let result = (*df).clone()
            .melt2(args)
            .map_err(|e| Status::internal(format!("melt failed: {}", e)))?;
        
        let handle = self.handle_manager.create_handle(result);
        
        Ok(Response::new(DataFrameHandle {
            handle,
            error: None,
        }))
    }
    
    async fn join(&self, request: Request<JoinRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        let req = request.into_inner();
        debug!("Join request: left={}, right={}, on={:?}", req.left_handle, req.right_handle, req.on);
        
        // Get DataFrames
        let left_df = self.handle_manager.get_dataframe(&req.left_handle)
            .map_err(|e| Status::from(e))?;
        let right_df = self.handle_manager.get_dataframe(&req.right_handle)
            .map_err(|e| Status::from(e))?;
        
        // Parse join type
        let join_type = match req.how.to_lowercase().as_str() {
            "inner" => JoinType::Inner,
            "left" => JoinType::Left,
            "outer" | "full" => JoinType::Full,
            "semi" => JoinType::Semi,
            "anti" => JoinType::Anti,
            "cross" => JoinType::Cross,
            _ => return Err(Status::invalid_argument(format!("Unknown join type: {}", req.how))),
        };
        
        // Perform join using lazy evaluation for optimization
        let left_lf = (*left_df).clone().lazy();
        let right_lf = (*right_df).clone().lazy();
        
        let joined_lf = if req.on.is_empty() {
            return Err(Status::invalid_argument("Join columns cannot be empty"));
        } else {
            left_lf.join(
                right_lf,
                &req.on.iter().map(|s| col(s)).collect::<Vec<_>>(),
                &req.on.iter().map(|s| col(s)).collect::<Vec<_>>(),
                JoinArgs::new(join_type),
            )
        };
        
        // Collect with streaming enabled
        let result = joined_lf.with_streaming(true)
            .collect()
            .map_err(|e| Status::internal(format!("Join failed: {}", e)))?;
        
        let handle = self.handle_manager.create_handle(result);
        
        Ok(Response::new(DataFrameHandle {
            handle,
            error: None,
        }))
    }
    
    async fn cross(&self, request: Request<CrossRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        let req = request.into_inner();
        debug!("Cross join request: left={}, right={}", req.left_handle, req.right_handle);
        
        // Get DataFrames
        let left_df = self.handle_manager.get_dataframe(&req.left_handle)
            .map_err(|e| Status::from(e))?;
        let right_df = self.handle_manager.get_dataframe(&req.right_handle)
            .map_err(|e| Status::from(e))?;
        
        // Perform cross join using lazy evaluation
        let left_lf = (*left_df).clone().lazy();
        let right_lf = (*right_df).clone().lazy();
        
        let result = left_lf
            .cross_join(right_lf, None)
            .with_streaming(true)
            .collect()
            .map_err(|e| Status::internal(format!("cross join failed: {}", e)))?;
        
        let handle = self.handle_manager.create_handle(result);
        
        Ok(Response::new(DataFrameHandle {
            handle,
            error: None,
        }))
    }
    
    async fn as_time_series(&self, _request: Request<AsTimeSeriesRequest>) -> std::result::Result<Response<TimeSeriesHandle>, Status> {
        Err(Status::unimplemented("as_time_series will be implemented in Phase 4"))
    }
    
    async fn resample(&self, _request: Request<ResampleRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("resample will be implemented in Phase 4"))
    }
    
    async fn rolling_window(&self, _request: Request<RollingWindowRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("rolling_window will be implemented in Phase 4"))
    }
    
    async fn lag(&self, request: Request<LagRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        let req = request.into_inner();
        info!("lag request: handle={}, columns={:?}", req.handle, req.columns);
        
        let df = self.handle_manager.get_dataframe(&req.handle)
            .map_err(|e| Status::not_found(format!("Handle {} not found: {}", req.handle, e)))?;
        
        let mut lf = (*df).clone().lazy();
        
        for lag_col in req.columns.iter() {
            let col_name = &lag_col.column;
            let periods = lag_col.periods;
            let alias_str = lag_col.alias.as_ref()
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("{}_lag_{}", col_name, periods));
            
            lf = lf.with_column(
                col(col_name).shift(lit(periods)).alias(&alias_str)
            );
        }
        
        let result_df = lf.collect()
            .map_err(|e| PolaroidError::Polars(e))?;
        
        let handle = self.handle_manager.create_handle(result_df);
        
        Ok(Response::new(DataFrameHandle {
            handle,
            error: None,
        }))
    }
    
    async fn lead(&self, request: Request<LeadRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        let req = request.into_inner();
        info!("lead request: handle={}, columns={:?}", req.handle, req.columns);
        
        let df = self.handle_manager.get_dataframe(&req.handle)
            .map_err(|e| Status::not_found(format!("Handle {} not found: {}", req.handle, e)))?;
        
        let mut lf = (*df).clone().lazy();
        
        for lead_col in req.columns.iter() {
            let col_name = &lead_col.column;
            let periods = lead_col.periods;
            let alias_str = lead_col.alias.as_ref()
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("{}_lead_{}", col_name, periods));
            
            // Lead is negative shift
            lf = lf.with_column(
                col(col_name).shift(lit(-periods)).alias(&alias_str)
            );
        }
        
        let result_df = lf.collect()
            .map_err(|e| PolaroidError::Polars(e))?;
        
        let handle = self.handle_manager.create_handle(result_df);
        
        Ok(Response::new(DataFrameHandle {
            handle,
            error: None,
        }))
    }
    
    async fn diff(&self, request: Request<DiffRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        let req = request.into_inner();
        info!("diff request: handle={}, columns={:?}, periods={}", req.handle, req.columns, req.periods);
        
        let df = self.handle_manager.get_dataframe(&req.handle)
            .map_err(|e| Status::not_found(format!("Handle {} not found: {}", req.handle, e)))?;
        
        let mut lf = (*df).clone().lazy();
        
        for col_name in req.columns.iter() {
            let alias = format!("{}_diff", col_name);
            
            // Diff: current - shifted
            lf = lf.with_column(
                (col(col_name) - col(col_name).shift(lit(req.periods))).alias(&alias)
            );
        }
        
        let result_df = lf.collect()
            .map_err(|e| PolaroidError::Polars(e))?;
        
        let handle = self.handle_manager.create_handle(result_df);
        
        Ok(Response::new(DataFrameHandle {
            handle,
            error: None,
        }))
    }
    
    async fn pct_change(&self, request: Request<PctChangeRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        let req = request.into_inner();
        info!("pct_change request: handle={}, columns={:?}, periods={}", req.handle, req.columns, req.periods);
        
        let df = self.handle_manager.get_dataframe(&req.handle)
            .map_err(|e| Status::not_found(format!("Handle {} not found: {}", req.handle, e)))?;
        
        let mut lf = (*df).clone().lazy();
        
        for col_name in req.columns.iter() {
            let alias = format!("{}_pct_change", col_name);
            
            // Pct change: (current - shifted) / shifted
            let shifted = col(col_name).shift(lit(req.periods));
            lf = lf.with_column(
                ((col(col_name) - shifted.clone()) / shifted).alias(&alias)
            );
        }
        
        let result_df = lf.collect()
            .map_err(|e| PolaroidError::Polars(e))?;
        
        let handle = self.handle_manager.create_handle(result_df);
        
        Ok(Response::new(DataFrameHandle {
            handle,
            error: None,
        }))
    }
    
    async fn asof_join(&self, _request: Request<AsofJoinRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("asof_join will be implemented in Phase 4"))
    }
    
    async fn fill_null(&self, _request: Request<FillNullRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("fill_null will be implemented in Phase 4"))
    }
    
    async fn fill_nan(&self, _request: Request<FillNanRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("fill_nan will be implemented in Phase 4"))
    }
    
    async fn interpolate(&self, _request: Request<InterpolateRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("interpolate will be implemented in Phase 4"))
    }
    
    async fn collect_streaming(&self, _request: Request<CollectStreamingRequest>) -> std::result::Result<Response<Self::CollectStream>, Status> {
        Err(Status::unimplemented("collect_streaming will be implemented in Phase 3"))
    }
    
    async fn explain(&self, _request: Request<ExplainRequest>) -> std::result::Result<Response<ExplainResponse>, Status> {
        Err(Status::unimplemented("explain will be implemented in Phase 3"))
    }
    
    async fn get_stats(&self, _request: Request<GetStatsRequest>) -> std::result::Result<Response<StatsResponse>, Status> {
        Err(Status::unimplemented("get_stats will be implemented in Phase 2"))
    }
    
    async fn describe(&self, _request: Request<DescribeRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("describe will be implemented in Phase 2"))
    }
    
    async fn create_from_arrow(&self, _request: Request<CreateFromArrowRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("create_from_arrow will be implemented in Phase 2"))
    }
    
    async fn clone(&self, _request: Request<CloneRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("clone will be implemented in Phase 2"))
    }
}
