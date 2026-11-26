use sqlx::PgConnection;

use crate::mail::service::MailRequest;

/// Enqueue a CMS message for a configured target, using the caller's transaction.
/// The key is unique among CMS messages and makes submissions idempotent.
pub async fn enqueue(
    connection: &mut PgConnection,
    key: &str,
    target: &str,
    message: &MailRequest,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO public.mail_queue (job_key, target, sender_name, reply_to, subject, body)
         VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT (source, job_key) DO NOTHING",
    )
    .bind(key)
    .bind(target)
    .bind(&message.name)
    .bind(&message.reply_to)
    .bind(&message.subject)
    .bind(&message.text)
    .execute(connection)
    .await?;

    Ok(result.rows_affected() == 1)
}
