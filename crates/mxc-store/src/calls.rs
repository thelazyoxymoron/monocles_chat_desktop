//! Call history (audio/video calls placed and received).

use crate::{Result, Store};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CallLogEntry {
    pub peer: String,
    /// 'in' | 'out'.
    pub direction: String,
    pub video: bool,
    /// Whether the call connected (vs. missed / not answered).
    pub answered: bool,
    pub timestamp: String,
}

impl Store {
    /// Record a finished call.
    pub async fn insert_call_log(
        &self,
        account_id: i64,
        peer: &str,
        direction: &str,
        video: bool,
        answered: bool,
        timestamp: &str,
    ) -> Result<()> {
        sqlx::query!(
            r#"INSERT INTO call_log (account_id, peer, direction, video, answered, timestamp)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6)"#,
            account_id,
            peer,
            direction,
            video,
            answered,
            timestamp,
        )
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Most recent calls, newest first.
    pub async fn recent_calls(&self, account_id: i64, limit: i64) -> Result<Vec<CallLogEntry>> {
        let rows = sqlx::query_as::<_, CallLogEntry>(
            r#"SELECT peer, direction, video, answered, timestamp
               FROM call_log WHERE account_id = ?1
               ORDER BY timestamp DESC LIMIT ?2"#,
        )
        .bind(account_id)
        .bind(limit)
        .fetch_all(self.pool())
        .await?;
        Ok(rows)
    }
}
