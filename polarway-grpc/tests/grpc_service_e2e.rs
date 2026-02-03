use std::net::SocketAddr;
use std::time::Duration;

use polaroid_grpc::proto::data_frame_service_client::DataFrameServiceClient;
use polaroid_grpc::proto::data_frame_service_server::DataFrameServiceServer;
use polaroid_grpc::proto::*;
use polaroid_grpc::PolaroidDataFrameService;
use polars::prelude::*;
use polars_utils::plpath::PlPath;
use tokio::sync::oneshot;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::Server;

async fn spawn_grpc_server() -> (String, oneshot::Sender<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let local_addr: SocketAddr = listener.local_addr().expect("local addr");

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

    (format!("http://{local_addr}"), shutdown_tx)
}

fn unique_tmp_path(ext: &str) -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    let name = format!("polaroid_grpc_test_{}_{}.{}", std::process::id(), uuid::Uuid::new_v4(), ext);
    p.push(name);
    p
}

async fn connect_client(endpoint: &str) -> DataFrameServiceClient<tonic::transport::Channel> {
    DataFrameServiceClient::connect(endpoint.to_string())
        .await
        .expect("connect grpc client")
}

#[tokio::test]
async fn grpc_read_parquet_write_parquet_roundtrip() {
    let (endpoint, shutdown_tx) = spawn_grpc_server().await;
    let mut client = connect_client(&endpoint).await;

    let input_path = unique_tmp_path("parquet");
    let output_path = unique_tmp_path("parquet");

    // Create a tiny parquet file.
    let df = DataFrame::new(vec![
        Series::new("a".into(), [1i64, 2, 3]).into(),
        Series::new("b".into(), ["x", "y", "z"]).into(),
    ])
    .expect("df");

    {
        let mut f = std::fs::File::create(&input_path).expect("create parquet");
        ParquetWriter::new(&mut f)
            .finish(&mut df.clone())
            .expect("write parquet");
    }

    let handle = client
        .read_parquet(ReadParquetRequest {
            path: input_path.to_string_lossy().to_string(),
            columns: vec!["a".to_string(), "b".to_string()],
            predicate: None,
            n_rows: Some(2),
            row_index_offset: None,
            parallel: false,
        })
        .await
        .expect("read_parquet")
        .into_inner()
        .handle;

    assert!(!handle.is_empty());

    let schema = client
        .get_schema(GetSchemaRequest { handle: handle.clone() })
        .await
        .expect("get_schema")
        .into_inner();

    assert!(schema.columns.iter().any(|c| c.name == "a"));

    let write = client
        .write_parquet(WriteParquetRequest {
            handle: handle.clone(),
            path: output_path.to_string_lossy().to_string(),
            compression: None,
            row_group_size: None,
            append: false,
        })
        .await
        .expect("write_parquet")
        .into_inner();

    assert!(write.success);
    assert_eq!(write.rows_written, Some(2));

    let out_height = tokio::task::spawn_blocking({
        let output_path = output_path.clone();
        move || {
            LazyFrame::scan_parquet(PlPath::new(output_path.to_string_lossy().as_ref()), Default::default())
                .expect("scan parquet")
                .collect()
                .expect("collect parquet")
                .height()
        }
    })
    .await
    .expect("join");

    assert_eq!(out_height, 2);

    let _ = std::fs::remove_file(&input_path);
    let _ = std::fs::remove_file(&output_path);
    let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn grpc_collect_returns_decodable_arrow_ipc() {
    let (endpoint, shutdown_tx) = spawn_grpc_server().await;
    let mut client = connect_client(&endpoint).await;

    let input_path = unique_tmp_path("parquet");

    let df = DataFrame::new(vec![
        Series::new("x".into(), [10i64, 20, 30]).into(),
        Series::new("y".into(), [1.0f64, 2.0, 3.0]).into(),
    ])
    .expect("df");

    {
        let mut f = std::fs::File::create(&input_path).expect("create parquet");
        ParquetWriter::new(&mut f)
            .finish(&mut df.clone())
            .expect("write parquet");
    }

    let handle = client
        .read_parquet(ReadParquetRequest {
            path: input_path.to_string_lossy().to_string(),
            columns: vec![],
            predicate: None,
            n_rows: None,
            row_index_offset: None,
            parallel: false,
        })
        .await
        .expect("read_parquet")
        .into_inner()
        .handle;

    let mut stream = client
        .collect(CollectRequest { handle, limit: None })
        .await
        .expect("collect")
        .into_inner();

    let first = tokio::time::timeout(Duration::from_secs(5), stream.message())
        .await
        .expect("timeout")
        .expect("stream message")
        .expect("batch");

    assert!(!first.arrow_ipc.is_empty());

    // Validate bytes can be decoded via Polars IPC reader.
    let decoded = polars::io::ipc::IpcReader::new(std::io::Cursor::new(first.arrow_ipc))
        .finish()
        .expect("decode ipc");

    assert_eq!(decoded.height(), 3);

    let _ = std::fs::remove_file(&input_path);
    let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn grpc_stream_rest_api_streams_batches() {
    async fn handler() -> &'static str {
        "[{\"k\":1},{\"k\":2}]"
    }

    let app = axum::Router::new().route("/data", axum::routing::get(handler));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind http");
    let http_addr = listener.local_addr().expect("http addr");

    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let (endpoint, shutdown_tx) = spawn_grpc_server().await;
    let mut client = connect_client(&endpoint).await;

    let url = format!("http://{http_addr}/data");

    let mut stream = client
        .stream_rest_api(RestApiRequest {
            url,
            method: "GET".to_string(),
            headers: Default::default(),
            body: None,
            pagination: None,
            schema_json: String::new(),
            rate_limit: None,
            format: MessageFormat::Json as i32,
        })
        .await
        .expect("stream_rest_api")
        .into_inner();

    let first = tokio::time::timeout(Duration::from_secs(5), stream.message())
        .await
        .expect("timeout")
        .expect("stream message")
        .expect("batch");

    assert!(!first.arrow_ipc.is_empty());

    let decoded = polars::io::ipc::IpcReader::new(std::io::Cursor::new(first.arrow_ipc))
        .finish()
        .expect("decode ipc");

    assert_eq!(decoded.height(), 2);
    assert!(decoded.get_column_names().iter().any(|n| *n == "k"));

    let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn grpc_time_series_rpcs_are_unimplemented() {
    let (endpoint, shutdown_tx) = spawn_grpc_server().await;
    let mut client = connect_client(&endpoint).await;

    let err = client
        .as_time_series(AsTimeSeriesRequest {
            handle: "does_not_matter".to_string(),
            timestamp_column: "ts".to_string(),
            frequency: None,
            ensure_sorted: false,
        })
        .await
        .expect_err("as_time_series should be unimplemented");

    assert_eq!(err.code(), tonic::Code::Unimplemented);

    let err = client
        .resample(ResampleRequest {
            handle: "does_not_matter".to_string(),
            frequency: "1m".to_string(),
            aggregations: vec![],
            label: None,
        })
        .await
        .expect_err("resample should be unimplemented");

    assert_eq!(err.code(), tonic::Code::Unimplemented);

    let err = client
        .rolling_window(RollingWindowRequest {
            handle: "does_not_matter".to_string(),
            window_size: "10".to_string(),
            aggregations: vec![],
            min_periods: None,
            center: false,
        })
        .await
        .expect_err("rolling_window should be unimplemented");

    assert_eq!(err.code(), tonic::Code::Unimplemented);

    let _ = shutdown_tx.send(());
}
