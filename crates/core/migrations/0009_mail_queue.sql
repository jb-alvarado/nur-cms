CREATE TABLE public.mail_queue (
    source text NOT NULL DEFAULT 'cms',
    job_key text NOT NULL CHECK (length(job_key) BETWEEN 1 AND 160),
    target text NOT NULL CHECK (octet_length(target) <= 255),
    sender_name text NOT NULL CHECK (octet_length(sender_name) <= 1024),
    reply_to text NOT NULL CHECK (octet_length(reply_to) <= 1024),
    subject text CHECK (octet_length(subject) <= 1024),
    body text NOT NULL CHECK (octet_length(body) <= 65536),
    filename text CHECK (octet_length(filename) <= 640),
    attachment_kind text CHECK (attachment_kind IN ('pdf', 'png', 'jpeg', 'text', 'csv', 'json')),
    attachment bytea CHECK (octet_length(attachment) <= 1048576),
    enabled boolean NOT NULL DEFAULT TRUE,
    state text NOT NULL DEFAULT 'pending' CHECK (state IN ('pending', 'sending', 'sent', 'failed')),
    attempts integer NOT NULL DEFAULT 0 CHECK (attempts BETWEEN 0 AND 10),
    claim_token uuid,
    last_attempt timestamptz,
    due_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (source, job_key),
    CHECK (
        (
            attachment IS NULL
            AND filename IS NULL
            AND attachment_kind IS NULL
        )
        OR (
            attachment IS NOT NULL
            AND filename IS NOT NULL
            AND attachment_kind IS NOT NULL
        )
    )
);

CREATE INDEX mail_queue_due ON public.mail_queue (due_at, source, job_key)
WHERE
    enabled
    AND state IN ('pending', 'sending');

CREATE TABLE public.mail_queue_source_attempts (source text PRIMARY KEY, last_attempt timestamptz NOT NULL);
