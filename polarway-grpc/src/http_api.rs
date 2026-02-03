use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use polars::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::info;

use crate::handles::HandleManager;

#[derive(Clone)]
pub struct HttpApiState {
    pub handle_manager: Arc<HandleManager>,
}

#[derive(Debug, Deserialize)]
pub struct ExecQuery {
    /// Polaroid extension: return the DataFrame referenced by this handle.
    pub handle: Option<String>,

    /// QuestDB compatibility: the SQL query parameter.
    ///
    /// Note: in Polaroid, SQL execution is planned via DataFusion/Ballista.
    /// For now this endpoint primarily supports `handle=`.
    pub query: Option<String>,

    /// Limit rows returned (default: 1_000).
    pub limit: Option<usize>,

    /// Response format (default: json).
    ///
    /// Supported: json
    pub fmt: Option<String>,
}

#[derive(Debug, Serialize)]
struct QuestDbLikeResponse {
    query: String,
    columns: Vec<QuestDbLikeColumn>,
    dataset: Vec<Vec<Value>>,
    count: usize,
}

#[derive(Debug, Serialize)]
struct QuestDbLikeColumn {
    name: String,
    #[serde(rename = "type")]
    ty: String,
}

pub fn router(state: HttpApiState) -> Router {
    Router::new()
        .route("/ping", get(ping))
        .route("/exec", get(exec))
        .with_state(state)
}

pub async fn serve(bind: SocketAddr, state: HttpApiState) -> Result<(), std::io::Error> {
    let listener = tokio::net::TcpListener::bind(bind).await?;
    info!("🌐 HTTP API listening on http://{}", bind);
    axum::serve(listener, router(state)).await
}

async fn ping() -> &'static str {
    "ok"
}

async fn exec(State(state): State<HttpApiState>, Query(q): Query<ExecQuery>) -> Response {
    let fmt = q.fmt.as_deref().unwrap_or("json");
    if fmt != "json" {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Unsupported fmt. Only fmt=json is supported."})),
        )
            .into_response();
    }

    let limit = q.limit.unwrap_or(1_000);

    match (q.handle.as_deref(), q.query.as_deref()) {
        (Some(handle), _) => match state.handle_manager.get_dataframe(handle) {
            Ok(df) => match dataframe_to_questdb_like_json(&df, limit, format!("handle:{handle}")) {
                Ok(resp) => (StatusCode::OK, Json(resp)).into_response(),
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": format!("Failed to render dataframe: {e}")})),
                )
                    .into_response(),
            },
            Err(e) => (
                StatusCode::NOT_FOUND,
                Json(json!({"error": format!("{e}")})),
            )
                .into_response(),
        },
        (None, Some(sql)) => {
            // QuestDB compatibility mode: proxy /exec?query=... to QuestDB if configured.
            // This makes Polaroid usable as a single entrypoint for time-series + metadata.
            let questdb_url = std::env::var("POLAROID_QUESTDB_HTTP_URL")
                .or_else(|_| std::env::var("QUESTDB_HTTP_URL"))
                .ok();

            let Some(base) = questdb_url else {
                return (
                    StatusCode::PRECONDITION_FAILED,
                    Json(json!({
                        "error": "QuestDB is not configured.",
                        "how": "Set POLAROID_QUESTDB_HTTP_URL=http://questdb:9000 (or QUESTDB_HTTP_URL).",
                        "note": "Polaroid will proxy /exec?query=... to QuestDB when configured."
                    })),
                )
                    .into_response();
            };

            let url = format!("{}/exec", base.trim_end_matches('/'));
            let client = reqwest::Client::new();
            let resp = match client
                .get(url)
                .query(&[("query", sql), ("fmt", "json")])
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    return (
                        StatusCode::BAD_GATEWAY,
                        Json(json!({"error": format!("Failed to reach QuestDB: {e}")})),
                    )
                        .into_response();
                }
            };

            let status = StatusCode::from_u16(resp.status().as_u16())
                .unwrap_or(StatusCode::BAD_GATEWAY);

            let body = match resp.bytes().await {
                Ok(b) => b,
                Err(e) => {
                    return (
                        StatusCode::BAD_GATEWAY,
                        Json(json!({"error": format!("Failed to read QuestDB response: {e}")})),
                    )
                        .into_response();
                }
            };

            (status, body).into_response()
        }
        (None, None) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Missing parameter: provide handle=... or query=..."})),
        )
            .into_response(),
    }
}

fn dataframe_to_questdb_like_json(
    df: &DataFrame,
    limit: usize,
    query: String,
) -> Result<QuestDbLikeResponse, PolarsError> {
    let df = if df.height() > limit {
        df.head(Some(limit))
    } else {
        df.clone()
    };

    let columns = df
        .get_columns()
        .iter()
        .map(|s| QuestDbLikeColumn {
            name: s.name().to_string(),
            ty: questdb_type_name(s.dtype()),
        })
        .collect::<Vec<_>>();

    let mut dataset = Vec::with_capacity(df.height());
    for row_idx in 0..df.height() {
        let mut row = Vec::with_capacity(df.width());
        for s in df.get_columns() {
            let av = s.get(row_idx)?;
            row.push(anyvalue_to_json(&av));
        }
        dataset.push(row);
    }

    Ok(QuestDbLikeResponse {
        query,
        columns,
        dataset,
        count: df.height(),
    })
}

fn questdb_type_name(dtype: &DataType) -> String {
    match dtype {
        DataType::Boolean => "BOOLEAN",
        DataType::Int8 => "BYTE",
        DataType::Int16 => "SHORT",
        DataType::Int32 => "INT",
        DataType::Int64 => "LONG",
        DataType::UInt8 | DataType::UInt16 | DataType::UInt32 | DataType::UInt64 => "LONG",
        DataType::Float32 => "FLOAT",
        DataType::Float64 => "DOUBLE",
        DataType::String => "STRING",
        DataType::Date => "DATE",
        DataType::Time => "TIME",
        DataType::Datetime(_, _) => "TIMESTAMP",
        DataType::Duration(_) => "LONG",
        _ => "STRING",
    }
    .to_string()
}

fn anyvalue_to_json(v: &AnyValue) -> Value {
    use base64::engine::general_purpose::STANDARD as BASE64;
    use base64::Engine as _;

    match v {
        AnyValue::Null => Value::Null,
        AnyValue::Boolean(b) => json!(b),
        AnyValue::Int8(x) => json!(x),
        AnyValue::Int16(x) => json!(x),
        AnyValue::Int32(x) => json!(x),
        AnyValue::Int64(x) => json!(x),
        AnyValue::UInt8(x) => json!(*x as u64),
        AnyValue::UInt16(x) => json!(*x as u64),
        AnyValue::UInt32(x) => json!(*x as u64),
        AnyValue::UInt64(x) => json!(x),
        AnyValue::Float32(x) => json!(x),
        AnyValue::Float64(x) => json!(x),
        AnyValue::String(x) => json!(x),
        AnyValue::StringOwned(x) => json!(x),
        AnyValue::Binary(x) => json!(BASE64.encode(x)),
        AnyValue::BinaryOwned(x) => json!(BASE64.encode(x)),
        AnyValue::Date(x) => json!(x),
        AnyValue::Time(x) => json!(x),
        AnyValue::Datetime(x, _, _) => json!(x),
        AnyValue::Duration(x, _) => json!(x),
        _ => json!(v.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode as HttpStatus};
    use http_body_util::BodyExt;
    use parking_lot::Mutex;
    use serde_json::Value;
    use std::net::SocketAddr;
    use tokio::net::TcpListener;
    use tower::ServiceExt;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    async fn body_to_json(resp: axum::response::Response) -> Value {
        let status = resp.status();
        assert!(status.is_success(), "expected success, got {status}");
        let collected = resp.into_body().collect().await.unwrap();
        let bytes = collected.to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    async fn body_to_bytes(resp: axum::response::Response) -> (HttpStatus, Vec<u8>) {
        let status = resp.status();
        let collected = resp.into_body().collect().await.unwrap();
        (status, collected.to_bytes().to_vec())
    }

    #[tokio::test]
    async fn ping_ok() {
        let hm = Arc::new(HandleManager::default());
        let app = router(HttpApiState {
            handle_manager: hm,
        });

        let resp = app
            .oneshot(Request::builder().uri("/ping").body(Body::empty()).unwrap())
            .await
            .unwrap();

        let (status, bytes) = body_to_bytes(resp).await;
        assert_eq!(status, HttpStatus::OK);
        assert_eq!(String::from_utf8(bytes).unwrap(), "ok");
    }

    #[tokio::test]
    async fn exec_requires_handle_or_query() {
        let hm = Arc::new(HandleManager::default());
        let app = router(HttpApiState {
            handle_manager: hm,
        });

        let resp = app
            .oneshot(Request::builder().uri("/exec").body(Body::empty()).unwrap())
            .await
            .unwrap();

        let (status, bytes) = body_to_bytes(resp).await;
        assert_eq!(status, HttpStatus::BAD_REQUEST);
        let json: Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json.get("error").is_some());
    }

    #[tokio::test]
    async fn exec_rejects_unsupported_format() {
        let hm = Arc::new(HandleManager::default());
        let app = router(HttpApiState {
            handle_manager: hm,
        });

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/exec?fmt=csv&handle=abc")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let (status, bytes) = body_to_bytes(resp).await;
        assert_eq!(status, HttpStatus::BAD_REQUEST);
        let json: Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json["error"].as_str().unwrap().contains("fmt=json"));
    }

    #[tokio::test]
    async fn exec_handle_mode_returns_questdb_like_json_with_limit() {
        let hm = Arc::new(HandleManager::default());
        let df = df! {
            "b" => &[true, false, true],
            "i" => &[1i64, 2i64, 3i64],
            "s" => &["x", "y", "z"],
        }
        .unwrap();
        let handle = hm.create_handle(df);

        let app = router(HttpApiState {
            handle_manager: hm,
        });

        let uri = format!("/exec?handle={handle}&limit=2");
        let resp = app
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();

        let json = body_to_json(resp).await;
        assert_eq!(json["count"].as_u64().unwrap(), 2);

        let cols = json["columns"].as_array().unwrap();
        assert_eq!(cols.len(), 3);
        let col_types = cols
            .iter()
            .map(|c| c["type"].as_str().unwrap().to_string())
            .collect::<Vec<_>>();

        // Ensure we emit stable, QuestDB-like type names.
        assert!(col_types.contains(&"BOOLEAN".to_string()));
        assert!(col_types.contains(&"LONG".to_string()));
        assert!(col_types.contains(&"STRING".to_string()));

        let dataset = json["dataset"].as_array().unwrap();
        assert_eq!(dataset.len(), 2);
        assert_eq!(dataset[0].as_array().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn exec_handle_missing_is_404() {
        let hm = Arc::new(HandleManager::default());
        let app = router(HttpApiState {
            handle_manager: hm,
        });

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/exec?handle=does-not-exist")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let (status, bytes) = body_to_bytes(resp).await;
        assert_eq!(status, HttpStatus::NOT_FOUND);
        let json: Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json.get("error").is_some());
    }

    #[tokio::test]
    async fn exec_query_mode_requires_questdb_env() {
        let _guard = ENV_LOCK.lock();
        std::env::remove_var("POLAROID_QUESTDB_HTTP_URL");
        std::env::remove_var("QUESTDB_HTTP_URL");

        let hm = Arc::new(HandleManager::default());
        let app = router(HttpApiState {
            handle_manager: hm,
        });

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/exec?query=select%201")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let (status, bytes) = body_to_bytes(resp).await;
        assert_eq!(status, HttpStatus::PRECONDITION_FAILED);
        let json: Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json["error"].as_str().unwrap().contains("QuestDB"));
    }

    async fn spawn_mock_questdb() -> (String, tokio::task::JoinHandle<()>) {
        async fn mock_exec(Query(q): Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
            let query = q.get("query").cloned().unwrap_or_default();
            let fmt = q.get("fmt").cloned().unwrap_or_default();

            (
                HttpStatus::OK,
                Json(json!({
                    "query": query,
                    "fmt": fmt,
                    "columns": [{"name": "x", "type": "INT"}],
                    "dataset": [[1]],
                    "count": 1
                })),
            )
        }

        let app = Router::new().route("/exec", get(mock_exec));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        let base = format!("http://{}", addr);

        let handle = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        (base, handle)
    }

    #[tokio::test]
    async fn exec_query_mode_proxies_to_questdb() {
        let _guard = ENV_LOCK.lock();
        let (base, server) = spawn_mock_questdb().await;
        std::env::set_var("POLAROID_QUESTDB_HTTP_URL", &base);

        let hm = Arc::new(HandleManager::default());
        let app = router(HttpApiState {
            handle_manager: hm,
        });

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/exec?query=select%2042")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let json = body_to_json(resp).await;
        assert_eq!(json["query"].as_str().unwrap(), "select 42");
        assert_eq!(json["fmt"].as_str().unwrap(), "json");

        std::env::remove_var("POLAROID_QUESTDB_HTTP_URL");
        server.abort();
    }
}
