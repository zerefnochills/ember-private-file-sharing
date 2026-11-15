//! Filesystem storage backend for Ember relay server.

use std::path::{Path, PathBuf};
use async_trait::async_trait;
use tokio::fs;

use crate::error::ServerError;
use crate::model::BlobRecord;
use crate::storage::Storage;

/// Filesystem-backed storage storing metadata as JSON and parts as binary files.
pub struct FilesystemStorage {
    root: PathBuf,
}

impl FilesystemStorage {
    pub async fn new(root: impl AsRef<Path>) -> Result<Self, ServerError> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(&root)
            .await
            .map_err(|e| ServerError::Storage(format!("Failed to create storage root: {e}")))?;
        Ok(Self { root })
    }

    fn blob_dir(&self, id: &str) -> PathBuf {
        self.root.join(id)
    }

    fn meta_path(&self, id: &str) -> PathBuf {
        self.blob_dir(id).join("meta.json")
    }

    fn part_path(&self, id: &str, part_index: u32) -> PathBuf {
        self.blob_dir(id).join(format!("part_{part_index}.bin"))
    }

    fn tombstones_dir(&self) -> PathBuf {
        self.root.join(".tombstones")
    }

    fn tombstone_path(&self, id: &str) -> PathBuf {
        self.tombstones_dir().join(id)
    }
}

#[async_trait]
impl Storage for FilesystemStorage {
    async fn create_blob(&self, record: &BlobRecord) -> Result<(), ServerError> {
        let dir = self.blob_dir(&record.id);
        fs::create_dir_all(&dir)
            .await
            .map_err(|e| ServerError::Storage(format!("Failed to create blob dir: {e}")))?;

        let meta_json = serde_json::to_vec_pretty(record)
            .map_err(|e| ServerError::Storage(format!("Failed to serialize metadata: {e}")))?;

        fs::write(self.meta_path(&record.id), meta_json)
            .await
            .map_err(|e| ServerError::Storage(format!("Failed to write metadata: {e}")))?;

        Ok(())
    }

    async fn get_blob(&self, id: &str) -> Result<Option<BlobRecord>, ServerError> {
        let path = self.meta_path(id);
        if !path.exists() {
            return Ok(None);
        }

        let bytes = fs::read(&path)
            .await
            .map_err(|e| ServerError::Storage(format!("Failed to read metadata: {e}")))?;

        let record: BlobRecord = serde_json::from_slice(&bytes)
            .map_err(|e| ServerError::Storage(format!("Failed to parse metadata: {e}")))?;

        Ok(Some(record))
    }

    async fn update_blob(&self, record: &BlobRecord) -> Result<(), ServerError> {
        let meta_json = serde_json::to_vec_pretty(record)
            .map_err(|e| ServerError::Storage(format!("Failed to serialize metadata: {e}")))?;

        fs::write(self.meta_path(&record.id), meta_json)
            .await
            .map_err(|e| ServerError::Storage(format!("Failed to update metadata: {e}")))?;

        Ok(())
    }

    async fn delete_blob(&self, id: &str) -> Result<(), ServerError> {
        let dir = self.blob_dir(id);
        if dir.exists() {
            fs::remove_dir_all(&dir)
                .await
                .map_err(|e| ServerError::Storage(format!("Failed to delete blob dir: {e}")))?;
        }
        Ok(())
    }

    async fn write_part(&self, id: &str, part_index: u32, data: &[u8]) -> Result<(), ServerError> {
        let path = self.part_path(id, part_index);
        fs::write(&path, data)
            .await
            .map_err(|e| ServerError::Storage(format!("Failed to write part {part_index}: {e}")))?;
        Ok(())
    }

    async fn read_part(&self, id: &str, part_index: u32) -> Result<Option<Vec<u8>>, ServerError> {
        let path = self.part_path(id, part_index);
        if !path.exists() {
            return Ok(None);
        }

        let bytes = fs::read(&path)
            .await
            .map_err(|e| ServerError::Storage(format!("Failed to read part {part_index}: {e}")))?;
        Ok(Some(bytes))
    }

    async fn part_exists(&self, id: &str, part_index: u32) -> Result<bool, ServerError> {
        Ok(self.part_path(id, part_index).exists())
    }

    async fn get_part_size(&self, id: &str, part_index: u32) -> Result<Option<u64>, ServerError> {
        let path = self.part_path(id, part_index);
        match fs::metadata(&path).await {
            Ok(m) => Ok(Some(m.len())),
            Err(_) => Ok(None),
        }
    }

    async fn list_blobs(&self) -> Result<Vec<BlobRecord>, ServerError> {
        let mut entries = match fs::read_dir(&self.root).await {
            Ok(e) => e,
            Err(_) => return Ok(Vec::new()),
        };

        let mut blobs = Vec::new();
        while let Ok(Some(entry)) = entries.next_entry().await {
            let meta_file = entry.path().join("meta.json");
            if meta_file.exists() {
                if let Ok(bytes) = fs::read(&meta_file).await {
                    if let Ok(record) = serde_json::from_slice::<BlobRecord>(&bytes) {
                        blobs.push(record);
                    }
                }
            }
        }

        Ok(blobs)
    }

    async fn record_tombstone(&self, id: &str, expires_at: u64) -> Result<(), ServerError> {
        let dir = self.tombstones_dir();
        fs::create_dir_all(&dir)
            .await
            .map_err(|e| ServerError::Storage(format!("Failed to create tombstones dir: {e}")))?;

        let content = expires_at.to_string();
        fs::write(self.tombstone_path(id), content)
            .await
            .map_err(|e| ServerError::Storage(format!("Failed to write tombstone: {e}")))?;
        Ok(())
    }

    async fn is_tombstone(&self, id: &str) -> Result<bool, ServerError> {
        Ok(self.tombstone_path(id).exists())
    }

    async fn sweep_tombstones(&self, now: u64) -> Result<usize, ServerError> {
        let dir = self.tombstones_dir();
        if !dir.exists() {
            return Ok(0);
        }

        let mut entries = match fs::read_dir(&dir).await {
            Ok(e) => e,
            Err(_) => return Ok(0),
        };

        let mut deleted = 0;
        while let Ok(Some(entry)) = entries.next_entry().await {
            if let Ok(content) = fs::read_to_string(entry.path()).await {
                if let Ok(exp) = content.trim().parse::<u64>() {
                    if now >= exp {
                        let _ = fs::remove_file(entry.path()).await;
                        deleted += 1;
                    }
                }
            }
        }
        Ok(deleted)
    }
}
