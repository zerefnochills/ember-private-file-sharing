//! In-memory storage backend for Ember relay server.

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::error::ServerError;
use crate::model::{BlobRecord, MailboxMessageRecord, MailboxRecord};
use crate::storage::Storage;
use ember_protocol::MAX_MAILBOX_MESSAGES;

#[derive(Default)]
struct InnerMemoryStorage {
    blobs: HashMap<String, BlobRecord>,
    parts: HashMap<(String, u32), Vec<u8>>,
    tombstones: HashMap<String, u64>,
    mailboxes: HashMap<String, MailboxRecord>,
    mailbox_messages: HashMap<String, Vec<MailboxMessageRecord>>,
    next_msg_id: HashMap<String, u64>,
}

/// In-memory storage implementation for testing.
#[derive(Clone, Default)]
pub struct MemoryStorage {
    inner: Arc<RwLock<InnerMemoryStorage>>,
}

impl MemoryStorage {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl Storage for MemoryStorage {
    async fn create_blob(&self, record: &BlobRecord) -> Result<(), ServerError> {
        let mut inner = self.inner.write().await;
        inner.blobs.insert(record.id.clone(), record.clone());
        Ok(())
    }

    async fn get_blob(&self, id: &str) -> Result<Option<BlobRecord>, ServerError> {
        let inner = self.inner.read().await;
        Ok(inner.blobs.get(id).cloned())
    }

    async fn update_blob(&self, record: &BlobRecord) -> Result<(), ServerError> {
        let mut inner = self.inner.write().await;
        inner.blobs.insert(record.id.clone(), record.clone());
        Ok(())
    }

    async fn delete_blob(&self, id: &str) -> Result<(), ServerError> {
        let mut inner = self.inner.write().await;
        inner.blobs.remove(id);
        inner.parts.retain(|(blob_id, _), _| blob_id != id);
        Ok(())
    }

    async fn write_part(&self, id: &str, part_index: u32, data: &[u8]) -> Result<(), ServerError> {
        let mut inner = self.inner.write().await;
        inner
            .parts
            .insert((id.to_string(), part_index), data.to_vec());
        Ok(())
    }

    async fn read_part(&self, id: &str, part_index: u32) -> Result<Option<Vec<u8>>, ServerError> {
        let inner = self.inner.read().await;
        Ok(inner.parts.get(&(id.to_string(), part_index)).cloned())
    }

    async fn part_exists(&self, id: &str, part_index: u32) -> Result<bool, ServerError> {
        let inner = self.inner.read().await;
        Ok(inner.parts.contains_key(&(id.to_string(), part_index)))
    }

    async fn get_part_size(&self, id: &str, part_index: u32) -> Result<Option<u64>, ServerError> {
        let inner = self.inner.read().await;
        Ok(inner
            .parts
            .get(&(id.to_string(), part_index))
            .map(|p| p.len() as u64))
    }

    async fn list_blobs(&self) -> Result<Vec<BlobRecord>, ServerError> {
        let inner = self.inner.read().await;
        Ok(inner.blobs.values().cloned().collect())
    }

    async fn record_tombstone(&self, id: &str, expires_at: u64) -> Result<(), ServerError> {
        let mut inner = self.inner.write().await;
        inner.tombstones.insert(id.to_string(), expires_at);
        Ok(())
    }

    async fn is_tombstone(&self, id: &str) -> Result<bool, ServerError> {
        let inner = self.inner.read().await;
        Ok(inner.tombstones.contains_key(id))
    }

    async fn sweep_tombstones(&self, now: u64) -> Result<usize, ServerError> {
        let mut inner = self.inner.write().await;
        let before = inner.tombstones.len();
        inner.tombstones.retain(|_, &mut exp| exp > now);
        Ok(before - inner.tombstones.len())
    }

    async fn create_mailbox(&self, record: &MailboxRecord) -> Result<(), ServerError> {
        let mut inner = self.inner.write().await;
        inner.mailboxes.insert(record.id.clone(), record.clone());
        Ok(())
    }

    async fn get_mailbox(&self, id: &str) -> Result<Option<MailboxRecord>, ServerError> {
        let inner = self.inner.read().await;
        Ok(inner.mailboxes.get(id).cloned())
    }

    async fn delete_mailbox(&self, id: &str) -> Result<(), ServerError> {
        let mut inner = self.inner.write().await;
        inner.mailboxes.remove(id);
        inner.mailbox_messages.remove(id);
        inner.next_msg_id.remove(id);
        Ok(())
    }

    async fn push_mailbox_message(
        &self,
        id: &str,
        payload: Vec<u8>,
        now: u64,
    ) -> Result<u64, ServerError> {
        let mut inner = self.inner.write().await;
        if !inner.mailboxes.contains_key(id) {
            return Err(ServerError::NotFound);
        }

        let queue = inner.mailbox_messages.entry(id.to_string()).or_default();
        if queue.len() >= MAX_MAILBOX_MESSAGES {
            return Err(ServerError::Validation("Mailbox message queue is full".into()));
        }

        let entry = inner.next_msg_id.entry(id.to_string()).or_insert(0);
        *entry += 1;
        let msg_id = *entry;

        queue.push(MailboxMessageRecord {
            id: msg_id,
            payload,
            created_at: now,
        });

        Ok(msg_id)
    }

    async fn get_mailbox_messages(
        &self,
        id: &str,
        after_id: Option<u64>,
        drain: bool,
    ) -> Result<Vec<MailboxMessageRecord>, ServerError> {
        let mut inner = self.inner.write().await;
        if !inner.mailboxes.contains_key(id) {
            return Err(ServerError::NotFound);
        }

        let threshold = after_id.unwrap_or(0);
        let queue = inner.mailbox_messages.entry(id.to_string()).or_default();

        let matched: Vec<MailboxMessageRecord> = queue
            .iter()
            .filter(|m| m.id > threshold)
            .cloned()
            .collect();

        if drain && !matched.is_empty() {
            let matched_ids: std::collections::HashSet<u64> =
                matched.iter().map(|m| m.id).collect();
            queue.retain(|m| !matched_ids.contains(&m.id));
        }

        Ok(matched)
    }

    async fn list_mailboxes(&self) -> Result<Vec<MailboxRecord>, ServerError> {
        let inner = self.inner.read().await;
        Ok(inner.mailboxes.values().cloned().collect())
    }
}
