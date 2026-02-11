//! S3 Sink Connector
//!
//! Writes StreamHouse records to Amazon S3 in configurable formats (JSON, Parquet, Avro).
//! Supports time-based partitioning (none, hourly, daily) and configurable batching.
//!
//! ## Configuration
//!
//! | Key                | Description                              | Default  |
//! |--------------------|------------------------------------------|----------|
//! | `s3.bucket`        | S3 bucket name                           | required |
//! | `s3.prefix`        | Key prefix for objects                   | required |
//! | `s3.region`        | AWS region                               | required |
//! | `format`           | Output format: `json`, `parquet`, `avro` | `json`   |
//! | `batch.size`       | Records per file                         | `10000`  |
//! | `batch.interval_ms`| Maximum age of a batch before flush (ms) | `60000`  |
//! | `partitioning`     | Time partitioning: `none`, `hourly`, `daily` | `none` |

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use chrono::{TimeZone, Utc};
use object_store::aws::AmazonS3Builder;
use object_store::path::Path as ObjectPath;
use object_store::ObjectStore;
use uuid::Uuid;

use arrow::array::{BinaryArray, StringArray, UInt32Array, UInt64Array};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use parquet::arrow::ArrowWriter;

use crate::error::{ConnectorError, Result};
use crate::traits::{SinkConnector, SinkRecord};

/// Output format for S3 objects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Json,
    Parquet,
    Avro,
}

impl OutputFormat {
    /// Parse from a string (case-insensitive).
    pub fn from_str_config(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "json" | "ndjson" => Ok(OutputFormat::Json),
            "parquet" => Ok(OutputFormat::Parquet),
            "avro" => Ok(OutputFormat::Avro),
            other => Err(ConnectorError::ConfigError(format!(
                "unknown output format: '{}'",
                other
            ))),
        }
    }

    /// File extension for this format.
    pub fn extension(&self) -> &str {
        match self {
            OutputFormat::Json => "json",
            OutputFormat::Parquet => "parquet",
            OutputFormat::Avro => "avro",
        }
    }
}

/// Time-based partitioning strategy for S3 keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Partitioning {
    None,
    Hourly,
    Daily,
}

impl Partitioning {
    /// Parse from a string (case-insensitive).
    pub fn from_str_config(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "none" => Ok(Partitioning::None),
            "hourly" => Ok(Partitioning::Hourly),
            "daily" => Ok(Partitioning::Daily),
            other => Err(ConnectorError::ConfigError(format!(
                "unknown partitioning: '{}'",
                other
            ))),
        }
    }
}

/// Configuration parsed from the connector's config map.
#[derive(Debug, Clone)]
pub struct S3SinkConfig {
    pub bucket: String,
    pub prefix: String,
    pub region: String,
    pub format: OutputFormat,
    pub batch_size: usize,
    pub batch_interval_ms: u64,
    pub partitioning: Partitioning,
}

impl S3SinkConfig {
    /// Parse an S3SinkConfig from a string key-value map.
    pub fn from_config_map(config: &HashMap<String, String>) -> Result<Self> {
        let bucket = config
            .get("s3.bucket")
            .ok_or_else(|| ConnectorError::ConfigError("missing required 's3.bucket'".to_string()))?
            .clone();
        let prefix = config
            .get("s3.prefix")
            .ok_or_else(|| ConnectorError::ConfigError("missing required 's3.prefix'".to_string()))?
            .clone();
        let region = config
            .get("s3.region")
            .ok_or_else(|| ConnectorError::ConfigError("missing required 's3.region'".to_string()))?
            .clone();

        let format = config
            .get("format")
            .map(|s| OutputFormat::from_str_config(s))
            .transpose()?
            .unwrap_or(OutputFormat::Json);

        let batch_size = config
            .get("batch.size")
            .map(|s| {
                s.parse::<usize>()
                    .map_err(|e| ConnectorError::ConfigError(format!("invalid batch.size: {}", e)))
            })
            .transpose()?
            .unwrap_or(10_000);

        let batch_interval_ms = config
            .get("batch.interval_ms")
            .map(|s| {
                s.parse::<u64>().map_err(|e| {
                    ConnectorError::ConfigError(format!("invalid batch.interval_ms: {}", e))
                })
            })
            .transpose()?
            .unwrap_or(60_000);

        let partitioning = config
            .get("partitioning")
            .map(|s| Partitioning::from_str_config(s))
            .transpose()?
            .unwrap_or(Partitioning::None);

        Ok(S3SinkConfig {
            bucket,
            prefix,
            region,
            format,
            batch_size,
            batch_interval_ms,
            partitioning,
        })
    }
}

/// S3 Sink Connector implementation.
///
/// Accumulates records in a buffer and writes them to S3 when the buffer
/// reaches `batch_size` or when `flush()` is called explicitly.
pub struct S3SinkConnector {
    name: String,
    config: S3SinkConfig,
    store: Option<Arc<dyn ObjectStore>>,
    buffer: Vec<SinkRecord>,
}

impl S3SinkConnector {
    /// Create a new S3SinkConnector with the given name and config map.
    pub fn new(name: &str, config_map: &HashMap<String, String>) -> Result<Self> {
        let config = S3SinkConfig::from_config_map(config_map)?;
        Ok(Self {
            name: name.to_string(),
            config,
            store: None,
            buffer: Vec::new(),
        })
    }

    /// Create with an already-parsed config.
    pub fn with_config(name: &str, config: S3SinkConfig) -> Self {
        Self {
            name: name.to_string(),
            config,
            store: None,
            buffer: Vec::new(),
        }
    }

    /// Create with an injected ObjectStore (useful for testing).
    pub fn with_store(name: &str, config: S3SinkConfig, store: Arc<dyn ObjectStore>) -> Self {
        Self {
            name: name.to_string(),
            config,
            store: Some(store),
            buffer: Vec::new(),
        }
    }

    /// Build the S3 key path for a batch of records.
    fn build_key(&self, topic: &str, timestamp_ms: u64) -> String {
        let uuid = Uuid::new_v4();
        let ext = self.config.format.extension();

        match self.config.partitioning {
            Partitioning::None => {
                format!("{}/{}/{}.{}", self.config.prefix, topic, uuid, ext)
            }
            Partitioning::Hourly => {
                let dt = Utc.timestamp_millis_opt(timestamp_ms as i64).unwrap();
                format!(
                    "{}/{}/year={}/month={:02}/day={:02}/hour={:02}/{}.{}",
                    self.config.prefix,
                    topic,
                    dt.format("%Y"),
                    dt.format("%m"),
                    dt.format("%d"),
                    dt.format("%H"),
                    uuid,
                    ext
                )
            }
            Partitioning::Daily => {
                let dt = Utc.timestamp_millis_opt(timestamp_ms as i64).unwrap();
                format!(
                    "{}/{}/year={}/month={:02}/day={:02}/{}.{}",
                    self.config.prefix,
                    topic,
                    dt.format("%Y"),
                    dt.format("%m"),
                    dt.format("%d"),
                    uuid,
                    ext
                )
            }
        }
    }

    /// Serialize a batch of records to NDJSON bytes.
    fn serialize_ndjson(records: &[SinkRecord]) -> Result<Bytes> {
        let mut output = Vec::new();
        for record in records {
            let doc = serde_json::json!({
                "topic": record.topic,
                "partition": record.partition,
                "offset": record.offset,
                "timestamp": record.timestamp,
                "key": record.key.as_ref().map(|k| String::from_utf8_lossy(k).to_string()),
                "value": String::from_utf8_lossy(&record.value).to_string(),
            });
            serde_json::to_writer(&mut output, &doc)
                .map_err(|e| ConnectorError::SerializationError(format!("JSON write error: {}", e)))?;
            output.push(b'\n');
        }
        Ok(Bytes::from(output))
    }

    /// Serialize a batch of records to Parquet bytes.
    fn serialize_parquet(records: &[SinkRecord]) -> Result<Bytes> {
        let schema = Arc::new(Schema::new(vec![
            Field::new("topic", DataType::Utf8, false),
            Field::new("partition", DataType::UInt32, false),
            Field::new("offset", DataType::UInt64, false),
            Field::new("timestamp", DataType::UInt64, false),
            Field::new("key", DataType::Binary, true),
            Field::new("value", DataType::Binary, false),
        ]));

        let topics: Vec<&str> = records.iter().map(|r| r.topic.as_str()).collect();
        let partitions: Vec<u32> = records.iter().map(|r| r.partition).collect();
        let offsets: Vec<u64> = records.iter().map(|r| r.offset).collect();
        let timestamps: Vec<u64> = records.iter().map(|r| r.timestamp).collect();
        let keys: Vec<Option<&[u8]>> = records
            .iter()
            .map(|r| r.key.as_ref().map(|k| k.as_ref()))
            .collect();
        let values: Vec<&[u8]> = records.iter().map(|r| r.value.as_ref()).collect();

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from(topics)),
                Arc::new(UInt32Array::from(partitions)),
                Arc::new(UInt64Array::from(offsets)),
                Arc::new(UInt64Array::from(timestamps)),
                Arc::new(BinaryArray::from(keys)),
                Arc::new(BinaryArray::from(values)),
            ],
        )
        .map_err(|e| ConnectorError::SerializationError(format!("Arrow error: {}", e)))?;

        let mut buf = Vec::new();
        let mut writer = ArrowWriter::try_new(&mut buf, schema, None)
            .map_err(|e| ConnectorError::SerializationError(format!("Parquet writer error: {}", e)))?;
        writer
            .write(&batch)
            .map_err(|e| ConnectorError::SerializationError(format!("Parquet write error: {}", e)))?;
        writer
            .close()
            .map_err(|e| ConnectorError::SerializationError(format!("Parquet close error: {}", e)))?;

        Ok(Bytes::from(buf))
    }

    /// Serialize a batch of records to Avro bytes.
    fn serialize_avro(records: &[SinkRecord]) -> Result<Bytes> {
        use apache_avro::types::Record as AvroRecord;
        use apache_avro::{Schema as AvroSchema, Writer};

        let raw_schema = r#"{
            "type": "record",
            "name": "SinkRecord",
            "fields": [
                {"name": "topic", "type": "string"},
                {"name": "partition", "type": "int"},
                {"name": "offset", "type": "long"},
                {"name": "timestamp", "type": "long"},
                {"name": "key", "type": ["null", "bytes"], "default": null},
                {"name": "value", "type": "bytes"}
            ]
        }"#;

        let schema = AvroSchema::parse_str(raw_schema)
            .map_err(|e| ConnectorError::SerializationError(format!("Avro schema error: {}", e)))?;

        let mut writer = Writer::new(&schema, Vec::new());

        for record in records {
            let mut avro_record = AvroRecord::new(&schema)
                .ok_or_else(|| ConnectorError::SerializationError("failed to create Avro record".to_string()))?;

            avro_record.put("topic", record.topic.as_str());
            avro_record.put("partition", record.partition as i32);
            avro_record.put("offset", record.offset as i64);
            avro_record.put("timestamp", record.timestamp as i64);

            match &record.key {
                Some(k) => {
                    avro_record.put(
                        "key",
                        apache_avro::types::Value::Union(
                            1,
                            Box::new(apache_avro::types::Value::Bytes(k.to_vec())),
                        ),
                    );
                }
                None => {
                    avro_record.put(
                        "key",
                        apache_avro::types::Value::Union(
                            0,
                            Box::new(apache_avro::types::Value::Null),
                        ),
                    );
                }
            }

            avro_record.put(
                "value",
                apache_avro::types::Value::Bytes(record.value.to_vec()),
            );

            writer.append(avro_record).map_err(|e| {
                ConnectorError::SerializationError(format!("Avro write error: {}", e))
            })?;
        }

        let encoded = writer.into_inner().map_err(|e| {
            ConnectorError::SerializationError(format!("Avro flush error: {}", e))
        })?;

        Ok(Bytes::from(encoded))
    }

    /// Write a batch of records to S3.
    async fn write_batch(&self, records: &[SinkRecord]) -> Result<()> {
        if records.is_empty() {
            return Ok(());
        }

        let store = self
            .store
            .as_ref()
            .ok_or_else(|| ConnectorError::ConnectionError("S3 store not initialized".to_string()))?;

        let topic = &records[0].topic;
        let timestamp = records[0].timestamp;
        let key = self.build_key(topic, timestamp);

        let data = match self.config.format {
            OutputFormat::Json => Self::serialize_ndjson(records)?,
            OutputFormat::Parquet => Self::serialize_parquet(records)?,
            OutputFormat::Avro => Self::serialize_avro(records)?,
        };

        let path = ObjectPath::from(key);
        store
            .put(&path, data.into())
            .await
            .map_err(|e| ConnectorError::SinkError(format!("S3 put error: {}", e)))?;

        Ok(())
    }
}

#[async_trait]
impl SinkConnector for S3SinkConnector {
    async fn start(&mut self) -> Result<()> {
        if self.store.is_none() {
            let store = AmazonS3Builder::new()
                .with_bucket_name(&self.config.bucket)
                .with_region(&self.config.region)
                .build()
                .map_err(|e| {
                    ConnectorError::ConnectionError(format!("failed to build S3 client: {}", e))
                })?;
            self.store = Some(Arc::new(store));
        }
        tracing::info!(connector = %self.name, "S3 sink connector started");
        Ok(())
    }

    async fn put(&mut self, records: &[SinkRecord]) -> Result<()> {
        self.buffer.extend(records.iter().cloned());

        // Auto-flush if we hit the batch size
        while self.buffer.len() >= self.config.batch_size {
            let batch: Vec<SinkRecord> = self.buffer.drain(..self.config.batch_size).collect();
            self.write_batch(&batch).await?;
        }

        Ok(())
    }

    async fn flush(&mut self) -> Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }
        let batch: Vec<SinkRecord> = self.buffer.drain(..).collect();
        self.write_batch(&batch).await?;
        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        self.flush().await?;
        tracing::info!(connector = %self.name, "S3 sink connector stopped");
        Ok(())
    }

    fn name(&self) -> &str {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_config_map() -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert("s3.bucket".to_string(), "test-bucket".to_string());
        m.insert("s3.prefix".to_string(), "data/output".to_string());
        m.insert("s3.region".to_string(), "us-east-1".to_string());
        m
    }

    fn sample_records(n: usize) -> Vec<SinkRecord> {
        (0..n)
            .map(|i| SinkRecord {
                topic: "test-topic".to_string(),
                partition: 0,
                offset: i as u64,
                timestamp: 1_700_000_000_000 + (i as u64 * 1000),
                key: Some(Bytes::from(format!("key-{}", i))),
                value: Bytes::from(format!(r#"{{"index":{}}}"#, i)),
            })
            .collect()
    }

    // ---------------------------------------------------------------
    // Config parsing
    // ---------------------------------------------------------------

    #[test]
    fn test_config_parse_minimal() {
        let m = base_config_map();
        let config = S3SinkConfig::from_config_map(&m).unwrap();
        assert_eq!(config.bucket, "test-bucket");
        assert_eq!(config.prefix, "data/output");
        assert_eq!(config.region, "us-east-1");
        assert_eq!(config.format, OutputFormat::Json);
        assert_eq!(config.batch_size, 10_000);
        assert_eq!(config.batch_interval_ms, 60_000);
        assert_eq!(config.partitioning, Partitioning::None);
    }

    #[test]
    fn test_config_parse_all_options() {
        let mut m = base_config_map();
        m.insert("format".to_string(), "parquet".to_string());
        m.insert("batch.size".to_string(), "5000".to_string());
        m.insert("batch.interval_ms".to_string(), "30000".to_string());
        m.insert("partitioning".to_string(), "hourly".to_string());

        let config = S3SinkConfig::from_config_map(&m).unwrap();
        assert_eq!(config.format, OutputFormat::Parquet);
        assert_eq!(config.batch_size, 5000);
        assert_eq!(config.batch_interval_ms, 30_000);
        assert_eq!(config.partitioning, Partitioning::Hourly);
    }

    #[test]
    fn test_config_missing_bucket() {
        let mut m = base_config_map();
        m.remove("s3.bucket");
        let result = S3SinkConfig::from_config_map(&m);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_missing_prefix() {
        let mut m = base_config_map();
        m.remove("s3.prefix");
        let result = S3SinkConfig::from_config_map(&m);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_missing_region() {
        let mut m = base_config_map();
        m.remove("s3.region");
        let result = S3SinkConfig::from_config_map(&m);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_invalid_format() {
        let mut m = base_config_map();
        m.insert("format".to_string(), "xml".to_string());
        let result = S3SinkConfig::from_config_map(&m);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_invalid_batch_size() {
        let mut m = base_config_map();
        m.insert("batch.size".to_string(), "not_a_number".to_string());
        let result = S3SinkConfig::from_config_map(&m);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_invalid_batch_interval() {
        let mut m = base_config_map();
        m.insert("batch.interval_ms".to_string(), "abc".to_string());
        let result = S3SinkConfig::from_config_map(&m);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_invalid_partitioning() {
        let mut m = base_config_map();
        m.insert("partitioning".to_string(), "weekly".to_string());
        let result = S3SinkConfig::from_config_map(&m);
        assert!(result.is_err());
    }

    // ---------------------------------------------------------------
    // OutputFormat
    // ---------------------------------------------------------------

    #[test]
    fn test_output_format_from_str() {
        assert_eq!(OutputFormat::from_str_config("json").unwrap(), OutputFormat::Json);
        assert_eq!(OutputFormat::from_str_config("JSON").unwrap(), OutputFormat::Json);
        assert_eq!(OutputFormat::from_str_config("ndjson").unwrap(), OutputFormat::Json);
        assert_eq!(OutputFormat::from_str_config("parquet").unwrap(), OutputFormat::Parquet);
        assert_eq!(OutputFormat::from_str_config("PARQUET").unwrap(), OutputFormat::Parquet);
        assert_eq!(OutputFormat::from_str_config("avro").unwrap(), OutputFormat::Avro);
        assert_eq!(OutputFormat::from_str_config("AVRO").unwrap(), OutputFormat::Avro);
    }

    #[test]
    fn test_output_format_extension() {
        assert_eq!(OutputFormat::Json.extension(), "json");
        assert_eq!(OutputFormat::Parquet.extension(), "parquet");
        assert_eq!(OutputFormat::Avro.extension(), "avro");
    }

    // ---------------------------------------------------------------
    // Partitioning
    // ---------------------------------------------------------------

    #[test]
    fn test_partitioning_from_str() {
        assert_eq!(Partitioning::from_str_config("none").unwrap(), Partitioning::None);
        assert_eq!(Partitioning::from_str_config("NONE").unwrap(), Partitioning::None);
        assert_eq!(Partitioning::from_str_config("hourly").unwrap(), Partitioning::Hourly);
        assert_eq!(Partitioning::from_str_config("daily").unwrap(), Partitioning::Daily);
    }

    #[test]
    fn test_partitioning_from_str_invalid() {
        assert!(Partitioning::from_str_config("weekly").is_err());
        assert!(Partitioning::from_str_config("monthly").is_err());
    }

    // ---------------------------------------------------------------
    // Key building
    // ---------------------------------------------------------------

    #[test]
    fn test_build_key_no_partitioning() {
        let config = S3SinkConfig {
            bucket: "b".to_string(),
            prefix: "prefix".to_string(),
            region: "r".to_string(),
            format: OutputFormat::Json,
            batch_size: 10_000,
            batch_interval_ms: 60_000,
            partitioning: Partitioning::None,
        };
        let connector = S3SinkConnector::with_config("test", config);
        let key = connector.build_key("events", 1_700_000_000_000);
        assert!(key.starts_with("prefix/events/"));
        assert!(key.ends_with(".json"));
    }

    #[test]
    fn test_build_key_hourly_partitioning() {
        let config = S3SinkConfig {
            bucket: "b".to_string(),
            prefix: "out".to_string(),
            region: "r".to_string(),
            format: OutputFormat::Parquet,
            batch_size: 10_000,
            batch_interval_ms: 60_000,
            partitioning: Partitioning::Hourly,
        };
        let connector = S3SinkConnector::with_config("test", config);
        // 2023-11-14 22:13:20 UTC
        let key = connector.build_key("logs", 1_700_000_000_000);
        assert!(key.starts_with("out/logs/year="));
        assert!(key.contains("/month="));
        assert!(key.contains("/day="));
        assert!(key.contains("/hour="));
        assert!(key.ends_with(".parquet"));
    }

    #[test]
    fn test_build_key_daily_partitioning() {
        let config = S3SinkConfig {
            bucket: "b".to_string(),
            prefix: "data".to_string(),
            region: "r".to_string(),
            format: OutputFormat::Avro,
            batch_size: 10_000,
            batch_interval_ms: 60_000,
            partitioning: Partitioning::Daily,
        };
        let connector = S3SinkConnector::with_config("test", config);
        let key = connector.build_key("metrics", 1_700_000_000_000);
        assert!(key.starts_with("data/metrics/year="));
        assert!(key.contains("/month="));
        assert!(key.contains("/day="));
        assert!(!key.contains("/hour="));
        assert!(key.ends_with(".avro"));
    }

    // ---------------------------------------------------------------
    // Serialization - NDJSON
    // ---------------------------------------------------------------

    #[test]
    fn test_serialize_ndjson() {
        let records = sample_records(3);
        let data = S3SinkConnector::serialize_ndjson(&records).unwrap();
        let text = String::from_utf8(data.to_vec()).unwrap();
        let lines: Vec<&str> = text.trim().split('\n').collect();
        assert_eq!(lines.len(), 3);

        // Each line should be valid JSON
        for line in &lines {
            let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
            assert!(parsed.get("topic").is_some());
            assert!(parsed.get("offset").is_some());
            assert!(parsed.get("timestamp").is_some());
        }
    }

    #[test]
    fn test_serialize_ndjson_empty() {
        let data = S3SinkConnector::serialize_ndjson(&[]).unwrap();
        assert!(data.is_empty());
    }

    #[test]
    fn test_serialize_ndjson_no_key() {
        let records = vec![SinkRecord {
            topic: "t".to_string(),
            partition: 0,
            offset: 0,
            timestamp: 0,
            key: None,
            value: Bytes::from("data"),
        }];
        let data = S3SinkConnector::serialize_ndjson(&records).unwrap();
        let text = String::from_utf8(data.to_vec()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
        assert!(parsed["key"].is_null());
    }

    // ---------------------------------------------------------------
    // Serialization - Parquet
    // ---------------------------------------------------------------

    #[test]
    fn test_serialize_parquet() {
        let records = sample_records(5);
        let data = S3SinkConnector::serialize_parquet(&records).unwrap();
        // Parquet files start with "PAR1" magic bytes
        assert!(data.len() > 4);
        assert_eq!(&data[0..4], b"PAR1");
    }

    #[test]
    fn test_serialize_parquet_single_record() {
        let records = sample_records(1);
        let data = S3SinkConnector::serialize_parquet(&records).unwrap();
        assert!(&data[0..4] == b"PAR1");
    }

    // ---------------------------------------------------------------
    // Serialization - Avro
    // ---------------------------------------------------------------

    #[test]
    fn test_serialize_avro() {
        let records = sample_records(3);
        let data = S3SinkConnector::serialize_avro(&records).unwrap();
        // Avro files start with "Obj" followed by 0x01
        assert!(data.len() > 4);
        assert_eq!(&data[0..3], b"Obj");
    }

    #[test]
    fn test_serialize_avro_null_key() {
        let records = vec![SinkRecord {
            topic: "t".to_string(),
            partition: 0,
            offset: 0,
            timestamp: 0,
            key: None,
            value: Bytes::from("data"),
        }];
        let data = S3SinkConnector::serialize_avro(&records).unwrap();
        assert!(!data.is_empty());
    }

    // ---------------------------------------------------------------
    // Constructor
    // ---------------------------------------------------------------

    #[test]
    fn test_new_connector() {
        let m = base_config_map();
        let connector = S3SinkConnector::new("my-s3-sink", &m).unwrap();
        assert_eq!(connector.name(), "my-s3-sink");
        assert!(connector.buffer.is_empty());
        assert!(connector.store.is_none());
    }

    #[test]
    fn test_new_connector_missing_config() {
        let m = HashMap::new();
        let result = S3SinkConnector::new("bad", &m);
        assert!(result.is_err());
    }

    #[test]
    fn test_with_config() {
        let config = S3SinkConfig {
            bucket: "b".to_string(),
            prefix: "p".to_string(),
            region: "r".to_string(),
            format: OutputFormat::Parquet,
            batch_size: 100,
            batch_interval_ms: 5000,
            partitioning: Partitioning::Daily,
        };
        let connector = S3SinkConnector::with_config("named", config);
        assert_eq!(connector.name(), "named");
        assert_eq!(connector.config.format, OutputFormat::Parquet);
        assert_eq!(connector.config.batch_size, 100);
    }
}
