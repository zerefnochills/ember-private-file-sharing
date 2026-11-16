//! Filesystem storage backend for Ember relay server.

use async_trait::async_trait;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::AsyncWriteExt;

use crate::error::ServerError;
use crate::model::{
    BlobRecord, MailboxMessageRecord, MailboxRecord, ReportRecord, TakedownRecord,
    TransparencyStats,
};
use crate::storage::Storage;
use ember_protocol::MAX_MAILBOX_MESSAGES;

/// Filesystem-backed storage storing metadata as JSON and parts as binary files.
pub struct FilesystemStorage {
    root: PathBuf,
}

/// Returns `id` unchanged if it is a well-formed 22-character base64url identifier, and a
/// fixed placeholder otherwise.
///
/// Identifiers become path components. A value such as `..`, `.` or `/etc` must never be
/// joined onto the storage root (`remove_dir_all(root.join(".."))` would delete the parent
/// directory). The placeholder cannot match any real record, so lookups simply miss.
fn safe_component(id: &str) -> &str {
    if ember_core::validate_blob_id(id).is_ok() {
        id
    } else {
        "invalid-identifier"
    }
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
        self.root.join(safe_component(id))
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
        self.tombstones_dir().join(safe_component(id))
    }

    fn mailboxes_dir(&self) -> PathBuf {
        self.root.join(".mailboxes")
    }

    fn mailbox_dir(&self, id: &str) -> PathBuf {
        self.mailboxes_dir().join(safe_component(id))
    }

    fn mailbox_meta_path(&self, id: &str) -> PathBuf {
        self.mailbox_dir(id).join("meta.json")
    }

    fn mailbox_messages_path(&self, id: &str) -> PathBuf {
        self.mailbox_dir(id).join("messages.json")
    }

    fn takedowns_path(&self) -> PathBuf {
        self.root.join(".takedowns.jsonl")
    }

    fn reports_dir(&self) -> PathBuf {
        self.root.join(".reports")
    }

    fn report_path(&self, id: &str) -> PathBuf {
        self.reports_dir().join(format!("{id}.json"))
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

        let meta_json = serde_json::to_vec_pretty(record).map_err(|e| {
            ServerError::Storage(format!("Failed to serialize mailbox metadata: {e}"))
        })?;

        fs::write(self.mailbox_meta_path(&record.id), meta_json)
            .await
            .map_err(|e| ServerError::Storage(format!("Failed to write mailbox metadata: {e}")))?;

        let empty_msgs: Vec<MailboxMessageRecord> = Vec::new();
        let msgs_json = serde_json::to_vec(&empty_msgs).map_err(|e| {
            ServerError::Storage(format!("Failed to serialize empty messages: {e}"))
        })?;

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
            return Err(ServerError::BadRequest(
                "Mailbox message queue is full".into(),
            ));
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
            fs::write(&msgs_path, updated_bytes).await.map_err(|e| {
                ServerError::Storage(format!("Failed to write drained messages: {e}"))
            })?;
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

    async fn record_takedown(&self, record: TakedownRecord) -> Result<(), ServerError> {
        let mut line = serde_json::to_vec(&record).map_err(|e| {
            ServerError::Storage(format!("Failed to serialize takedown record: {e}"))
        })?;
        line.push(b'\n');

        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.takedowns_path())
            .await
            .map_err(|e| ServerError::Storage(format!("Failed to open takedowns log: {e}")))?;

        file.write_all(&line)
            .await
            .map_err(|e| ServerError::Storage(format!("Failed to append to takedowns log: {e}")))?;

        Ok(())
    }

    async fn list_takedowns(&self) -> Result<Vec<TakedownRecord>, ServerError> {
        let path = self.takedowns_path();
        if !path.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(&path)
            .await
            .map_err(|e| ServerError::Storage(format!("Failed to read takedowns log: {e}")))?;

        let mut takedowns = Vec::new();
        for line in content.lines() {
            let line = line.trim();
            if !line.is_empty() {
                if let Ok(record) = serde_json::from_str::<TakedownRecord>(line) {
                    takedowns.push(record);
                }
            }
        }

        Ok(takedowns)
    }

    async fn save_report(&self, report: ReportRecord) -> Result<(), ServerError> {
        let dir = self.reports_dir();
        fs::create_dir_all(&dir)
            .await
            .map_err(|e| ServerError::Storage(format!("Failed to create reports dir: {e}")))?;

        let path = self.report_path(&report.id);
        let bytes = serde_json::to_vec_pretty(&report)
            .map_err(|e| ServerError::Storage(format!("Failed to serialize report: {e}")))?;

        fs::write(path, bytes)
            .await
            .map_err(|e| ServerError::Storage(format!("Failed to write report: {e}")))?;

        Ok(())
    }

    async fn list_reports(&self) -> Result<Vec<ReportRecord>, ServerError> {
        let dir = self.reports_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut entries = match fs::read_dir(&dir).await {
            Ok(e) => e,
            Err(_) => return Ok(Vec::new()),
        };

        let mut reports = Vec::new();
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json") {
                if let Ok(bytes) = fs::read(&path).await {
                    if let Ok(report) = serde_json::from_slice::<ReportRecord>(&bytes) {
                        reports.push(report);
                    }
                }
            }
        }

        Ok(reports)
    }

    async fn delete_report(&self, id: &str) -> Result<(), ServerError> {
        let path = self.report_path(id);
        if path.exists() {
            fs::remove_file(path)
                .await
                .map_err(|e| ServerError::Storage(format!("Failed to delete report: {e}")))?;
        }
        Ok(())
    }

    async fn get_transparency_stats(&self) -> Result<TransparencyStats, ServerError> {
        let takedowns = self.list_takedowns().await?;
        let reports = self.list_reports().await?;

        let total_takedowns = takedowns.len();
        let total_reports = reports.len();

        let mut takedowns_by_reason = HashMap::new();
        for t in &takedowns {
            *takedowns_by_reason.entry(t.reason.clone()).or_insert(0) += 1;
        }

        let mut reports_by_reason = HashMap::new();
        for r in &reports {
            *reports_by_reason.entry(r.reason.clone()).or_insert(0) += 1;
        }

        Ok(TransparencyStats {
            total_takedowns,
            total_reports,
            takedowns_by_reason,
            reports_by_reason,
        })
    }
}
