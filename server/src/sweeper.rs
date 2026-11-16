//! Background TTL and inactive upload sweeper for Ember relay server.

use std::time::Duration;
use tokio::time::sleep;

use crate::model::BlobState;
use crate::state::{now_secs, AppState};

/// Runs a single sweep iteration across all stored blobs.
pub async fn sweep_once(state: &AppState) -> usize {
    let blobs = match state.storage.list_blobs().await {
        Ok(b) => b,
        Err(_) => return 0,
    };

    let now = now_secs();
    let mut deleted_count = 0;

    for record in blobs {
        let should_delete = now >= record.expires_at
            || (record.burn && record.claim_expires_at.is_some_and(|exp| now >= exp))
            || (record.state == BlobState::Uploading
                && now >= record.created_at + state.upload_timeout_secs);

        if should_delete {
            let tombstone_exp = if now >= record.expires_at {
                record.expires_at + 3600
            } else {
                now + 3600
            };
            let _ = state
                .storage
                .record_tombstone(&record.id, tombstone_exp)
                .await;
            if state.storage.delete_blob(&record.id).await.is_ok() {
                deleted_count += 1;
            }
        }
    }

    if let Ok(mailboxes) = state.storage.list_mailboxes().await {
        for mb in mailboxes {
            if now >= mb.expires_at && state.storage.delete_mailbox(&mb.id).await.is_ok() {
                deleted_count += 1;
            }
        }
    }

    if let Ok(reports) = state.storage.list_reports().await {
        for report in reports {
            if now >= report.expires_at && state.storage.delete_report(&report.id).await.is_ok() {
                deleted_count += 1;
            }
        }
    }

    state.pow.sweep_expired().await;
    state.rate_limiter.sweep_expired().await;
    state.access_logger.prune_expired(now);

    deleted_count
}

/// Spawns a background loop that periodically executes `sweep_once`.
pub fn spawn_sweeper(state: AppState, interval: Duration) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            sleep(interval).await;
            sweep_once(&state).await;
        }
    })
}
