use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use ts_rs::TS;

#[derive(Debug, Clone, FromRow, Serialize, TS)]
#[ts(export, export_to = "subtitles.d.ts")]
pub struct MediaSubtitle {
    pub id: String,
    pub language: String,
    pub label: String,
    pub is_default: bool,
    #[serde(skip_serializing)]
    #[ts(skip)]
    pub webvtt: String,
}

#[derive(Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, export_to = "subtitles.d.ts")]
pub struct SubtitleUpload {
    pub language: String,
    pub label: String,
    pub is_default: bool,
    pub webvtt: String,
}

#[derive(FromRow)]
pub struct SubtitleTarget {
    pub path: String,
    pub filename: String,
    pub processing_status: String,
}

// A transaction-scoped lock coordinates filesystem publication, rename and deletion
// across processes without holding an in-memory mutex across awaits.
pub async fn lock_media_files(
    pool: &PgPool,
    media_id: i32,
) -> Result<Transaction<'_, Postgres>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    lock_media_files_on(&mut tx, media_id).await?;
    Ok(tx)
}

pub async fn lock_media_files_on(
    tx: &mut Transaction<'_, Postgres>,
    media_id: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT pg_advisory_xact_lock(78321, $1)")
        .bind(media_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub async fn subtitle_target(
    tx: &mut Transaction<'_, Postgres>,
    media_id: i32,
) -> Result<Option<SubtitleTarget>, sqlx::Error> {
    sqlx::query_as("SELECT m.path, v.filename, m.processing_status FROM media m JOIN media_video_variants v ON v.media_id = m.id AND v.kind = 'hls' WHERE m.id = $1")
        .bind(media_id).fetch_optional(&mut **tx).await
}

pub async fn select_subtitles_on(
    tx: &mut Transaction<'_, Postgres>,
    media_id: i32,
) -> Result<Vec<MediaSubtitle>, sqlx::Error> {
    sqlx::query_as("SELECT id::text, language, label, is_default, webvtt FROM media_subtitles WHERE media_id = $1 ORDER BY language")
        .bind(media_id).fetch_all(&mut **tx).await
}

pub async fn save_subtitle(
    tx: &mut Transaction<'_, Postgres>,
    media_id: i32,
    input: &SubtitleUpload,
) -> Result<(), sqlx::Error> {
    if input.is_default {
        sqlx::query("UPDATE media_subtitles SET is_default = FALSE WHERE media_id = $1")
            .bind(media_id)
            .execute(&mut **tx)
            .await?;
    }
    sqlx::query("INSERT INTO media_subtitles (id, media_id, language, label, is_default, webvtt) VALUES ($1::text::uuid, $2, $3, $4, $5, $6) ON CONFLICT (media_id, language) DO UPDATE SET id = EXCLUDED.id, label = EXCLUDED.label, is_default = EXCLUDED.is_default, webvtt = EXCLUDED.webvtt")
        .bind(uuid::Uuid::new_v4().to_string()).bind(media_id).bind(&input.language)
        .bind(&input.label).bind(input.is_default).bind(&input.webvtt).execute(&mut **tx).await?;
    Ok(())
}

pub async fn delete_subtitle(
    tx: &mut Transaction<'_, Postgres>,
    media_id: i32,
    id: &str,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM media_subtitles WHERE media_id = $1 AND id::text = $2")
        .bind(media_id)
        .bind(id)
        .execute(&mut **tx)
        .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn media_has_active_video_job(
    tx: &mut Transaction<'_, Postgres>,
    media_id: i32,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM media_processing_jobs WHERE media_id = $1 AND status IN ('queued', 'running'))")
        .bind(media_id).fetch_one(&mut **tx).await
}

pub async fn rotate_subtitle_revisions(
    tx: &mut Transaction<'_, Postgres>,
    media_id: i32,
) -> Result<Vec<MediaSubtitle>, sqlx::Error> {
    let mut tracks = select_subtitles_on(tx, media_id).await?;
    for track in &mut tracks {
        let revision = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "UPDATE media_subtitles SET id = $1::text::uuid WHERE id::text = $2 AND media_id = $3",
        )
        .bind(&revision)
        .bind(&track.id)
        .bind(media_id)
        .execute(&mut **tx)
        .await?;
        track.id = revision;
    }
    Ok(tracks)
}

pub async fn subtitle_thumbnail_filenames(
    tx: &mut Transaction<'_, Postgres>,
    media_id: i32,
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT DISTINCT filename FROM media_variants WHERE media_id = $1")
        .bind(media_id)
        .fetch_all(&mut **tx)
        .await
}

pub async fn update_hls_size(
    tx: &mut Transaction<'_, Postgres>,
    media_id: i32,
    bytes: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE media_video_variants SET size = $1 WHERE media_id = $2 AND kind = 'hls'")
        .bind(bytes)
        .bind(media_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

// Session scope keeps the same media lock across both publication transactions.
// Close rather than recycle this connection, including on cancellation, so a
// session advisory lock can never leak into a different pool borrower.
pub async fn lock_media_publication(
    pool: &PgPool,
    media_id: i32,
) -> Result<sqlx::pool::PoolConnection<Postgres>, sqlx::Error> {
    let mut connection = pool.acquire().await?;
    connection.close_on_drop();
    sqlx::query("SELECT pg_advisory_lock(78321, $1)")
        .bind(media_id)
        .execute(&mut *connection)
        .await?;
    Ok(connection)
}

#[derive(FromRow)]
pub struct SubtitlePublication {
    pub previous_master: String,
    pub master: String,
    pub backup_filename: String,
}

pub async fn save_subtitle_publication(
    tx: &mut Transaction<'_, Postgres>,
    media_id: i32,
    previous_master: &str,
    master: &str,
    backup_filename: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO media_subtitle_publications (media_id, previous_master, master, backup_filename) VALUES ($1, $2, $3, $4)")
        .bind(media_id).bind(previous_master).bind(master).bind(backup_filename).execute(&mut **tx).await?;
    Ok(())
}

pub async fn pending_subtitle_publication(
    tx: &mut Transaction<'_, Postgres>,
    media_id: i32,
) -> Result<Option<SubtitlePublication>, sqlx::Error> {
    sqlx::query_as("SELECT previous_master, master, backup_filename FROM media_subtitle_publications WHERE media_id = $1")
        .bind(media_id).fetch_optional(&mut **tx).await
}

pub async fn finish_subtitle_publication(
    tx: &mut Transaction<'_, Postgres>,
    media_id: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM media_subtitle_publications WHERE media_id = $1")
        .bind(media_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub async fn pending_subtitle_media(pool: &PgPool) -> Result<Vec<i32>, sqlx::Error> {
    sqlx::query_scalar("SELECT media_id FROM media_subtitle_publications ORDER BY media_id")
        .fetch_all(pool)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

    #[sqlx::test(migrator = "MIGRATOR")]
    async fn subtitle_revisions_defaults_rollback_and_cascade(pool: PgPool) {
        let id: i32 = sqlx::query_scalar("INSERT INTO media (filename, path, type) VALUES ('clip.mp4', '/uploads', 'video/mp4') RETURNING id")
            .fetch_one(&pool).await.expect("media");
        let mut tx = lock_media_files(&pool, id).await.expect("lock");
        let mut input = SubtitleUpload {
            language: "de".into(),
            label: "Deutsch".into(),
            is_default: true,
            webvtt: "WEBVTT\n\n00:00:00.000 --> 00:00:01.000\nHallo\n".into(),
        };
        save_subtitle(&mut tx, id, &input)
            .await
            .expect("German track");
        let first = select_subtitles_on(&mut tx, id).await.expect("tracks");
        save_subtitle(&mut tx, id, &input).await.expect("replace");
        let replaced = select_subtitles_on(&mut tx, id).await.expect("tracks");
        assert_eq!(replaced.len(), 1);
        assert_ne!(first[0].id, replaced[0].id);
        input.language = "en".into();
        input.label = "English".into();
        save_subtitle(&mut tx, id, &input)
            .await
            .expect("English track");
        let tracks = select_subtitles_on(&mut tx, id).await.expect("tracks");
        assert_eq!(tracks.iter().filter(|t| t.is_default).count(), 1);
        assert!(tracks.iter().any(|t| t.language == "en" && t.is_default));
        tx.commit().await.expect("commit");
        let mut tx = lock_media_files(&pool, id).await.expect("lock");
        assert!(
            !delete_subtitle(&mut tx, id + 1, &tracks[0].id)
                .await
                .expect("wrong media")
        );
        assert!(
            delete_subtitle(&mut tx, id, &tracks[0].id)
                .await
                .expect("remove")
        );
        tx.rollback().await.expect("rollback");
        let mut tx = pool.begin().await.expect("tx");
        assert_eq!(
            select_subtitles_on(&mut tx, id)
                .await
                .expect("restored")
                .len(),
            2
        );
        tx.rollback().await.expect("rollback");
        sqlx::query("DELETE FROM media WHERE id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .expect("delete media");
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM media_subtitles")
            .fetch_one(&pool)
            .await
            .expect("count");
        assert_eq!(count, 0);
    }
    #[sqlx::test(migrator = "MIGRATOR")]
    async fn publication_intent_commits_atomically_and_lock_survives_commit(pool: PgPool) {
        use sqlx::Connection;

        let id: i32 = sqlx::query_scalar("INSERT INTO media (filename, path, type) VALUES ('clip.mp4', '/uploads', 'video/mp4') RETURNING id")
            .fetch_one(&pool).await.expect("media");
        let mut connection = lock_media_publication(&pool, id)
            .await
            .expect("session lock");
        let mut tx = connection.begin().await.expect("tx");
        save_subtitle_publication(&mut tx, id, "previous", "updated", "backup")
            .await
            .expect("intent");
        tx.rollback().await.expect("rollback");
        assert!(
            pending_subtitle_media(&pool)
                .await
                .expect("pending")
                .is_empty()
        );
        let mut tx = connection.begin().await.expect("tx");
        save_subtitle_publication(&mut tx, id, "previous", "updated", "backup")
            .await
            .expect("intent");
        tx.commit().await.expect("commit");
        assert_eq!(
            pending_subtitle_media(&pool)
                .await
                .expect("durable pending"),
            vec![id]
        );
        let mut other = pool.begin().await.expect("other tx");
        let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_xact_lock(78321, $1)")
            .bind(id)
            .fetch_one(&mut *other)
            .await
            .expect("try lock");
        assert!(!acquired, "commit must not release publication lock");
        other.rollback().await.expect("other rollback");
        let mut tx = connection.begin().await.expect("recovery tx");
        assert_eq!(
            pending_subtitle_publication(&mut tx, id)
                .await
                .expect("intent")
                .expect("present")
                .master,
            "updated"
        );
        finish_subtitle_publication(&mut tx, id)
            .await
            .expect("finish");
        tx.commit().await.expect("finish commit");
        drop(connection);
        let mut tx = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            lock_media_files(&pool, id),
        )
        .await
        .expect("lock released after disconnect")
        .expect("new owner");
        assert!(
            pending_subtitle_publication(&mut tx, id)
                .await
                .expect("pending")
                .is_none()
        );
        tx.rollback().await.expect("cleanup");
    }
}
