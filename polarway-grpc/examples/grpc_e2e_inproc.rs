use std::net::SocketAddr;
use std::time::Duration;

use polaroid_grpc::proto::data_frame_service_client::DataFrameServiceClient;
use polaroid_grpc::proto::data_frame_service_server::DataFrameServiceServer;
use polaroid_grpc::proto::*;
use polaroid_grpc::PolaroidDataFrameService;
use polars::prelude::*;
use tokio::sync::oneshot;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::Server;

fn unique_tmp_path(ext: &str) -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    let name = format!("polaroid_grpc_example_{}_{}.{}", std::process::id(), uuid::Uuid::new_v4(), ext);
    p.push(name);
    p
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Start an in-process gRPC server on an ephemeral port.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr: SocketAddr = listener.local_addr()?;

    let service = PolaroidDataFrameService::new();
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    tokio::spawn(async move {
        let incoming = TcpListenerStream::new(listener);
        let _ = Server::builder()
            .add_service(DataFrameServiceServer::new(service))
            .serve_with_incoming_shutdown(incoming, async {
                let _ = shutdown_rx.await;
            })
            .await;
    });

    let endpoint = format!("http://{addr}");
    println!("gRPC server listening on {endpoint}");

    // Create a small Parquet file.
    let input_path = unique_tmp_path("parquet");
    let output_path = unique_tmp_path("parquet");

    let df = DataFrame::new(vec![
        Series::new("id".into(), [1i64, 2, 3]).into(),
        Series::new("sym".into(), ["AAPL", "MSFT", "NVDA"]).into(),
    ])?;

    {
        let mut f = std::fs::File::create(&input_path)?;
        ParquetWriter::new(&mut f).finish(&mut df.clone())?;
    }

    let mut client = DataFrameServiceClient::connect(endpoint.clone()).await?;

    // Load via gRPC -> handle.
    let handle = client
        .read_parquet(ReadParquetRequest {
            path: input_path.to_string_lossy().to_string(),
            columns: vec![],
            predicate: None,
            n_rows: Some(2),
            row_index_offset: None,
            parallel: false,
        })
        .await?
        .into_inner()
        .handle;

    println!("ReadParquet handle: {handle}");

    // Collect via gRPC (Arrow IPC bytes).
    let mut stream = client
        .collect(CollectRequest {
            handle: handle.clone(),
            limit: None,
        })
        .await?
        .into_inner();
    let first = tokio::time::timeout(Duration::from_secs(5), stream.message()).await??
        .ok_or_else(|| anyhow::anyhow!("no collect batch received"))?;

    println!("Collect batch bytes: {}", first.arrow_ipc.len());

    let collected_df = polars::io::ipc::IpcReader::new(std::io::Cursor::new(first.arrow_ipc)).finish()?;
    println!("Collected DataFrame shape: {:?}", collected_df.shape());

    // Save back to Parquet via gRPC.
    let write = client
        .write_parquet(WriteParquetRequest {
            handle: handle.clone(),
            path: output_path.to_string_lossy().to_string(),
            compression: None,
            row_group_size: None,
            append: false,
        })
        .await?
        .into_inner();

    println!("WriteParquet success={} rows_written={:?}", write.success, write.rows_written);

    // Clean up server and files.
    let _ = shutdown_tx.send(());
    let _ = std::fs::remove_file(&input_path);
    let _ = std::fs::remove_file(&output_path);

    Ok(())
}
