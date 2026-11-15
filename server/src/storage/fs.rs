//! Filesystem storage backend for Ember relay server.

use async_trait::async_trait;
use std::path::{Path, PathBuf};
use tokio::fs;

use crate::error::ServerError;
use crate::model::{BlobRecord, MailboxMessageRecord, MailboxRecord};
use crate::storage::Storage;
use ember_protocol::MAX_MAILBOX_MESSAGES;

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

    fn mailboxes_dir(&self) -> PathBuf {
        self.root.join(".mailboxes")
    }

    fn mailbox_dir(&self, id: &str) -> PathBuf {
        self.mailboxes_dir().join(id)
    }

    fn mailbox_meta_path(&self, id: &str) -> PathBuf {
        self.mailbox_dir(id).join("meta.json")
    }

    fn mailbox_messages_path(&self, id: &str) -> PathBuf {
        self.mailbox_dir(id).join("messages.json")
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

    async fn create_mailbox(&self, record: &MailboxRecord) -> Result<(), ServerError> {
        let dir = self.mailbox_dir(&record.id);
        fs::create_dir_all(&dir)
            .await
            .map_err(|e| ServerError::Storage(format!("Failed to create mailbox dir: {e}")))?;

        let meta_json = serde_json::to_vec_pretty(record)
            .map_err(|e| ServerError::Storage(format!("Failed to serialize mailbox metadata: {e}")))?;

        fs::write(self.mailbox_meta_path(&record.id), meta_json)
            .await
            .map_err(|e| ServerError::Storage(format!("Failed to write mailbox metadata: {e}")))?;

        let empty_msgs: Vec<MailboxMessageRecord> = Vec::new();
        let msgs_json = serde_json::to_vec(&empty_msgs)
            .map_err(|e| ServerError::Storage(format!("Failed to serialize empty messages: {e}")))?;

        fs::write(self.mailbox_messages_path(&record.id), msgs_json)
            .await
            .map_err(|e| ServerError::Storage(format!("Failed to write mailbox messages: {e}")))?;

        Ok(())
    }

    async fn get_mailbox(&self, id: &str) -> Result<Option<MailboxRecord>, ServerError> {
        let path = self.mailbox_meta_path(id);
        if !path.exists() {
            return Ok(None);
        }

        let bytes = fs::read(&path)
            .await
            .map_err(|e| ServerError::Storage(format!("Failed to read mailbox metadata: {e}")))?;

        let record: MailboxRecord = serde_json::from_slice(&bytes)
            .map_err(|e| ServerError::Storage(format!("Failed to parse mailbox metadata: {e}")))?;

        Ok(Some(record))
    }

    async fn delete_mailbox(&self, id: &str) -> Result<(), ServerError> {
        let dir = self.mailbox_dir(id);
        if dir.exists() {
            fs::remove_dir_all(&dir)
                .await
                .map_err(|e| ServerError::Storage(format!("Failed to delete mailbox dir: {e}")))?;
        }
        Ok(())
    }

    async fn push_mailbox_message(
        &self,
        id: &str,
        payload: Vec<u8>,
        now: u64,
    ) -> Result<u64, ServerError> {
        if !self.mailbox_meta_path(id).exists() {
            return Err(ServerError::NotFound);
        }

        let msgs_path = self.mailbox_messages_path(id);
        let mut messages: Vec<MailboxMessageRecord> = if msgs_path.exists() {
            let bytes = fs::read(&msgs_path)
                .await
                .map_err(|e| ServerError::Storage(format!("Failed to read messages: {e}")))?;
            serde_json::from_slice(&bytes).unwrap_or_default()
        } else {
            Vec::new()
        };

        if messages.len() >= MAX_MAILBOX_MESSAGES {
            return Err(ServerError::Validation("Mailbox message queue is full".into()));
        }

        let next_id = messages.last().map(|m| m.id).unwrap_or(0) + 1;
        messages.push(MailboxMessageRecord {
            id: next_id,
            payload,
            created_at: now,
        });

        let updated_bytes = serde_json::to_vec(&messages)
            .map_err(|e| ServerError::Storage(format!("Failed to serialize messages: {e}")))?;
        fs::write(&msgs_path, updated_bytes)
            .await
            .map_err(|e| ServerError::Storage(format!("Failed to write messages: {e}")))?;

        Ok(next_id)
    }

    async fn get_mailbox_messages(
        &self,
        id: &str,
        after_id: Option<u64>,
        drain: bool,
    ) -> Result<Vec<MailboxMessageRecord>, ServerError> {
        if !self.mailbox_meta_path(id).exists() {
            return Err(ServerError::NotFound);
        }

        let msgs_path = self.mailbox_messages_path(id);
        if !msgs_path.exists() {
            return Ok(Vec::new());
        }

        let bytes = fs::read(&msgs_path)
            .await
            .map_err(|e| ServerError::Storage(format!("Failed to read messages: {e}")))?;
        let mut messages: Vec<MailboxMessageRecord> =
            serde_json::from_slice(&bytes).unwrap_or_default();

        let threshold = after_id.unwrap_or(0);
        let matched: Vec<MailboxMessageRecord> = messages
            .iter()
            .filter(|m| m.id > threshold)
            .cloned()
            .collect();

        if drain && !matched.is_empty() {
            let matched_ids: std::collections::HashSet<u64> =
                matched.iter().map(|m| m.id).collect();
            messages.retain(|m| !matched_ids.contains(&m.id));

            let updated_bytes = serde_json::to_vec(&messages)
                .map_err(|e| ServerError::Storage(format!("Failed to serialize messages: {e}")))?;
            fs::write(&msgs_path, updated_bytes)
                .await
                .map_err(|e| ServerError::Storage(format!("Failed to write drained messages: {e}")))?;
        }

        Ok(matched)
    }

    async fn list_mailboxes(&self) -> Result<Vec<MailboxRecord>, ServerError> {
        let dir = self.mailboxes_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut entries = match fs::read_dir(&dir).await {
            Ok(e) => e,
            Err(_) => return Ok(Vec::new()),
        };

        let mut mailboxes = Vec::new();
        while let Ok(Some(entry)) = entries.next_entry().await {
            let meta_file = entry.path().join("meta.json");
            if meta_file.exists() {
                if let Ok(bytes) = fs::read(&meta_file).await {
                    if let Ok(record) = serde_json::from_slice::<MailboxRecord>(&bytes) {
                        mailboxes.push(record);
                    }
                }
            }
        }

        Ok(mailboxes)
    }
}
