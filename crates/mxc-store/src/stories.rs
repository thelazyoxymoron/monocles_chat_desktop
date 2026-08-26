//! Social-feed Stories: ephemeral 24h media posts from contacts, cached locally.

use crate::{Result, Store};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StoryRow {
    pub uuid: String,
    /// Publisher bare JID.
    pub contact: String,
    pub url: String,
    pub r#type: String,
    pub title: Option<String>,
    /// Unix seconds.
    pub published: i64,
}

/// 24 hours, matching the server-side `pubsub#item_expire`.
const STORY_TTL_SECS: i64 = 86_400;

impl Store {
    /// Upsert a story (dedup on its uuid).
    pub async fn upsert_story(
        &self,
        account_id: i64,
        uuid: &str,
        contact: &str,
        url: &str,
        type_: &str,
        title: Option<&str>,
        published: i64,
    ) -> Result<()> {
        sqlx::query!(
            r#"INSERT INTO stories (uuid, account_id, contact, url, type, title, published)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
               ON CONFLICT(uuid) DO UPDATE SET
                 url = excluded.url, type = excluded.type, title = excluded.title,
                 published = excluded.published"#,
            uuid,
            account_id,
            contact,
            url,
            type_,
            title,
            published,
        )
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// All non-expired stories, newest first.
    pub async fn recent_stories(&self, account_id: i64, now: i64) -> Result<Vec<StoryRow>> {
        let cutoff = now - STORY_TTL_SECS;
        let rows = sqlx::query_as::<_, StoryRow>(
            r#"SELECT uuid, contact, url, type, title, published
               FROM stories WHERE account_id = ?1 AND published >= ?2
               ORDER BY published DESC"#,
        )
        .bind(account_id)
        .bind(cutoff)
        .fetch_all(self.pool())
        .await?;
        Ok(rows)
    }

    /// Drop expired stories (older than 24h).
    pub async fn expire_stories(&self, now: i64) -> Result<()> {
        let cutoff = now - STORY_TTL_SECS;
        sqlx::query!("DELETE FROM stories WHERE published < ?1", cutoff)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Remove one story (e.g. on retract).
    pub async fn delete_story(&self, uuid: &str) -> Result<()> {
        sqlx::query!("DELETE FROM stories WHERE uuid = ?1", uuid)
            .execute(self.pool())
            .await?;
        Ok(())
    }
}
