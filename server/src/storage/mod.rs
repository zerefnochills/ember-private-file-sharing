//! Storage trait and backends for Ember relay server.

pub mod fs;
pub mod mem;

use async_trait::async_trait;
use crate::error::ServerError;
use crate::model::BlobRecord;

/// Abstract storage interface for blob records and opaque ciphertext parts.
#[async_trait]
pub trait Storage: Send + Sync + 'static {
    /// Inserts a new blob metadata record.
    async fn create_blob(&self, record: &BlobRecord) -> Result<(), ServerError>;

    /// Retrieves a blob metadata record by ID.
    async fn get_blob(&self, id: &str) -> Result<Option<BlobRecord>, ServerError>;

    /// Updates an existing blob metadata record.
    async fn update_blob(&self, record: &BlobRecord) -> Result<(), ServerError>;

    /// Completely removes a blob and all of its uploaded parts.
    async fn delete_blob(&self, id: &str) -> Result<(), ServerError>;

    /// Writes raw ciphertext bytes for part `part_index`.
    async fn write_part(&self, id: &str, part_index: u32, data: &[u8]) -> Result<(), ServerError>;

    /// Reads raw ciphertext bytes for part `part_index`.
    async fn read_part(&self, id: &str, part_index: u32) -> Result<Option<Vec<u8>>, ServerError>;

    /// Checks if part `part_index` has been uploaded.
    async fn part_exists(&self, id: &str, part_index: u32) -> Result<bool, ServerError>;

    /// Gets the size in bytes of an uploaded part.
    async fn get_part_size(&self, id: &str, part_index: u32) -> Result<Option<u64>, ServerError>;

    /// Lists all active blob records (for sweeper).
    async fn list_blobs(&self) -> Result<Vec<BlobRecord>, ServerError>;

    /// Records an expired or burned blob ID as a tombstone so subsequent requests return 410 Gone.
    async fn record_tombstone(&self, id: &str, expires_at: u64) -> Result<(), ServerError>;

    /// Checks if a blob ID is a tombstone (burned or expired).
    async fn is_tombstone(&self, id: &str) -> Result<bool, ServerError>;

    /// Cleans up tombstones that have passed their retention period.
    async fn sweep_tombstones(&self, now: u64) -> Result<usize, ServerError>;
}
