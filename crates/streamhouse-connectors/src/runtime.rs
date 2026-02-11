//! Connector runtime for managing the lifecycle of connector instances.
//!
//! The `ConnectorRuntime` spawns and manages sink connectors as background
//! tokio tasks, with support for pause, resume, and stop control signals.
//! It accepts a generic record source (an async stream of `SinkRecord` batches)
//! rather than depending on a specific consumer implementation, keeping
//! the crate decoupled.

use std::collections::HashMap;
use std::pin::Pin;
use std::future::Future;

use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing;

use crate::config::{ConnectorConfig, ConnectorState};
use crate::error::{ConnectorError, Result};
use crate::traits::{SinkConnector, SinkRecord};

/// Control signals sent from the runtime to a running connector task.
#[derive(Debug)]
enum ControlSignal {
    Pause,
    Resume,
    Stop,
}

/// Handle to a running connector task.
struct ConnectorHandle {
    /// Tokio task handle for the connector loop.
    join_handle: JoinHandle<()>,
    /// Channel to send control signals.
    control_tx: mpsc::Sender<ControlSignal>,
    /// Current state of the connector.
    state: ConnectorState,
}

/// A factory function that produces batches of sink records.
///
/// The runtime calls this function repeatedly to obtain records for the sink.
/// Returning an empty vec signals that no records are currently available.
/// Returning an error causes the connector to transition to the Failed state.
pub type RecordSourceFn = Box<
    dyn Fn() -> Pin<Box<dyn Future<Output = Result<Vec<SinkRecord>>> + Send>>
        + Send
        + Sync,
>;

/// Runtime that manages the lifecycle of connector instances.
///
/// # Example
///
/// ```ignore
/// use streamhouse_connectors::runtime::ConnectorRuntime;
///
/// let mut runtime = ConnectorRuntime::new();
/// runtime.start_sink(config, sink, record_source).await?;
/// runtime.pause("my-sink")?;
/// runtime.resume("my-sink")?;
/// runtime.stop("my-sink").await?;
/// ```
pub struct ConnectorRuntime {
    connectors: HashMap<String, ConnectorHandle>,
}

impl ConnectorRuntime {
    /// Create a new empty runtime.
    pub fn new() -> Self {
        Self {
            connectors: HashMap::new(),
        }
    }

    /// Start a sink connector as a background task.
    ///
    /// The connector will be initialized via `start()`, then enter a loop
    /// that polls the record source, passes records to the sink via `put()`,
    /// and periodically calls `flush()`.
    ///
    /// # Arguments
    /// - `config` - Connector configuration.
    /// - `sink` - The sink connector implementation.
    /// - `record_source` - An async function that returns batches of records.
    pub async fn start_sink(
        &mut self,
        config: ConnectorConfig,
        mut sink: Box<dyn SinkConnector>,
        record_source: RecordSourceFn,
    ) -> Result<()> {
        let name = config.name.clone();

        if self.connectors.contains_key(&name) {
            return Err(ConnectorError::RuntimeError(format!(
                "connector '{}' is already running",
                name
            )));
        }

        // Initialize the sink
        sink.start().await?;

        let (control_tx, mut control_rx) = mpsc::channel::<ControlSignal>(16);
        let connector_name = name.clone();

        let join_handle = tokio::spawn(async move {
            let mut paused = false;

            loop {
                // Check for control signals (non-blocking)
                match control_rx.try_recv() {
                    Ok(ControlSignal::Stop) => {
                        tracing::info!(connector = %connector_name, "stopping connector");
                        if let Err(e) = sink.flush().await {
                            tracing::error!(connector = %connector_name, error = %e, "error flushing on stop");
                        }
                        if let Err(e) = sink.stop().await {
                            tracing::error!(connector = %connector_name, error = %e, "error stopping connector");
                        }
                        break;
                    }
                    Ok(ControlSignal::Pause) => {
                        tracing::info!(connector = %connector_name, "pausing connector");
                        paused = true;
                    }
                    Ok(ControlSignal::Resume) => {
                        tracing::info!(connector = %connector_name, "resuming connector");
                        paused = false;
                    }
                    Err(mpsc::error::TryRecvError::Empty) => {}
                    Err(mpsc::error::TryRecvError::Disconnected) => {
                        tracing::warn!(connector = %connector_name, "control channel closed, stopping");
                        let _ = sink.stop().await;
                        break;
                    }
                }

                if paused {
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    continue;
                }

                // Poll for records from the generic record source
                match record_source().await {
                    Ok(records) if records.is_empty() => {
                        // No records available; brief sleep to avoid busy-loop
                        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    }
                    Ok(records) => {
                        if let Err(e) = sink.put(&records).await {
                            tracing::error!(
                                connector = %connector_name,
                                error = %e,
                                "error putting records to sink"
                            );
                            // Continue rather than crash - the runtime can be stopped externally
                        }
                        if let Err(e) = sink.flush().await {
                            tracing::error!(
                                connector = %connector_name,
                                error = %e,
                                "error flushing sink"
                            );
                        }
                    }
                    Err(e) => {
                        tracing::error!(
                            connector = %connector_name,
                            error = %e,
                            "error polling record source"
                        );
                        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
                    }
                }
            }
        });

        self.connectors.insert(
            name,
            ConnectorHandle {
                join_handle,
                control_tx,
                state: ConnectorState::Running,
            },
        );

        Ok(())
    }

    /// Send a pause signal to a running connector.
    pub fn pause(&mut self, name: &str) -> Result<()> {
        let handle = self
            .connectors
            .get_mut(name)
            .ok_or_else(|| ConnectorError::RuntimeError(format!("connector '{}' not found", name)))?;

        if handle.state != ConnectorState::Running {
            return Err(ConnectorError::RuntimeError(format!(
                "connector '{}' is not running (state: {})",
                name, handle.state
            )));
        }

        handle
            .control_tx
            .try_send(ControlSignal::Pause)
            .map_err(|e| ConnectorError::RuntimeError(format!("failed to send pause: {}", e)))?;

        handle.state = ConnectorState::Paused;
        Ok(())
    }

    /// Send a resume signal to a paused connector.
    pub fn resume(&mut self, name: &str) -> Result<()> {
        let handle = self
            .connectors
            .get_mut(name)
            .ok_or_else(|| ConnectorError::RuntimeError(format!("connector '{}' not found", name)))?;

        if handle.state != ConnectorState::Paused {
            return Err(ConnectorError::RuntimeError(format!(
                "connector '{}' is not paused (state: {})",
                name, handle.state
            )));
        }

        handle
            .control_tx
            .try_send(ControlSignal::Resume)
            .map_err(|e| ConnectorError::RuntimeError(format!("failed to send resume: {}", e)))?;

        handle.state = ConnectorState::Running;
        Ok(())
    }

    /// Stop a connector and wait for its task to finish.
    pub async fn stop(&mut self, name: &str) -> Result<()> {
        let handle = self
            .connectors
            .remove(name)
            .ok_or_else(|| ConnectorError::RuntimeError(format!("connector '{}' not found", name)))?;

        let _ = handle.control_tx.send(ControlSignal::Stop).await;
        let _ = handle.join_handle.await;

        Ok(())
    }

    /// Return the current state of a connector, or None if not found.
    pub fn state(&self, name: &str) -> Option<ConnectorState> {
        self.connectors.get(name).map(|h| h.state)
    }

    /// Return the names of all managed connectors.
    pub fn connector_names(&self) -> Vec<&str> {
        self.connectors.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for ConnectorRuntime {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ConnectorType;
    use crate::traits::SinkRecord;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // A simple mock sink that counts put calls
    struct CountingSink {
        name: String,
        put_count: Arc<AtomicUsize>,
        flush_count: Arc<AtomicUsize>,
        started: bool,
        stopped: bool,
    }

    impl CountingSink {
        fn new(name: &str) -> (Self, Arc<AtomicUsize>, Arc<AtomicUsize>) {
            let put_count = Arc::new(AtomicUsize::new(0));
            let flush_count = Arc::new(AtomicUsize::new(0));
            (
                Self {
                    name: name.to_string(),
                    put_count: put_count.clone(),
                    flush_count: flush_count.clone(),
                    started: false,
                    stopped: false,
                },
                put_count,
                flush_count,
            )
        }
    }

    #[async_trait::async_trait]
    impl SinkConnector for CountingSink {
        async fn start(&mut self) -> Result<()> {
            self.started = true;
            Ok(())
        }
        async fn put(&mut self, _records: &[SinkRecord]) -> Result<()> {
            self.put_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        async fn flush(&mut self) -> Result<()> {
            self.flush_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        async fn stop(&mut self) -> Result<()> {
            self.stopped = true;
            Ok(())
        }
        fn name(&self) -> &str {
            &self.name
        }
    }

    fn test_config(name: &str) -> ConnectorConfig {
        ConnectorConfig {
            name: name.to_string(),
            connector_type: ConnectorType::Sink,
            connector_class: "test".to_string(),
            topics: vec!["test-topic".to_string()],
            tasks_max: 1,
            config: HashMap::new(),
        }
    }

    fn empty_record_source() -> RecordSourceFn {
        Box::new(|| Box::pin(async { Ok(vec![]) }))
    }

    // ---------------------------------------------------------------
    // Basic lifecycle
    // ---------------------------------------------------------------

    #[test]
    fn test_runtime_new() {
        let runtime = ConnectorRuntime::new();
        assert!(runtime.connector_names().is_empty());
    }

    #[test]
    fn test_runtime_default() {
        let runtime = ConnectorRuntime::default();
        assert!(runtime.connector_names().is_empty());
    }

    #[tokio::test]
    async fn test_start_and_stop_sink() {
        let mut runtime = ConnectorRuntime::new();
        let (sink, _put_count, _flush_count) = CountingSink::new("test-sink");
        let config = test_config("test-sink");

        runtime
            .start_sink(config, Box::new(sink), empty_record_source())
            .await
            .unwrap();

        assert_eq!(runtime.state("test-sink"), Some(ConnectorState::Running));

        runtime.stop("test-sink").await.unwrap();
        assert!(runtime.state("test-sink").is_none());
    }

    #[tokio::test]
    async fn test_duplicate_name_rejected() {
        let mut runtime = ConnectorRuntime::new();
        let (sink1, _, _) = CountingSink::new("dup");
        let (sink2, _, _) = CountingSink::new("dup");
        let config1 = test_config("dup");
        let config2 = test_config("dup");

        runtime
            .start_sink(config1, Box::new(sink1), empty_record_source())
            .await
            .unwrap();

        let result = runtime
            .start_sink(config2, Box::new(sink2), empty_record_source())
            .await;
        assert!(result.is_err());

        runtime.stop("dup").await.unwrap();
    }

    // ---------------------------------------------------------------
    // Pause / Resume
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn test_pause_and_resume() {
        let mut runtime = ConnectorRuntime::new();
        let (sink, _, _) = CountingSink::new("pr-test");
        let config = test_config("pr-test");

        runtime
            .start_sink(config, Box::new(sink), empty_record_source())
            .await
            .unwrap();

        runtime.pause("pr-test").unwrap();
        assert_eq!(runtime.state("pr-test"), Some(ConnectorState::Paused));

        runtime.resume("pr-test").unwrap();
        assert_eq!(runtime.state("pr-test"), Some(ConnectorState::Running));

        runtime.stop("pr-test").await.unwrap();
    }

    #[tokio::test]
    async fn test_pause_non_running_fails() {
        let mut runtime = ConnectorRuntime::new();
        let (sink, _, _) = CountingSink::new("p2");
        let config = test_config("p2");

        runtime
            .start_sink(config, Box::new(sink), empty_record_source())
            .await
            .unwrap();

        runtime.pause("p2").unwrap();
        // Second pause should fail
        let result = runtime.pause("p2");
        assert!(result.is_err());

        runtime.stop("p2").await.unwrap();
    }

    #[tokio::test]
    async fn test_resume_non_paused_fails() {
        let mut runtime = ConnectorRuntime::new();
        let (sink, _, _) = CountingSink::new("r2");
        let config = test_config("r2");

        runtime
            .start_sink(config, Box::new(sink), empty_record_source())
            .await
            .unwrap();

        // Resume without pause should fail
        let result = runtime.resume("r2");
        assert!(result.is_err());

        runtime.stop("r2").await.unwrap();
    }

    // ---------------------------------------------------------------
    // Not-found errors
    // ---------------------------------------------------------------

    #[test]
    fn test_pause_not_found() {
        let mut runtime = ConnectorRuntime::new();
        let result = runtime.pause("ghost");
        assert!(result.is_err());
    }

    #[test]
    fn test_resume_not_found() {
        let mut runtime = ConnectorRuntime::new();
        let result = runtime.resume("ghost");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_stop_not_found() {
        let mut runtime = ConnectorRuntime::new();
        let result = runtime.stop("ghost").await;
        assert!(result.is_err());
    }

    // ---------------------------------------------------------------
    // State queries
    // ---------------------------------------------------------------

    #[test]
    fn test_state_not_found() {
        let runtime = ConnectorRuntime::new();
        assert!(runtime.state("missing").is_none());
    }

    #[tokio::test]
    async fn test_connector_names() {
        let mut runtime = ConnectorRuntime::new();
        let (s1, _, _) = CountingSink::new("alpha");
        let (s2, _, _) = CountingSink::new("beta");

        runtime
            .start_sink(test_config("alpha"), Box::new(s1), empty_record_source())
            .await
            .unwrap();
        runtime
            .start_sink(test_config("beta"), Box::new(s2), empty_record_source())
            .await
            .unwrap();

        let mut names = runtime.connector_names();
        names.sort();
        assert_eq!(names, vec!["alpha", "beta"]);

        runtime.stop("alpha").await.unwrap();
        runtime.stop("beta").await.unwrap();
    }
}
