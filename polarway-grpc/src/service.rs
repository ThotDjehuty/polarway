use tonic::{Request, Response, Status};
use tokio_stream::wrappers::ReceiverStream;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info};
use polars::prelude::*;
use polars_utils::plpath::PlPath;

use crate::proto::{
    data_frame_service_server::DataFrameService,
    *,
};
use crate::handles::HandleManager;
use crate::error::{PolaroidError, Result};

pub struct PolaroidDataFrameService {
    handle_manager: Arc<HandleManager>,
}

impl PolaroidDataFrameService {
    pub fn new() -> Self {
        let handle_manager = Arc::new(HandleManager::default());
        
        // Spawn cleanup task
        let manager_clone = Arc::clone(&handle_manager);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(300));
            loop {
                interval.tick().await;
                manager_clone.cleanup_expired();
            }
        });
        
        Self { handle_manager }
    }

    pub fn handle_manager(&self) -> Arc<HandleManager> {
        Arc::clone(&self.handle_manager)
    }
    
    /// Convert Polars DataFrame to Arrow IPC bytes
    fn dataframe_to_arrow_ipc(df: &DataFrame) -> Result<Vec<u8>> {
        let mut buffer = Vec::new();

        polars::io::ipc::IpcWriter::new(&mut buffer)
            .finish(&mut df.clone())
            .map_err(PolaroidError::Polars)?;

        Ok(buffer)
    }
    
    /// Fetch data from REST API and convert to DataFrame
    async fn fetch_rest_api_data(req: RestApiRequest) -> std::result::Result<DataFrame, Status> {
        // Build HTTP client
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| Status::internal(format!("Failed to create HTTP client: {}", e)))?;
        
        // Build request
        let mut request_builder = match req.method.to_lowercase().as_str() {
            "get" | "" => client.get(&req.url),
            "post" => client.post(&req.url),
            "put" => client.put(&req.url),
            _ => return Err(Status::invalid_argument(format!("Unsupported HTTP method: {}", req.method))),
        };
        
        // Add headers
        for (key, value) in req.headers.iter() {
            request_builder = request_builder.header(key, value);
        }
        
        // Add body for POST/PUT
        if let Some(body) = req.body {
            request_builder = request_builder.body(body);
        }
        
        // Execute request
        let response = request_builder
            .send()
            .await
            .map_err(|e| Status::internal(format!("HTTP request failed: {}", e)))?;
        
        // Check status
        if !response.status().is_success() {
            return Err(Status::internal(format!("HTTP error: {}", response.status())));
        }
        
        // Parse JSON response
        let json_text = response
            .text()
            .await
            .map_err(|e| Status::internal(format!("Failed to read response body: {}", e)))?;
        
        // Convert JSON to DataFrame using Polars (blocking)
        let json_bytes = json_text.into_bytes();
        let df = tokio::task::spawn_blocking(move || {
            polars::io::json::JsonReader::new(std::io::Cursor::new(json_bytes))
                .finish()
                .map_err(|e| Status::internal(format!("Failed to parse JSON to DataFrame: {}", e)))
        })
        .await
        .map_err(|e| Status::internal(format!("JSON parse task failed: {}", e)))??;

        Ok(df)
    }
    
    /// Convert DataFrame to Arrow IPC batches for streaming
    fn dataframe_to_arrow_batches_simple(df: &DataFrame) -> Result<Vec<ArrowBatch>> {
        // For simplicity, convert entire DataFrame to single batch
        // In production, this should chunk large DataFrames
        let mut buffer = Vec::new();
        
        // Write DataFrame as Arrow IPC
        polars::io::ipc::IpcWriter::new(&mut buffer)
            .finish(&mut df.clone())
            .map_err(|e| PolaroidError::Polars(e))?;
        
        Ok(vec![ArrowBatch {
            arrow_ipc: buffer,
            error: None,
        }])
    }
}

#[tonic::async_trait]
impl DataFrameService for PolaroidDataFrameService {
    type CollectStream = ReceiverStream<std::result::Result<ArrowBatch, Status>>;
    type CollectStreamingStream = ReceiverStream<std::result::Result<ArrowBatch, Status>>;
    type StreamWebSocketStream = ReceiverStream<std::result::Result<ArrowBatch, Status>>;
    type StreamRestApiStream = ReceiverStream<std::result::Result<ArrowBatch, Status>>;
    type StreamGrpcStream = ReceiverStream<std::result::Result<ArrowBatch, Status>>;
    type StreamMessageQueueStream = ReceiverStream<std::result::Result<ArrowBatch, Status>>;

    /// Read Parquet file (optimized with pushdown)
    async fn read_parquet(
        &self,
        request: Request<ReadParquetRequest>,
    ) -> std::result::Result<Response<DataFrameHandle>, Status> {
        let req = request.into_inner();
        info!("ReadParquet request: path={}", req.path);

        let handle_manager = self.handle_manager();
        let handle = tokio::task::spawn_blocking(move || {
            let mut args = ScanArgsParquet::default();
            args.parallel = if req.parallel {
                ParallelStrategy::Auto
            } else {
                ParallelStrategy::None
            };

            let path = PlPath::new(&req.path);
            let mut lf = LazyFrame::scan_parquet(path, args)
                .map_err(|e| Status::internal(format!("Failed to scan parquet: {}", e)))?;

            // Apply projection pushdown
            if !req.columns.is_empty() {
                lf = lf.select(&req.columns.iter().map(|s| col(s)).collect::<Vec<_>>());
            }

            // Apply row limit
            if let Some(n_rows) = req.n_rows {
                if n_rows > 0 {
                    lf = lf.limit(n_rows as IdxSize);
                }
            }

            // Collect DataFrame
            let df = lf
                .collect()
                .map_err(|e| Status::internal(format!("Failed to collect: {}", e)))?;

            Ok::<_, Status>(handle_manager.create_handle(df))
        })
        .await
        .map_err(|e| Status::internal(format!("ReadParquet task failed: {}", e)))??;
        
        Ok(Response::new(DataFrameHandle {
            handle,
            error: None,
        }))
    }
    
    /// Write Parquet
    async fn write_parquet(
        &self,
        request: Request<WriteParquetRequest>,
    ) -> std::result::Result<Response<WriteResponse>, Status> {
        let req = request.into_inner();
        info!("WriteParquet request: handle={}, path={}", req.handle, req.path);

        let handle_manager = self.handle_manager();
        let rows_written = tokio::task::spawn_blocking(move || {
            let df = handle_manager.get_dataframe(&req.handle).map_err(Status::from)?;

            let mut file = std::fs::File::create(&req.path)
                .map_err(|e| Status::internal(format!("Failed to create file: {}", e)))?;

            ParquetWriter::new(&mut file)
                .finish(&mut (*df).clone())
                .map_err(|e| Status::internal(format!("Failed to write parquet: {}", e)))?;

            Ok::<_, Status>(df.height() as i64)
        })
        .await
        .map_err(|e| Status::internal(format!("WriteParquet task failed: {}", e)))??;

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
        
        let df = self.handle_manager.get_dataframe(&req.handle)
            .map_err(|e| Status::from(e))?;
        
        // For now, return unfiltered (expression parsing would go here)
        let handle = self.handle_manager.create_handle((*df).clone());
        
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
        
        let df = self.handle_manager.get_dataframe(&req.handle)
            .map_err(|e| Status::from(e))?;
        
        let selected = (*df).clone().lazy()
            .select(&req.columns.iter().map(|s| col(s)).collect::<Vec<_>>())
            .collect()
            .map_err(|e| Status::internal(format!("Select failed: {}", e)))?;
        
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
        
        let df = self.handle_manager.get_dataframe(&req.handle)
            .map_err(|e| Status::from(e))?;
        
        let schema_json = serde_json::to_string(&df.schema())
            .map_err(|e| Status::internal(format!("Failed to serialize schema: {}", e)))?;
        
        // Build ColumnInfo vector
        let columns = df.get_column_names().iter().zip(df.dtypes().iter())
            .map(|(name, dtype)| crate::proto::ColumnInfo {
                name: name.to_string(),
                data_type: format!("{:?}", dtype),
                nullable: true, // Polars columns are generally nullable
            })
            .collect();
        
        Ok(Response::new(SchemaResponse {
            schema_json,
            columns,
        }))
    }
    
    /// Collect DataFrame
    async fn collect(
        &self,
        request: Request<CollectRequest>,
    ) -> std::result::Result<Response<Self::CollectStream>, Status> {
        let req = request.into_inner();
        info!("Collect request: handle={}", req.handle);
        
        let df = self.handle_manager.get_dataframe(&req.handle)
            .map_err(|e| Status::from(e))?;
        
        let arrow_data = Self::dataframe_to_arrow_ipc(&df)
            .map_err(|e| Status::from(e))?;
        
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        
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
        self.handle_manager.drop_handle(&req.handle)
            .map_err(|e| Status::from(e))?;
        Ok(Response::new(DropHandleResponse { success: true }))
    }
    
    /// Heartbeat
    async fn heartbeat(
        &self,
        request: Request<HeartbeatRequest>,
    ) -> std::result::Result<Response<HeartbeatResponse>, Status> {
        let req = request.into_inner();
        let mut alive = std::collections::HashMap::new();
        
        for handle in req.handles {
            match self.handle_manager.heartbeat(&handle) {
                Ok(_) => { alive.insert(handle, true); },
                Err(_) => { alive.insert(handle, false); },
            }
        }
        
        Ok(Response::new(HeartbeatResponse { alive }))
    }
    
    // === Stub implementations for remaining operations ===
    
    async fn read_csv(&self, _req: Request<ReadCsvRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("read_csv"))
    }
    
    async fn write_csv(&self, _req: Request<WriteCsvRequest>) -> std::result::Result<Response<WriteResponse>, Status> {
        Err(Status::unimplemented("write_csv"))
    }
    
    async fn stream_web_socket(&self, _req: Request<WebSocketSourceRequest>) -> std::result::Result<Response<Self::StreamWebSocketStream>, Status> {
        Err(Status::unimplemented("stream_web_socket"))
    }
    
    async fn stream_rest_api(&self, req: Request<RestApiRequest>) -> std::result::Result<Response<Self::StreamRestApiStream>, Status> {
        let req = req.into_inner();
        info!("StreamRestApi request: url={}", req.url);
        
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        
        tokio::spawn(async move {
            match Self::fetch_rest_api_data(req).await {
                Ok(df) => {
                    // Convert DataFrame to Arrow IPC batches
                    match Self::dataframe_to_arrow_batches_simple(&df) {
                        Ok(batches) => {
                            for batch in batches {
                                if tx.send(Ok(batch)).await.is_err() {
                                    break;
                                }
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(Err(Status::internal(format!("Arrow conversion failed: {}", e)))).await;
                        }
                    }
                }
                Err(e) => {
                    let _ = tx.send(Err(e)).await;
                }
            }
        });
        
        Ok(Response::new(ReceiverStream::new(rx)))
    }
    
    async fn stream_grpc(&self, _req: Request<GrpcStreamRequest>) -> std::result::Result<Response<Self::StreamGrpcStream>, Status> {
        Err(Status::unimplemented("stream_grpc"))
    }
    
    async fn stream_message_queue(&self, _req: Request<MessageQueueRequest>) -> std::result::Result<Response<Self::StreamMessageQueueStream>, Status> {
        Err(Status::unimplemented("stream_message_queue"))
    }
    
    async fn read_rest_api(
        &self,
        request: Request<RestApiRequest>,
    ) -> std::result::Result<Response<DataFrameHandle>, Status> {
        let req = request.into_inner();
        info!("ReadRestApi request: url={}", req.url);
        
        // Fetch data from REST API
        let df = Self::fetch_rest_api_data(req).await?;
        
        // Create handle for the DataFrame
        let handle = self.handle_manager.create_handle(df);
        
        Ok(Response::new(DataFrameHandle {
            handle,
            error: None,
        }))
    }
    
    async fn with_column(&self, _req: Request<WithColumnRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("with_column"))
    }
    
    async fn with_columns(&self, _req: Request<WithColumnsRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("with_columns"))
    }
    
    async fn drop(&self, _req: Request<DropRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("drop"))
    }
    
    async fn rename(&self, _req: Request<RenameRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("rename"))
    }
    
    async fn sort(&self, _req: Request<SortRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("sort"))
    }
    
    async fn unique(&self, _req: Request<UniqueRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("unique"))
    }
    
    async fn limit(&self, _req: Request<LimitRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("limit"))
    }
    
    async fn head(&self, _req: Request<HeadRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("head"))
    }
    
    async fn tail(&self, _req: Request<TailRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("tail"))
    }
    
    async fn slice(&self, _req: Request<SliceRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("slice"))
    }
    
    async fn group_by(&self, _req: Request<GroupByRequest>) -> std::result::Result<Response<GroupByHandle>, Status> {
        Err(Status::unimplemented("group_by"))
    }
    
    async fn agg(&self, _req: Request<AggRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("agg"))
    }
    
    async fn pivot(&self, _req: Request<PivotRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("pivot"))
    }
    
    async fn melt(&self, _req: Request<MeltRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("melt"))
    }
    
    async fn join(&self, _req: Request<JoinRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("join"))
    }
    
    async fn cross(&self, _req: Request<CrossRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("cross"))
    }
    
    async fn as_time_series(&self, _req: Request<AsTimeSeriesRequest>) -> std::result::Result<Response<TimeSeriesHandle>, Status> {
        Err(Status::unimplemented("as_time_series"))
    }
    
    async fn resample(&self, _req: Request<ResampleRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("resample"))
    }
    
    async fn rolling_window(&self, _req: Request<RollingWindowRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("rolling_window"))
    }
    
    async fn lag(&self, _req: Request<LagRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("lag"))
    }
    
    async fn lead(&self, _req: Request<LeadRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("lead"))
    }
    
    async fn diff(&self, _req: Request<DiffRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("diff"))
    }
    
    async fn pct_change(&self, _req: Request<PctChangeRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("pct_change"))
    }
    
    async fn asof_join(&self, _req: Request<AsofJoinRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("asof_join"))
    }
    
    async fn fill_null(&self, _req: Request<FillNullRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("fill_null"))
    }
    
    async fn fill_nan(&self, _req: Request<FillNanRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("fill_nan"))
    }
    
    async fn interpolate(&self, _req: Request<InterpolateRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("interpolate"))
    }
    
    async fn collect_streaming(&self, _req: Request<CollectStreamingRequest>) -> std::result::Result<Response<Self::CollectStreamingStream>, Status> {
        Err(Status::unimplemented("collect_streaming"))
    }
    
    async fn explain(&self, _req: Request<ExplainRequest>) -> std::result::Result<Response<ExplainResponse>, Status> {
        Err(Status::unimplemented("explain"))
    }
    
    async fn get_shape(&self, _req: Request<GetShapeRequest>) -> std::result::Result<Response<ShapeResponse>, Status> {
        Err(Status::unimplemented("get_shape"))
    }
    
    async fn get_stats(&self, _req: Request<GetStatsRequest>) -> std::result::Result<Response<StatsResponse>, Status> {
        Err(Status::unimplemented("get_stats"))
    }
    
    async fn describe(&self, _req: Request<DescribeRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("describe"))
    }
    
    async fn create_from_arrow(&self, _req: Request<CreateFromArrowRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("create_from_arrow"))
    }
    
    async fn clone(&self, _req: Request<CloneRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("clone"))
    }
}
