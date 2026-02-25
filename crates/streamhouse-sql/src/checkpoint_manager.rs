//! Checkpoint management for durable stream processing state
//!
//! Provides [`CheckpointManager`] which coordinates saving, loading, listing,
//! and automatic cleanup of [`StateCheckpoint`] snapshots. Checkpoints are
//! persisted via a pluggable [`CheckpointStorage`] backend.

use std::path::PathBuf;

use async_trait::async_trait;
use tracing::{debug, info};

use crate::streaming::StateCheckpoint;
use crate::error::SqlError;
use crate::Result;

// ---------------------------------------------------------------------------
// CheckpointStorage trait
// ---------------------------------------------------------------------------

/// Async storage backend for persisting checkpoints.
#[async_trait]
pub trait CheckpointStorage: Send + Sync {
    /// Save a checkpoint under the given `checkpoint_id`.
    async fn save(&self, checkpoint_id: &str, checkpoint: &StateCheckpoint) -> Result<()>;

    /// Load a checkpoint by its `checkpoint_id`.
    async fn load(&self, checkpoint_id: &str) -> Result<Option<StateCheckpoint>>;

    /// List all stored checkpoint IDs, ordered by creation time (oldest first).
    async fn list(&self) -> Result<Vec<String>>;

    /// Delete a checkpoint by its `checkpoint_id`.
    async fn delete(&self, checkpoint_id: &str) -> Result<()>;
}

// ---------------------------------------------------------------------------
// FileCheckpointStorage
// ---------------------------------------------------------------------------

/// A [`CheckpointStorage`] that writes checkpoints to individual files in a
/// directory. Writes are atomic: data is first written to a `.tmp` file and
/// then renamed into place.
pub struct FileCheckpointStorage {
    directory: PathBuf,
}

impl FileCheckpointStorage {
    /// Create a new file-based checkpoint storage.
    ///
    /// The directory is created if it does not already exist.
    pub fn new(directory: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&directory).map_err(|e| {
            SqlError::CheckpointError(format!(
                "failed to create checkpoint directory {}: {e}",
                directory.display()
            ))
        })?;
        Ok(Self { directory })
    }

    fn checkpoint_path(&self, checkpoint_id: &str) -> PathBuf {
        self.directory.join(format!("{checkpoint_id}.ckpt"))
    }

    fn tmp_path(&self, checkpoint_id: &str) -> PathBuf {
        self.directory.join(format!("{checkpoint_id}.ckpt.tmp"))
    }
}

#[async_trait]
impl CheckpointStorage for FileCheckpointStorage {
    async fn save(&self, checkpoint_id: &str, checkpoint: &StateCheckpoint) -> Result<()> {
        let data = bincode::serialize(checkpoint).map_err(|e| {
            SqlError::CheckpointError(format!("failed to serialize checkpoint: {e}"))
        })?;

        let tmp = self.tmp_path(checkpoint_id);
        let final_path = self.checkpoint_path(checkpoint_id);

        // Write to temporary file first
        tokio::fs::write(&tmp, &data).await.map_err(|e| {
            SqlError::CheckpointError(format!("failed to write tmp checkpoint file: {e}"))
        })?;

        // Atomic rename
        tokio::fs::rename(&tmp, &final_path).await.map_err(|e| {
            SqlError::CheckpointError(format!("failed to rename checkpoint file: {e}"))
        })?;

        debug!(checkpoint_id = %checkpoint_id, bytes = data.len(), "checkpoint saved");
        Ok(())
    }

    async fn load(&self, checkpoint_id: &str) -> Result<Option<StateCheckpoint>> {
        let path = self.checkpoint_path(checkpoint_id);
        if !path.exists() {
            return Ok(None);
        }
        let data = tokio::fs::read(&path).await.map_err(|e| {
            SqlError::CheckpointError(format!("failed to read checkpoint file: {e}"))
        })?;
        let checkpoint: StateCheckpoint = bincode::deserialize(&data).map_err(|e| {
            SqlError::CheckpointError(format!("failed to deserialize checkpoint: {e}"))
        })?;
        Ok(Some(checkpoint))
    }

    async fn list(&self) -> Result<Vec<String>> {
        let mut entries = Vec::new();
        let mut dir = tokio::fs::read_dir(&self.directory).await.map_err(|e| {
            SqlError::CheckpointError(format!("failed to read checkpoint directory: {e}"))
        })?;

        while let Some(entry) = dir.next_entry().await.map_err(|e| {
            SqlError::CheckpointError(format!("failed to read directory entry: {e}"))
        })? {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".ckpt") {
                let id = name.trim_end_matches(".ckpt").to_string();
                // Get modification time for sorting
                let metadata = entry.metadata().await.ok();
                let modified = metadata
                    .and_then(|m| m.modified().ok())
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                entries.push((id, modified));
            }
        }

        // Sort by modification time (oldest first)
        entries.sort_by(|a, b| a.1.cmp(&b.1));
        Ok(entries.into_iter().map(|(id, _)| id).collect())
    }

    async fn delete(&self, checkpoint_id: &str) -> Result<()> {
        let path = self.checkpoint_path(checkpoint_id);
        if path.exists() {
            tokio::fs::remove_file(&path).await.map_err(|e| {
                SqlError::CheckpointError(format!("failed to delete checkpoint file: {e}"))
            })?;
            debug!(checkpoint_id = %checkpoint_id, "checkpoint deleted");
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// CheckpointManager
// ---------------------------------------------------------------------------

/// Manages checkpoint lifecycle: saving, loading, listing, and automatic
/// retention-based cleanup.
pub struct CheckpointManager {
    storage: Box<dyn CheckpointStorage>,
    /// Maximum number of checkpoints to retain. Older checkpoints beyond this
    /// limit are automatically deleted after each save.
    max_retained_checkpoints: usize,
}

impl CheckpointManager {
    /// Create a new checkpoint manager.
    pub fn new(storage: Box<dyn CheckpointStorage>, max_retained_checkpoints: usize) -> Self {
        Self {
            storage,
            max_retained_checkpoints,
        }
    }

    /// Save a checkpoint and clean up old ones if necessary.
    pub async fn save_checkpoint(&self, checkpoint: &StateCheckpoint) -> Result<()> {
        self.storage
            .save(&checkpoint.checkpoint_id, checkpoint)
            .await?;
        info!(
            checkpoint_id = %checkpoint.checkpoint_id,
            "checkpoint saved"
        );

        // Auto-cleanup
        self.cleanup_old_checkpoints().await?;
        Ok(())
    }

    /// Load a checkpoint by ID.
    pub async fn load_checkpoint(&self, checkpoint_id: &str) -> Result<Option<StateCheckpoint>> {
        self.storage.load(checkpoint_id).await
    }

    /// List all checkpoint IDs (oldest first).
    pub async fn list_checkpoints(&self) -> Result<Vec<String>> {
        self.storage.list().await
    }

    /// Delete a specific checkpoint.
    pub async fn delete_checkpoint(&self, checkpoint_id: &str) -> Result<()> {
        self.storage.delete(checkpoint_id).await
    }

    /// Load the most recent checkpoint.
    pub async fn load_latest_checkpoint(&self) -> Result<Option<StateCheckpoint>> {
        let ids = self.storage.list().await?;
        if let Some(latest_id) = ids.last() {
            self.storage.load(latest_id).await
        } else {
            Ok(None)
        }
    }

    /// Remove old checkpoints, keeping at most `max_retained_checkpoints`.
    async fn cleanup_old_checkpoints(&self) -> Result<()> {
        let ids = self.storage.list().await?;
        if ids.len() > self.max_retained_checkpoints {
            let to_remove = ids.len() - self.max_retained_checkpoints;
            for id in &ids[..to_remove] {
                self.storage.delete(id).await?;
                debug!(checkpoint_id = %id, "old checkpoint cleaned up");
            }
            info!(
                removed = to_remove,
                retained = self.max_retained_checkpoints,
                "checkpoint cleanup completed"
            );
        }
        Ok(())
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tempfile::TempDir;
    use uuid::Uuid;

    fn make_checkpoint(id: &str) -> StateCheckpoint {
        StateCheckpoint {
            checkpoint_id: id.to_string(),
            processor_id: "test-proc".to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            offsets: HashMap::new(),
            state_snapshot: vec![1, 2, 3, 4],
        }
    }

    fn make_random_checkpoint() -> StateCheckpoint {
        make_checkpoint(&Uuid::new_v4().to_string())
    }

    #[tokio::test]
    async fn test_file_storage_save_and_load() {
        let dir = TempDir::new().unwrap();
        let storage = FileCheckpointStorage::new(dir.path().to_path_buf()).unwrap();

        let cp = make_checkpoint("cp-1");
        storage.save("cp-1", &cp).await.unwrap();

        let loaded = storage.load("cp-1").await.unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().checkpoint_id, "cp-1");
    }

    #[tokio::test]
    async fn test_file_storage_load_nonexistent() {
        let dir = TempDir::new().unwrap();
        let storage = FileCheckpointStorage::new(dir.path().to_path_buf()).unwrap();

        let loaded = storage.load("nonexistent").await.unwrap();
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn test_file_storage_list() {
        let dir = TempDir::new().unwrap();
        let storage = FileCheckpointStorage::new(dir.path().to_path_buf()).unwrap();

        storage
            .save("cp-a", &make_checkpoint("cp-a"))
            .await
            .unwrap();
        // Small delay to ensure different modification times
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        storage
            .save("cp-b", &make_checkpoint("cp-b"))
            .await
            .unwrap();

        let ids = storage.list().await.unwrap();
        assert_eq!(ids.len(), 2);
    }

    #[tokio::test]
    async fn test_file_storage_delete() {
        let dir = TempDir::new().unwrap();
        let storage = FileCheckpointStorage::new(dir.path().to_path_buf()).unwrap();

        storage
            .save("cp-del", &make_checkpoint("cp-del"))
            .await
            .unwrap();
        storage.delete("cp-del").await.unwrap();

        let loaded = storage.load("cp-del").await.unwrap();
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn test_file_storage_delete_nonexistent_is_ok() {
        let dir = TempDir::new().unwrap();
        let storage = FileCheckpointStorage::new(dir.path().to_path_buf()).unwrap();
        storage.delete("does-not-exist").await.unwrap();
    }

    #[tokio::test]
    async fn test_manager_save_and_load() {
        let dir = TempDir::new().unwrap();
        let storage = FileCheckpointStorage::new(dir.path().to_path_buf()).unwrap();
        let manager = CheckpointManager::new(Box::new(storage), 5);

        let cp = make_random_checkpoint();
        let id = cp.checkpoint_id.clone();
        manager.save_checkpoint(&cp).await.unwrap();

        let loaded = manager.load_checkpoint(&id).await.unwrap();
        assert!(loaded.is_some());
    }

    #[tokio::test]
    async fn test_manager_auto_cleanup() {
        let dir = TempDir::new().unwrap();
        let storage = FileCheckpointStorage::new(dir.path().to_path_buf()).unwrap();
        let manager = CheckpointManager::new(Box::new(storage), 2);

        for i in 0..4 {
            let cp = make_checkpoint(&format!("cp-{i}"));
            manager.save_checkpoint(&cp).await.unwrap();
            // Small delay so file modification times differ
            tokio::time::sleep(std::time::Duration::from_millis(15)).await;
        }

        let ids = manager.list_checkpoints().await.unwrap();
        assert!(ids.len() <= 2, "should retain at most 2, got {}", ids.len());
    }

    #[tokio::test]
    async fn test_manager_load_latest() {
        let dir = TempDir::new().unwrap();
        let storage = FileCheckpointStorage::new(dir.path().to_path_buf()).unwrap();
        let manager = CheckpointManager::new(Box::new(storage), 10);

        manager
            .save_checkpoint(&make_checkpoint("cp-old"))
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(15)).await;
        manager
            .save_checkpoint(&make_checkpoint("cp-new"))
            .await
            .unwrap();

        let latest = manager.load_latest_checkpoint().await.unwrap();
        assert!(latest.is_some());
        assert_eq!(latest.unwrap().checkpoint_id, "cp-new");
    }

    #[tokio::test]
    async fn test_manager_load_latest_empty() {
        let dir = TempDir::new().unwrap();
        let storage = FileCheckpointStorage::new(dir.path().to_path_buf()).unwrap();
        let manager = CheckpointManager::new(Box::new(storage), 5);

        let latest = manager.load_latest_checkpoint().await.unwrap();
        assert!(latest.is_none());
    }

    #[tokio::test]
    async fn test_manager_delete_checkpoint() {
        let dir = TempDir::new().unwrap();
        let storage = FileCheckpointStorage::new(dir.path().to_path_buf()).unwrap();
        let manager = CheckpointManager::new(Box::new(storage), 10);

        let cp = make_checkpoint("cp-to-delete");
        manager.save_checkpoint(&cp).await.unwrap();
        manager.delete_checkpoint("cp-to-delete").await.unwrap();

        let loaded = manager.load_checkpoint("cp-to-delete").await.unwrap();
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn test_manager_list_checkpoints() {
        let dir = TempDir::new().unwrap();
        let storage = FileCheckpointStorage::new(dir.path().to_path_buf()).unwrap();
        let manager = CheckpointManager::new(Box::new(storage), 10);

        for i in 0..3 {
            let cp = make_checkpoint(&format!("cp-list-{i}"));
            manager.save_checkpoint(&cp).await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        let ids = manager.list_checkpoints().await.unwrap();
        assert_eq!(ids.len(), 3);
    }

    #[tokio::test]
    async fn test_checkpoint_data_integrity() {
        let dir = TempDir::new().unwrap();
        let storage = FileCheckpointStorage::new(dir.path().to_path_buf()).unwrap();

        let mut offsets = HashMap::new();
        offsets.insert("topic1".to_string(), {
            let mut m = HashMap::new();
            m.insert(0u32, 42u64);
            m
        });

        let cp = StateCheckpoint {
            checkpoint_id: "integrity-test".to_string(),
            processor_id: "proc-1".to_string(),
            timestamp: 1234567890,
            offsets,
            state_snapshot: vec![10, 20, 30, 40, 50],
        };

        storage.save("integrity-test", &cp).await.unwrap();
        let loaded = storage.load("integrity-test").await.unwrap().unwrap();

        assert_eq!(loaded.checkpoint_id, "integrity-test");
        assert_eq!(loaded.processor_id, "proc-1");
        assert_eq!(loaded.timestamp, 1234567890);
        assert_eq!(loaded.state_snapshot, vec![10, 20, 30, 40, 50]);
        assert_eq!(loaded.offsets["topic1"][&0], 42);
    }
}
