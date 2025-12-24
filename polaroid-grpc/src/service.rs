use tonic::{Request, Response, Status};
use tokio_stream::wrappers::ReceiverStream;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, error};
use polars::prelude::*;
use polars_io::ipc::IpcWriter;
use polars_utils::plpath::PlPath;
use std::io::Cursor;

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
            let mut interval = tokio::time::interval(Duration::from_secs(300)); // Clean every 5 min
            loop {
                interval.tick().await;
                manager_clone.cleanup_expired();
            }
        });
        
        Self { handle_manager }
    }
    
    /// Convert Polars DataFrame to Arrow IPC bytes
    fn dataframe_to_arrow_ipc(df: &DataFrame) -> Result<Vec<u8>> {
        let mut buffer = Cursor::new(Vec::new());
        let mut df_clone = df.clone();  // Clone needed for mutable reference
        
        // Use Polars' IPC writer
        IpcWriter::new(&mut buffer)
            .finish(&mut df_clone)
            .map_err(|e| PolaroidError::Polars(e))?;
        
        Ok(buffer.into_inner())
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
    
    /// Read Parquet file
    async fn read_parquet(
        &self,
        request: Request<ReadParquetRequest>,
    ) -> std::result::Result<Response<DataFrameHandle>, Status> {
        let req = request.into_inner();
        info!("ReadParquet request: path={}", req.path);
        
        // Read parquet file
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
                // For Phase 1, we'll support simple predicates
                // TODO: Implement full expression parser in Phase 2
                debug!("Predicate pushdown: {}", predicate);
            }
        }
        
        // Apply row limit if specified
        if let Some(n_rows) = req.n_rows {
            if n_rows > 0 {
                lf = lf.limit(n_rows as u32);
            }
        }
        
        // Collect to DataFrame
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
        
        // For Phase 1, we'll implement basic expression support
        // Full expression parser in Phase 2
        if let Some(_expr) = req.predicate {
            // TODO: Parse and apply expression
            return Err(Status::unimplemented("Expression filtering not yet implemented in Phase 1"));
        }
        
        // For now, return same DataFrame (will implement in Phase 2)
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
    
    // Stubs for Phase 2+ operations
    async fn read_csv(&self, _request: Request<ReadCsvRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("read_csv will be implemented in Phase 2"))
    }
    
    async fn write_csv(&self, _request: Request<WriteCsvRequest>) -> std::result::Result<Response<WriteResponse>, Status> {
        Err(Status::unimplemented("write_csv will be implemented in Phase 2"))
    }
    
    async fn stream_web_socket(&self, _request: Request<WebSocketSourceRequest>) -> std::result::Result<Response<Self::CollectStream>, Status> {
        Err(Status::unimplemented("stream_web_socket will be implemented in Phase 5"))
    }
    
    async fn stream_rest_api(&self, _request: Request<RestApiRequest>) -> std::result::Result<Response<Self::CollectStream>, Status> {
        Err(Status::unimplemented("stream_rest_api will be implemented in Phase 5"))
    }
    
    async fn stream_grpc(&self, _request: Request<GrpcStreamRequest>) -> std::result::Result<Response<Self::CollectStream>, Status> {
        Err(Status::unimplemented("stream_grpc will be implemented in Phase 5"))
    }
    
    async fn stream_message_queue(&self, _request: Request<MessageQueueRequest>) -> std::result::Result<Response<Self::CollectStream>, Status> {
        Err(Status::unimplemented("stream_message_queue will be implemented in Phase 5"))
    }
    
    async fn with_column(&self, _request: Request<WithColumnRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("with_column will be implemented in Phase 2"))
    }
    
    async fn with_columns(&self, _request: Request<WithColumnsRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("with_columns will be implemented in Phase 2"))
    }
    
    async fn drop(&self, _request: Request<DropRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("drop will be implemented in Phase 2"))
    }
    
    async fn rename(&self, _request: Request<RenameRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("rename will be implemented in Phase 2"))
    }
    
    async fn sort(&self, _request: Request<SortRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("sort will be implemented in Phase 2"))
    }
    
    async fn unique(&self, _request: Request<UniqueRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("unique will be implemented in Phase 2"))
    }
    
    async fn limit(&self, _request: Request<LimitRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("limit will be implemented in Phase 2"))
    }
    
    async fn head(&self, _request: Request<HeadRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("head will be implemented in Phase 2"))
    }
    
    async fn tail(&self, _request: Request<TailRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("tail will be implemented in Phase 2"))
    }
    
    async fn slice(&self, _request: Request<SliceRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("slice will be implemented in Phase 2"))
    }
    
    async fn group_by(&self, _request: Request<GroupByRequest>) -> std::result::Result<Response<GroupByHandle>, Status> {
        Err(Status::unimplemented("group_by will be implemented in Phase 2"))
    }
    
    async fn agg(&self, _request: Request<AggRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("agg will be implemented in Phase 2"))
    }
    
    async fn pivot(&self, _request: Request<PivotRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("pivot will be implemented in Phase 2"))
    }
    
    async fn melt(&self, _request: Request<MeltRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("melt will be implemented in Phase 2"))
    }
    
    async fn join(&self, _request: Request<JoinRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("join will be implemented in Phase 2"))
    }
    
    async fn cross(&self, _request: Request<CrossRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("cross will be implemented in Phase 2"))
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
    
    async fn lag(&self, _request: Request<LagRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("lag will be implemented in Phase 4"))
    }
    
    async fn lead(&self, _request: Request<LeadRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("lead will be implemented in Phase 4"))
    }
    
    async fn diff(&self, _request: Request<DiffRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("diff will be implemented in Phase 4"))
    }
    
    async fn pct_change(&self, _request: Request<PctChangeRequest>) -> std::result::Result<Response<DataFrameHandle>, Status> {
        Err(Status::unimplemented("pct_change will be implemented in Phase 4"))
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
