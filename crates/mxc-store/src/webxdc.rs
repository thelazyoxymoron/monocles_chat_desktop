//! WebXDC (`urn:xmpp:webxdc:0`) status-update store.
//!
//! Each `.xdc` app instance is keyed by its `<thread>` UUID. Status updates are appended with a
//! monotonic `serial`; the in-app JS API (`getStatusUpdates(lastKnownSerial)`) pages forward from
//! a cursor, so we only ever query "updates with serial > N for this thread".

use crate::{Result, Store};

/// One WebXDC status update, as the JS API consumes it.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WebxdcUpdateRow {
    pub serial: i64,
    pub sender: Option<String>,
    pub info: Option<String>,
    pub document: Option<String>,
    pub summary: Option<String>,
    pub payload: Option<String>,
}

impl Store {
    /// Append a WebXDC status update for `thread`, returning its new serial.
    #[allow(clippy::too_many_arguments)]
    pub async fn insert_webxdc_update(
        &self,
        account_id: i64,
        thread: &str,
        message_id: Option<&str>,
        sender: Option<&str>,
        info: Option<&str>,
        document: Option<&str>,
        summary: Option<&str>,
        payload: Option<&str>,
    ) -> Result<i64> {
        // Dedup by message id: our own update is inserted locally on send and would otherwise be
        // inserted again when the server reflects it back (carbon / MUC echo).
        if let Some(mid) = message_id {
            if let Some(existing) = sqlx::query!(
                r#"SELECT serial as "serial!: i64" FROM webxdc_updates
                   WHERE account_id = ?1 AND thread = ?2 AND message_id = ?3 LIMIT 1"#,
                account_id,
                thread,
                mid,
            )
            .fetch_optional(self.pool())
            .await?
            {
                return Ok(existing.serial);
            }
        }
        let now = chrono::Utc::now().to_rfc3339();
        let rec = sqlx::query!(
            r#"
            INSERT INTO webxdc_updates
              (account_id, thread, message_id, sender, info, document, summary, payload, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            RETURNING serial as "serial!: i64"
            "#,
            account_id,
            thread,
            message_id,
            sender,
            info,
            document,
            summary,
            payload,
            now,
        )
        .fetch_one(self.pool())
        .await?;
        Ok(rec.serial)
    }

    /// All status updates for `thread` with `serial > after_serial`, oldest first.
    pub async fn webxdc_updates_since(
        &self,
        account_id: i64,
        thread: &str,
        after_serial: i64,
    ) -> Result<Vec<WebxdcUpdateRow>> {
        let rows = sqlx::query_as::<_, WebxdcUpdateRow>(
            r#"SELECT serial, sender, info, document, summary, payload
               FROM webxdc_updates
               WHERE account_id = ?1 AND thread = ?2 AND serial > ?3
               ORDER BY serial ASC"#,
        )
        .bind(account_id)
        .bind(thread)
        .bind(after_serial)
        .fetch_all(self.pool())
        .await?;
        Ok(rows)
    }

    /// The highest serial stored for `thread` (0 if none) — the `max_serial` the JS API reports.
    pub async fn webxdc_max_serial(&self, account_id: i64, thread: &str) -> Result<i64> {
        let rec = sqlx::query!(
            r#"SELECT COALESCE(MAX(serial), 0) as "max!: i64"
               FROM webxdc_updates WHERE account_id = ?1 AND thread = ?2"#,
            account_id,
            thread,
        )
        .fetch_one(self.pool())
        .await?;
        Ok(rec.max)
    }
}
