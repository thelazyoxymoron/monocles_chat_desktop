//! Roster items + presence cache.

use crate::{Result, Store};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RosterItem {
    pub jid: String,
    pub name: Option<String>,
    pub subscription: String,
    pub ask: Option<String>,
    /// JSON-encoded array of group names.
    pub groups: Option<String>,
}

impl Store {
    pub async fn replace_roster_item(
        &self,
        account_id: i64,
        item: &RosterItem,
    ) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO roster (account_id, jid, name, subscription, ask, groups)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(account_id, jid) DO UPDATE SET
                name = excluded.name,
                subscription = excluded.subscription,
                ask = excluded.ask,
                groups = excluded.groups
            "#,
            account_id,
            item.jid,
            item.name,
            item.subscription,
            item.ask,
            item.groups,
        )
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn remove_roster_item(&self, account_id: i64, jid: &str) -> Result<()> {
        sqlx::query!(
            "DELETE FROM roster WHERE account_id = ?1 AND jid = ?2",
            account_id,
            jid
        )
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// A single roster item by bare JID, if present.
    pub async fn roster_item(&self, account_id: i64, jid: &str) -> Result<Option<RosterItem>> {
        let row = sqlx::query_as::<_, RosterItem>(
            r#"SELECT jid, name, subscription, ask, groups
               FROM roster WHERE account_id = ?1 AND jid = ?2"#,
        )
        .bind(account_id)
        .bind(jid)
        .fetch_optional(self.pool())
        .await?;
        Ok(row)
    }

    pub async fn roster(&self, account_id: i64) -> Result<Vec<RosterItem>> {
        let rows = sqlx::query_as::<_, RosterItem>(
            r#"SELECT jid, name, subscription, ask, groups
               FROM roster WHERE account_id = ?1 ORDER BY COALESCE(name, jid)"#,
        )
        .bind(account_id)
        .fetch_all(self.pool())
        .await?;
        Ok(rows)
    }

    /// Update the cached presence for a full JID.
    pub async fn set_presence(
        &self,
        account_id: i64,
        full_jid: &str,
        show: Option<&str>,
        status: Option<&str>,
        priority: i64,
        caps_hash: Option<&str>,
    ) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO presence (account_id, full_jid, show, status, priority, caps_hash, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'))
            ON CONFLICT(account_id, full_jid) DO UPDATE SET
                show = excluded.show,
                status = excluded.status,
                priority = excluded.priority,
                caps_hash = excluded.caps_hash,
                updated_at = excluded.updated_at
            "#,
            account_id, full_jid, show, status, priority, caps_hash,
        )
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn clear_presence(&self, account_id: i64, full_jid: &str) -> Result<()> {
        sqlx::query!(
            "DELETE FROM presence WHERE account_id = ?1 AND full_jid = ?2",
            account_id,
            full_jid
        )
        .execute(self.pool())
        .await?;
        Ok(())
    }
}
