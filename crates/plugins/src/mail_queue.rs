use std::time::Duration;

use sqlx::{PgPool, Row, postgres::PgRow};
use tokio::{task::JoinHandle, time};
use tracing::warn;
use uuid::Uuid;

use nur_core::config::settings;
use nur_core::mail::service::{
    MailRequest, PluginMailContentKind, PluginMailError, deliver_plugin_mail_with_attachments,
    prepare_plugin_mail,
};

use crate::runtime::{
    bindings::nur::cms::mail_attachments::{Attachment, AttachmentType},
    host::mail::attachments,
};

pub(crate) struct Worker(JoinHandle<()>);

impl Drop for Worker {
    fn drop(&mut self) {
        self.0.abort();
    }
}

impl Worker {
    pub(crate) fn start(pool: PgPool, targets: Vec<(String, Vec<String>)>) -> Self {
        let mut sources = vec!["cms".to_owned()];
        sources.extend(targets.iter().map(|(source, _)| source.clone()));
        let interval = settings().plugins.mail_queue.interval_seconds;

        Self(tokio::spawn(async move {
            loop {
                match tick(&pool, &sources, &targets, interval).await {
                    Ok(true) => continue,
                    Ok(false) => {}
                    Err(error) => warn!(%error, "mail queue database error"),
                }
                time::sleep(Duration::from_secs(5)).await;
            }
        }))
    }
}

async fn tick(
    pool: &PgPool,
    sources: &[String],
    targets: &[(String, Vec<String>)],
    interval_seconds: u64,
) -> Result<bool, sqlx::Error> {
    let Some(row) = claim(pool, sources, interval_seconds).await? else {
        return Ok(false);
    };
    let source: String = row.try_get("source")?;
    let key: String = row.try_get("job_key")?;
    let attempt: i32 = row.try_get("attempts")?;
    let claim_token: String = row.try_get("lease_token")?;
    let permitted = targets
        .iter()
        .find(|(id, _)| *id == source)
        .map(|(_, targets)| targets.as_slice());

    let result = time::timeout(Duration::from_secs(60), deliver(pool, permitted, row)).await;
    let state = match result {
        Ok(Ok(())) => "sent",
        Ok(Err(
            PluginMailError::InvalidMessage
            | PluginMailError::Spam
            | PluginMailError::DynamicRecipientNotAllowed,
        )) => "failed",
        _ if attempt >= 10 => "failed",
        _ => "pending",
    };
    if state != "sent" {
        warn!(%source, attempt, state, "plugin mail queue delivery unsuccessful");
    }
    finish(pool, &source, &key, &claim_token, attempt, state).await?;

    Ok(true)
}

async fn claim(
    pool: &PgPool,
    sources: &[String],
    interval_seconds: u64,
) -> Result<Option<PgRow>, sqlx::Error> {
    let mut tx = pool.begin().await?;

    // Serialize the interval check across CMS processes and release the lock before SMTP.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('nur-cms-mail-queue', 1))")
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE public.mail_queue SET state = 'failed', claim_token = NULL
         WHERE (source, job_key) IN (
             SELECT source, job_key FROM public.mail_queue
             WHERE source = ANY($1) AND enabled AND state IN ('pending','sending')
               AND attempts >= 10 AND due_at <= statement_timestamp()
             ORDER BY due_at LIMIT 100 FOR UPDATE SKIP LOCKED
         )",
    )
    .bind(sources)
    .execute(&mut *tx)
    .await?;

    let row = sqlx::query(
        "WITH candidate AS (
            SELECT q.source, q.job_key FROM public.mail_queue q
            WHERE q.source = ANY($1) AND q.enabled AND q.state IN ('pending','sending') AND q.due_at <= statement_timestamp() AND q.attempts < 10
              AND (q.source = 'cms' OR NOT EXISTS (
                  SELECT 1 FROM public.mail_queue_source_attempts recent
                  WHERE recent.source = q.source AND recent.last_attempt > statement_timestamp() - make_interval(secs => $2)
              ))
            ORDER BY q.due_at, q.source, q.job_key LIMIT 1 FOR UPDATE SKIP LOCKED
         ), claimed AS (
            UPDATE public.mail_queue q
            SET state = 'sending', attempts = attempts + 1, claim_token = $3::text::uuid,
                last_attempt = statement_timestamp(), due_at = statement_timestamp() + interval '5 minutes'
            FROM candidate WHERE q.source = candidate.source AND q.job_key = candidate.job_key
            RETURNING q.*
         ), spaced AS (
            INSERT INTO public.mail_queue_source_attempts (source, last_attempt)
            SELECT source, last_attempt FROM claimed WHERE source <> 'cms'
            ON CONFLICT (source) DO UPDATE SET last_attempt = EXCLUDED.last_attempt
         )
         SELECT claimed.*, claim_token::text AS lease_token FROM claimed",
    )
    .bind(sources)
    .bind(interval_seconds as f64)
    .bind(Uuid::new_v4().to_string())
    .fetch_optional(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(row)
}

async fn finish(
    pool: &PgPool,
    source: &str,
    key: &str,
    claim_token: &str,
    attempt: i32,
    state: &str,
) -> Result<(), sqlx::Error> {
    let delay = 60_i32 * (1 << attempt.clamp(0, 6));

    sqlx::query(
        "UPDATE public.mail_queue SET state=$1, claim_token=NULL, due_at=now()+make_interval(secs => $2),
        attachment=CASE WHEN $1='sent' THEN NULL ELSE attachment END,
        filename=CASE WHEN $1='sent' THEN NULL ELSE filename END,
        attachment_kind=CASE WHEN $1='sent' THEN NULL ELSE attachment_kind END
        WHERE source=$3 AND job_key=$4 AND claim_token=$5::text::uuid AND state='sending'",
    )
    .bind(state)
    .bind(f64::from(delay))
    .bind(source)
    .bind(key)
    .bind(claim_token)
    .execute(pool)
    .await?;

    Ok(())
}

async fn deliver(
    pool: &PgPool,
    targets: Option<&[String]>,
    row: PgRow,
) -> Result<(), PluginMailError> {
    let target: String = row
        .try_get("target")
        .map_err(|_| PluginMailError::InvalidMessage)?;
    if targets.is_some_and(|allowed| !allowed.contains(&target)) {
        return Err(PluginMailError::InvalidMessage);
    }
    let text = |column: &str| {
        row.try_get::<String, _>(column)
            .map_err(|_| PluginMailError::InvalidMessage)
    };
    let message = MailRequest {
        reply_to: text("reply_to")?,
        name: text("sender_name")?,
        subject: row
            .try_get("subject")
            .map_err(|_| PluginMailError::InvalidMessage)?,
        text: text("body")?,
    };
    let mut files = Vec::new();
    if let Some(data) = row
        .try_get::<Option<Vec<u8>>, _>("attachment")
        .map_err(|_| PluginMailError::InvalidMessage)?
    {
        let kind = match text("attachment_kind")?.as_str() {
            "pdf" => AttachmentType::Pdf,
            "png" => AttachmentType::Png,
            "jpeg" => AttachmentType::Jpeg,
            "text" => AttachmentType::PlainText,
            "csv" => AttachmentType::Csv,
            "json" => AttachmentType::Json,
            _ => return Err(PluginMailError::InvalidMessage),
        };
        files.push(Attachment {
            filename: text("filename")?,
            kind,
            data,
        });
    }
    let files = attachments::validate(files).map_err(|_| PluginMailError::InvalidMessage)?;
    let prepared = prepare_plugin_mail(
        pool,
        &target,
        None,
        PluginMailContentKind::TrustedTemplateHtml,
        message,
    )
    .await?;
    deliver_plugin_mail_with_attachments(prepared, files).await
}

#[cfg(test)]
mod tests {
    use super::*;

    static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../core/migrations");

    #[sqlx::test(migrator = "MIGRATOR")]
    #[ignore = "requires PostgreSQL via DATABASE_URL"]
    async fn recovers_leases_and_fences_obsolete_results(pool: PgPool) {
        let scope = "nur_plugin_test";
        let sources = [scope.to_owned()];

        let message = MailRequest {
            reply_to: "test@example.org".into(),
            name: "CMS Test".into(),
            subject: None,
            text: "A queued CMS message.".into(),
        };
        let mut transaction = pool.begin().await.unwrap();
        let queued = nur_core::mail::queue::enqueue(&mut transaction, "a", "orders", &message)
            .await
            .unwrap();
        let duplicate = nur_core::mail::queue::enqueue(&mut transaction, "a", "orders", &message)
            .await
            .unwrap();

        transaction.commit().await.unwrap();

        assert!(queued);
        assert!(!duplicate);

        let cms = claim(&pool, &["cms".to_owned()], 60)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(cms.get::<String, _>("source"), "cms");

        finish(
            &pool,
            "cms",
            "a",
            &cms.get::<String, _>("lease_token"),
            cms.get("attempts"),
            "sent",
        )
        .await
        .unwrap();

        let mut connection = pool.acquire().await.unwrap();
        sqlx::query("CREATE SCHEMA nur_plugin_test")
            .execute(&mut *connection)
            .await
            .unwrap();
        crate::db::migrations::expose_mail_outbox(&mut connection, scope)
            .await
            .unwrap();
        sqlx::query("INSERT INTO nur_plugin_test.plugin_mail_outbox(job_key,target,sender_name,reply_to,body,filename,attachment_kind,attachment) VALUES ('a','orders','Test','test@example.org','body','signed.pdf','pdf',$1)")
            .bind(b"%PDF-1.4\n%%EOF".as_slice()).execute(&mut *connection).await.unwrap();
        assert!(sqlx::query("UPDATE nur_plugin_test.plugin_mail_outbox SET attempts=0, state='pending', last_attempt=NULL")
            .execute(&mut *connection).await.is_err());
        drop(connection);
        let first = claim(&pool, &sources, 60).await.unwrap().unwrap();
        assert_eq!(first.get::<i32, _>("attempts"), 1);
        let first_token: String = first.get("lease_token");
        assert!(claim(&pool, &sources, 0).await.unwrap().is_none());
        assert!(matches!(
            deliver(&pool, Some(&[]), first).await,
            Err(PluginMailError::InvalidMessage)
        ));

        sqlx::query("UPDATE public.mail_queue SET due_at=now()-interval '1 second' WHERE source='nur_plugin_test'")
            .execute(&pool)
            .await
            .unwrap();
        assert!(claim(&pool, &sources, 60).await.unwrap().is_none());
        let second = claim(&pool, &sources, 0).await.unwrap().unwrap();
        assert_eq!(second.get::<i32, _>("attempts"), 2);
        finish(&pool, scope, "a", &first_token, 1, "sent")
            .await
            .unwrap();
        finish(
            &pool,
            scope,
            "a",
            &second.get::<String, _>("lease_token"),
            2,
            "pending",
        )
        .await
        .unwrap();
        let retained: bool = sqlx::query_scalar("SELECT state='pending' AND attachment IS NOT NULL AND due_at>now() FROM public.mail_queue WHERE source='nur_plugin_test'").fetch_one(&pool).await.unwrap();
        assert!(retained);

        sqlx::query("UPDATE public.mail_queue SET due_at=now() WHERE source='nur_plugin_test'")
            .execute(&pool)
            .await
            .unwrap();
        let third = claim(&pool, &sources, 0).await.unwrap().unwrap();
        finish(
            &pool,
            scope,
            "a",
            &third.get::<String, _>("lease_token"),
            third.get("attempts"),
            "sent",
        )
        .await
        .unwrap();
        let sent: bool =
            sqlx::query_scalar("SELECT state='sent' AND attachment IS NULL FROM public.mail_queue WHERE source='nur_plugin_test'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(sent);
        assert!(claim(&pool, &sources, 0).await.unwrap().is_none());

        // Reset and reclaim with the same attempt number. The earlier lease
        // must still be unable to complete the new attempt.
        sqlx::query("UPDATE public.mail_queue SET state='pending', attempts=0, due_at=now() WHERE source=$1")
            .bind(scope).execute(&pool).await.unwrap();
        let reset = claim(&pool, &sources, 0).await.unwrap().unwrap();
        assert_eq!(reset.get::<i32, _>("attempts"), 1);
        finish(&pool, scope, "a", &first_token, 1, "sent")
            .await
            .unwrap();
        let active: bool =
            sqlx::query_scalar("SELECT state='sending' FROM public.mail_queue WHERE source=$1")
                .bind(scope)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(active);

        // The tenth attempt crashed. Its expired lease goes straight to failed.
        sqlx::query("UPDATE public.mail_queue SET attempts=10, due_at=now()-interval '1 second' WHERE source=$1")
            .bind(scope).execute(&pool).await.unwrap();
        assert!(claim(&pool, &sources, 0).await.unwrap().is_none());
        let failed: bool = sqlx::query_scalar("SELECT state='failed' AND attempts=10 AND claim_token IS NULL FROM public.mail_queue WHERE source=$1")
            .bind(scope).fetch_one(&pool).await.unwrap();
        assert!(failed);
    }
}
