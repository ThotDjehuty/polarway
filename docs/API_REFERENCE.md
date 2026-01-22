# Polaroid API Reference

## Module: `polars-streaming-adaptive::sources`

### Core Traits

#### `StreamingSource`

The main trait that all streaming sources must implement.

```rust
#[async_trait]
pub trait StreamingSource: Send + Sync {
    async fn metadata(&self) -> SourceResult<SourceMetadata>;
    async fn read_chunk(&mut self) -> SourceResult<Option<DataFrame>>;
    fn stats(&self) -> StreamingStats;
    async fn reset(&mut self) -> SourceResult<()>;
    async fn seek(&mut self, position: u64) -> SourceResult<()>;
    async fn close(&mut self) -> SourceResult<()>;
    fn has_more(&self) -> bool;
}
```

**Methods:**

- `metadata() -> SourceResult<SourceMetadata>`: Returns source metadata including schema, size, and capabilities
- `read_chunk() -> SourceResult<Option<DataFrame>>`: Reads the next chunk, returns `None` when exhausted
- `stats() -> StreamingStats`: Gets current streaming statistics (bytes read, memory usage, etc.)
- `reset() -> SourceResult<()>`: Resets stream to beginning (if supported)
- `seek(position: u64) -> SourceResult<()>`: Seeks to specific byte position (if supported)
- `close() -> SourceResult<()>`: Closes source and releases resources
- `has_more() -> bool`: Returns `true` if more data is available

**Example:**
```rust
let mut source: Box<dyn StreamingSource> = registry.create("csv", config)?;

while source.has_more() {
    if let Some(chunk) = source.read_chunk().await? {
        println!("Read {} rows", chunk.height());
    }
}

source.close().await?;
```

#### `SourceFactory`

Factory trait for creating streaming sources.

```rust
pub trait SourceFactory: Send + Sync {
    fn create(&self, config: SourceConfig) -> SourceResult<Box<dyn StreamingSource>>;
}
```

### Data Types

#### `SourceConfig`

Configuration for creating a streaming source.

```rust
pub struct SourceConfig {
    pub location: String,
    pub credentials: Option<Credentials>,
    pub memory_limit: Option<usize>,
    pub chunk_size: Option<usize>,
    pub parallel: bool,
    pub prefetch: bool,
    pub options: HashMap<String, String>,
}
```

**Builder Methods:**
- `new(location: impl Into<String>) -> Self`
- `with_credentials(credentials: Credentials) -> Self`
- `with_memory_limit(bytes: usize) -> Self`
- `with_chunk_size(size: usize) -> Self`
- `with_parallel(enable: bool) -> Self`
- `with_option(key: impl Into<String>, value: impl Into<String>) -> Self`

**Example:**
```rust
let config = SourceConfig::new("s3://bucket/data.parquet")
    .with_memory_limit(4_000_000_000)
    .with_chunk_size(10_000)
    .with_parallel(true)
    .with_option("region", "us-east-1");
```

#### `Credentials`

Authentication credentials for various sources.

```rust
pub enum Credentials {
    Aws {
        access_key_id: String,
        secret_access_key: String,
        region: Option<String>,
        session_token: Option<String>,
    },
    Azure {
        account_name: String,
        account_key: String,
    },
    Gcs {
        project_id: String,
        credentials_json: String,
    },
    Basic {
        username: String,
        password: String,
    },
    Bearer {
        token: String,
    },
    ApiKey {
        key: String,
        header_name: Option<String>,
    },
    // ... more variants
}
```

#### `SourceMetadata`

Information about a streaming source.

```rust
pub struct SourceMetadata {
    pub size_bytes: Option<u64>,
    pub num_records: Option<usize>,
    pub schema: Option<SchemaRef>,
    pub seekable: bool,
    pub parallelizable: bool,
}
```

#### `StreamingStats`

Statistics about streaming progress.

```rust
pub struct StreamingStats {
    pub bytes_read: u64,
    pub records_processed: usize,
    pub chunks_read: usize,
    pub memory_bytes: u64,
    pub avg_chunk_time_ms: f64,
}
```

### Error Types

#### `SourceError`

Comprehensive error type for source operations.

```rust
pub enum SourceError {
    Io(std::io::Error),
    Network(String),
    Auth(String),
    Config(String),
    UnsupportedSource(String),
    UnsupportedOperation(String),
    MemoryLimit(usize),
    EmptySource,
    PolarsError(String),
    CloudError(String),
    DatabaseError(String),
    KafkaError(String),
    ParseError(String),
    Other(String),
}
```

### Registry

#### `SourceRegistry`

Central registry for creating sources by type.

```rust
impl SourceRegistry {
    pub fn new() -> Self;
    pub fn register(&mut self, name: &str, factory: Box<dyn SourceFactory>);
    pub fn create(&self, source_type: &str, config: SourceConfig) 
        -> SourceResult<Box<dyn StreamingSource>>;
}
```

**Built-in Sources:**
- `"csv"` - CSV files
- `"filesystem"` - Local files with memory mapping
- `"http"` - HTTP/REST APIs
- `"s3"` - AWS S3
- `"azure"` - Azure Blob Storage
- `"gcs"` - Google Cloud Storage
- `"dynamodb"` - AWS DynamoDB
- `"kafka"` - Apache Kafka

**Example:**
```rust
let mut registry = SourceRegistry::new();

// Register custom source
registry.register("mysql", Box::new(MySqlSourceFactory));

// Create source
let source = registry.create("csv", config)?;
```

### Builder

#### `AdaptiveStreamBuilder`

Builder for configuring adaptive streaming behavior.

```rust
impl AdaptiveStreamBuilder {
    pub fn new(source: Box<dyn StreamingSource>) -> Self;
    pub fn with_memory_limit(self, bytes: usize) -> Self;
    pub fn with_chunk_size(self, size: usize) -> Self;
    pub fn with_parallel(self, enable: bool) -> Self;
    pub fn with_prefetch(self, enable: bool) -> Self;
    pub async fn collect(self) -> SourceResult<DataFrame>;
}
```

**Example:**
```rust
let df = AdaptiveStreamBuilder::new(source)
    .with_memory_limit(2_000_000_000)
    .with_parallel(true)
    .with_prefetch(true)
    .collect()
    .await?;
```

## Python API

### Installation

```python
pip install polaroid
```

### Quick Start

```python
import polaroid as pl

# Adaptive CSV reading
df = pl.scan_csv("large.csv") \\
    .with_adaptive_streaming() \\
    .with_memory_limit("2GB") \\
    .collect()
```

### Classes

#### `CsvSource`

```python
class CsvSource:
    def __init__(
        self,
        path: str,
        *,
        memory_limit: Optional[Union[int, str]] = None,
        chunk_size: Optional[int] = None,
        has_header: bool = True,
        delimiter: str = ",",
        encoding: str = "utf-8"
    ):
        ...
    
    def __iter__(self) -> Iterator[DataFrame]:
        ...
    
    def stats(self) -> Dict[str, Any]:
        ...
```

**Example:**
```python
from polaroid.streaming import CsvSource

source = CsvSource("data.csv", memory_limit="1GB", chunk_size=10_000)
for chunk in source:
    print(f"Processing {len(chunk)} rows")
    # Process chunk
```

#### `S3Source`

```python
class S3Source:
    def __init__(
        self,
        uri: str,
        *,
        aws_access_key_id: Optional[str] = None,
        aws_secret_access_key: Optional[str] = None,
        region: str = "us-east-1",
        memory_limit: Optional[Union[int, str]] = None
    ):
        ...
```

**Example:**
```python
from polaroid.streaming import S3Source

source = S3Source(
    "s3://my-bucket/data.parquet",
    aws_access_key_id=os.environ["AWS_ACCESS_KEY_ID"],
    aws_secret_access_key=os.environ["AWS_SECRET_ACCESS_KEY"],
    memory_limit="4GB"
)

for chunk in source:
    process(chunk)
```

#### `HttpSource`

```python
class HttpSource:
    def __init__(
        self,
        url: str,
        *,
        method: str = "GET",
        headers: Optional[Dict[str, str]] = None,
        auth_token: Optional[str] = None,
        timeout: int = 30,
        retry_attempts: int = 3
    ):
        ...
```

### Utility Functions

#### `adaptive_scan_csv`

```python
def adaptive_scan_csv(
    path: str,
    memory_limit: Optional[Union[int, str]] = None,
    chunk_size: Optional[int] = None,
    **kwargs
) -> DataFrame:
    """
    Read CSV with adaptive streaming.
    
    Args:
        path: Path to CSV file
        memory_limit: Maximum memory usage (e.g., "2GB", 2_000_000_000)
        chunk_size: Initial chunk size in rows
        **kwargs: Additional CSV options
    
    Returns:
        DataFrame with all data
    """
```

#### `adaptive_scan_parquet`

```python
def adaptive_scan_parquet(
    path: str,
    memory_limit: Optional[Union[int, str]] = None,
    use_mmap: bool = True
) -> DataFrame:
    """Read Parquet with adaptive streaming."""
```

### Memory Utilities

#### `parse_memory_limit`

```python
def parse_memory_limit(limit: Union[int, str]) -> int:
    """
    Parse memory limit string to bytes.
    
    Examples:
        "1GB" -> 1_000_000_000
        "500MB" -> 500_000_000
        "2.5GB" -> 2_500_000_000
    """
```

#### `get_available_memory`

```python
def get_available_memory() -> int:
    """Get currently available system memory in bytes."""
```

## Type Aliases

```rust
pub type SourceResult<T> = Result<T, SourceError>;
pub type SchemaRef = Arc<Schema>;
```

## Constants

```rust
pub const DEFAULT_CHUNK_SIZE: usize = 10_000;
pub const DEFAULT_MEMORY_LIMIT_RATIO: f64 = 0.6; // 60% of available
pub const MIN_CHUNK_SIZE: usize = 100;
pub const MAX_CHUNK_SIZE: usize = 1_000_000;
```

## Feature Flags

```toml
[features]
default = ["csv", "filesystem"]
csv = []
filesystem = []
cloud = ["aws", "azure", "gcs"]
aws = ["aws-sdk-s3", "aws-config"]
azure = ["azure_storage", "azure_storage_blobs"]
gcs = ["google-cloud-storage"]
database = ["dynamodb"]
dynamodb = ["aws-sdk-dynamodb"]
kafka = ["rdkafka"]
http = ["reqwest", "tokio"]
```

## Version History

- **v0.53.0** (Jan 2026): Generic source architecture, multiple adapters
- **v0.52.0** (Dec 2025): Initial adaptive streaming for Parquet
- **v0.51.0** (Nov 2025): Memory-aware chunking prototype
