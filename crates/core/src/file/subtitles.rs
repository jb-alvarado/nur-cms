use std::{io::ErrorKind, path::Path, process::Stdio, time::Duration};

use serde::Deserialize;
use sqlx::{PgPool, Postgres, Transaction};
use tokio::{fs, process::Command, time::timeout};
use tracing::warn;
use uuid::Uuid;

use crate::{
    db::handles::{self, MediaSubtitle, SubtitlePublication, SubtitleUpload},
    file::helper::{contained_storage_target, hls_directory_name},
    utils::errors::NurError,
};

pub const MAX_VTT_BYTES: usize = 512 * 1024;

fn invalid(message: &str) -> NurError {
    NurError::BadRequest(message.into())
}

pub fn validate_upload(input: &mut SubtitleUpload) -> Result<(), NurError> {
    input.language = input.language.trim().to_ascii_lowercase();
    input.label = input.label.trim().to_string();
    let mut parts = input.language.split('-');
    let first = parts.next().unwrap_or_default();
    if !(2..=8).contains(&first.len())
        || !first.bytes().all(|b| b.is_ascii_alphabetic())
        || input.language.len() > 35
        || parts.any(|part| {
            part.is_empty() || part.len() > 8 || !part.bytes().all(|b| b.is_ascii_alphanumeric())
        })
    {
        return Err(invalid("Invalid subtitle language tag."));
    }
    if input.label.is_empty()
        || input.label.chars().count() > 100
        || input.label.chars().any(|c| c.is_control() || c == '"')
    {
        return Err(invalid("Invalid subtitle label."));
    }
    input.webvtt = validate_vtt(&input.webvtt)?;
    Ok(())
}

fn timestamp(value: &str) -> Option<u64> {
    let parts: Vec<_> = value.split(':').collect();
    let (hours, minutes, seconds) = match parts.as_slice() {
        [minutes, seconds] => (0, *minutes, *seconds),
        [hours, minutes, seconds]
            if hours.len() >= 2 && hours.bytes().all(|b| b.is_ascii_digit()) =>
        {
            (hours.parse::<u64>().ok()?, *minutes, *seconds)
        }
        _ => return None,
    };
    if minutes.len() != 2 || !minutes.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let (seconds, millis) = seconds.split_once('.')?;
    if seconds.len() != 2
        || millis.len() != 3
        || !seconds
            .bytes()
            .chain(millis.bytes())
            .all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let minutes = minutes.parse::<u64>().ok()?;
    let seconds = seconds.parse::<u64>().ok()?;
    if minutes >= 60 || seconds >= 60 {
        return None;
    }
    hours
        .checked_mul(3_600_000)?
        .checked_add(minutes * 60_000 + seconds * 1000 + millis.parse::<u64>().ok()?)
}

// Accept cue text and cue settings, but never trust a supplied HLS timestamp map.
// The map is regenerated from the actual encoded video timeline.
pub fn validate_vtt(source: &str) -> Result<String, NurError> {
    if source.len() > MAX_VTT_BYTES
        || source
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\r' | '\n' | '\t'))
    {
        return Err(invalid(
            "Invalid or oversized WebVTT file (maximum 512 KiB).",
        ));
    }
    let normalized = source
        .trim_start_matches('\u{feff}')
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let (header, body) = normalized
        .split_once("\n\n")
        .ok_or_else(|| invalid("WebVTT requires a header and subtitle cues."))?;
    let first = header.lines().next().unwrap_or_default();
    if first != "WEBVTT" && !first.starts_with("WEBVTT ") && !first.starts_with("WEBVTT\t") {
        return Err(invalid("The file must start with WEBVTT."));
    }
    let mut cues = 0;
    let mut previous_start = 0;
    for block in body.split("\n\n").filter(|block| !block.trim().is_empty()) {
        let mut lines = block.lines();
        let first = lines.next().unwrap_or_default();
        if first == "NOTE" || first.starts_with("NOTE ") || first.starts_with("NOTE\t") {
            continue;
        }
        if matches!(first, "STYLE" | "REGION") {
            continue;
        }
        let timing = if first.contains("-->") {
            first
        } else {
            lines.next().unwrap_or_default()
        };
        let mut fields = timing.split([' ', '\t']).filter(|field| !field.is_empty());
        let start = fields.next().unwrap_or_default();
        if fields.next() != Some("-->") {
            return Err(invalid("Invalid WebVTT cue timing."));
        }
        let end = fields.next().unwrap_or_default();
        let start = timestamp(start).ok_or_else(|| invalid("Invalid WebVTT start timestamp."))?;
        let end = timestamp(end).ok_or_else(|| invalid("Invalid WebVTT end timestamp."))?;
        if end <= start || start < previous_start || lines.next().is_none() {
            return Err(invalid(
                "WebVTT cues must be ordered and contain text and a positive duration.",
            ));
        }
        previous_start = start;
        cues += 1;
    }
    if cues == 0 {
        return Err(invalid("The WebVTT file contains no subtitle cues."));
    }
    let canonical = format!("WEBVTT\n\n{}\n", body.trim_end());
    if canonical.len() > MAX_VTT_BYTES {
        return Err(invalid("WebVTT exceeds the maximum size of 512 KiB."));
    }
    Ok(canonical)
}

fn attributes(line: &str) -> Result<Vec<String>, NurError> {
    let mut quoted = false;
    let mut start = 0;
    let mut result = Vec::new();
    for (i, c) in line.char_indices() {
        if c == '"' {
            quoted = !quoted;
        }
        if c == ',' && !quoted {
            result.push(line[start..i].to_string());
            start = i + 1;
        }
    }
    if quoted {
        return Err(invalid("Invalid HLS master attributes."));
    }
    result.push(line[start..].to_string());
    Ok(result)
}

pub fn render_master(source: &str, tracks: &[MediaSubtitle]) -> Result<String, NurError> {
    if !source.starts_with("#EXTM3U\n") && !source.starts_with("#EXTM3U\r\n") {
        return Err(invalid("Invalid HLS master playlist."));
    }
    let mut labels = std::collections::HashSet::new();
    if tracks.iter().any(|track| !labels.insert(&track.label)) {
        return Err(invalid(
            "Subtitle display names must be unique within a video.",
        ));
    }
    let mut result = String::new();
    let mut variants = 0;
    for line in source.lines() {
        if let Some(attrs) = line.strip_prefix("#EXT-X-MEDIA:") {
            let attrs = attributes(attrs)?;
            if attrs.iter().any(|a| a == "TYPE=SUBTITLES") {
                // Existing foreign subtitle groups are not silently discarded.
                if !attrs.iter().any(|a| a == "GROUP-ID=\"cms-subs\"") {
                    return Err(invalid(
                        "This HLS master contains externally managed subtitles.",
                    ));
                }
                continue;
            }
        }
        if let Some(attrs) = line.strip_prefix("#EXT-X-STREAM-INF:") {
            let mut attrs = attributes(attrs)?;
            attrs.retain(|attr| !attr.starts_with("SUBTITLES="));
            if !tracks.is_empty() {
                attrs.push("SUBTITLES=\"cms-subs\"".into());
            }
            result.push_str(&format!("#EXT-X-STREAM-INF:{}\n", attrs.join(",")));
            variants += 1;
        } else {
            result.push_str(line);
            result.push('\n');
        }
    }
    if variants == 0 {
        return Err(invalid("The HLS master contains no video variants."));
    }
    for track in tracks {
        result.push_str(&format!("#EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID=\"cms-subs\",NAME=\"{}\",LANGUAGE=\"{}\",DEFAULT={},AUTOSELECT=YES,FORCED=NO,URI=\"sub-{}.m3u8\"\n", track.label, track.language, if track.is_default { "YES" } else { "NO" }, track.id));
    }
    Ok(result)
}

#[derive(Deserialize)]
struct Probe {
    streams: Vec<ProbeStream>,
    format: ProbeFormat,
}
#[derive(Deserialize)]
struct ProbeStream {
    start_time: String,
}
#[derive(Deserialize)]
struct ProbeFormat {
    duration: String,
}

pub async fn timeline(directory: &Path, master: &str) -> Result<(f64, u64), NurError> {
    // Probe only a generated local variant, never a URL supplied by the caller.
    let playlist = master
        .lines()
        .find(|line| !line.starts_with('#') && !line.trim().is_empty())
        .ok_or_else(|| invalid("Missing HLS video playlist."))?;
    if Path::new(playlist).components().count() != 1
        || !playlist.ends_with(".m3u8")
        || playlist.contains(['/', '\\', ':'])
    {
        return Err(invalid("Invalid HLS video playlist path."));
    }
    let output = timeout(
        Duration::from_secs(30),
        Command::new(&crate::config::settings().video.ffprobe)
            .args([
                "-v",
                "error",
                "-select_streams",
                "v:0",
                "-show_entries",
                "stream=start_time:format=duration",
                "-of",
                "json",
            ])
            .arg(directory.join(playlist))
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| invalid("Subtitle timeline probe timed out."))??;
    if !output.status.success() {
        return Err(invalid("Cannot read the HLS video timeline."));
    }
    let probe: Probe = serde_json::from_slice(&output.stdout)?;
    let duration: f64 = probe
        .format
        .duration
        .parse()
        .map_err(|_| invalid("Invalid HLS duration."))?;
    let start: f64 = probe
        .streams
        .first()
        .ok_or_else(|| invalid("Missing HLS video stream."))?
        .start_time
        .parse()
        .map_err(|_| invalid("Invalid HLS start time."))?;
    if !duration.is_finite() || duration <= 0.0 || !start.is_finite() || start < 0.0 {
        return Err(invalid("Invalid HLS video timeline."));
    }
    Ok((
        duration,
        ((start * 90_000.0).round() as u64) % (1_u64 << 33),
    ))
}

async fn write_new(path: &Path, bytes: &[u8]) -> Result<(), NurError> {
    use tokio::io::AsyncWriteExt;

    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .await?;
    let result = async {
        file.write_all(bytes).await?;
        file.sync_all().await
    }
    .await;
    drop(file);
    if let Err(error) = result {
        let _ = fs::remove_file(path).await;
        return Err(error.into());
    }
    Ok(())
}

fn track_files(
    track: &MediaSubtitle,
    duration: f64,
    mpegts: u64,
) -> Result<[(String, String); 2], NurError> {
    let vtt = track
        .webvtt
        .strip_prefix("WEBVTT\n\n")
        .ok_or(NurError::InvalidInput)?;
    let vtt = format!("WEBVTT\nX-TIMESTAMP-MAP=LOCAL:00:00:00.000,MPEGTS:{mpegts}\n\n{vtt}");
    let playlist = format!(
        "#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-TARGETDURATION:{}\n#EXT-X-MEDIA-SEQUENCE:0\n#EXT-X-PLAYLIST-TYPE:VOD\n#EXTINF:{duration:.6},\nsub-{}.vtt\n#EXT-X-ENDLIST\n",
        duration.ceil() as u64,
        track.id
    );
    Ok([
        (format!("sub-{}.vtt", track.id), vtt),
        (format!("sub-{}.m3u8", track.id), playlist),
    ])
}

// Include retained history, the new backup and the replacement master. Reused
// immutable subtitle files must not be counted twice.
pub async fn check_publication_size(
    directory: &Path,
    tracks: &[MediaSubtitle],
    timeline: (f64, u64),
    updated: &str,
    thumbnail_bytes: u64,
    limit: Option<u64>,
) -> Result<(), NurError> {
    let Some(limit) = limit else {
        return Ok(());
    };
    // The old master's bytes remain as the new backup, so only add the new master.
    let mut bytes = directory_size(directory)
        .await?
        .checked_add(thumbnail_bytes)
        .and_then(|size| size.checked_add(updated.len() as u64))
        .ok_or(NurError::InvalidInput)?;
    for track in tracks {
        for (name, content) in track_files(track, timeline.0, timeline.1)? {
            if !fs::try_exists(directory.join(name)).await? {
                bytes = bytes
                    .checked_add(content.len() as u64)
                    .ok_or(NurError::InvalidInput)?;
            }
        }
    }
    if bytes > limit {
        return Err(invalid(
            "HLS output including subtitles, backups and thumbnails exceeds the configured limit.",
        ));
    }
    Ok(())
}

pub async fn publish_tracks(
    directory: &Path,
    tracks: &[MediaSubtitle],
    duration: f64,
    mpegts: u64,
) -> Result<(), NurError> {
    for track in tracks {
        for (name, content) in track_files(track, duration, mpegts)? {
            let path = directory.join(name);
            if fs::try_exists(&path).await? {
                if fs::read(&path).await? != content.as_bytes() {
                    return Err(NurError::Conflict(
                        "Subtitle timeline has changed; replace the subtitle track.".into(),
                    ));
                }
            } else {
                write_new(&path, content.as_bytes()).await?;
            }
        }
    }
    Ok(())
}

pub async fn replace_master(
    directory: &Path,
    source: &str,
    updated: &str,
) -> Result<String, NurError> {
    let backup = backup_master(directory, source).await?;
    write_master(directory, updated).await?;
    Ok(backup)
}

async fn write_master(directory: &Path, source: &str) -> Result<(), NurError> {
    let temporary = directory.join(format!(".master-{}.tmp", Uuid::new_v4()));
    write_new(&temporary, source.as_bytes()).await?;
    if let Err(error) = fs::rename(&temporary, directory.join("master.m3u8")).await {
        let _ = fs::remove_file(&temporary).await;
        return Err(error.into());
    }
    Ok(())
}

pub async fn backup_master(directory: &Path, source: &str) -> Result<String, NurError> {
    let backup = format!("master.m3u8.{}.bak", Uuid::new_v4());
    write_new(&directory.join(&backup), source.as_bytes()).await?;
    Ok(backup)
}

pub async fn apply_publication(
    directory: &Path,
    publication: &SubtitlePublication,
) -> Result<(), NurError> {
    let expected_name = publication
        .backup_filename
        .strip_prefix("master.m3u8.")
        .and_then(|name| name.strip_suffix(".bak"));
    if expected_name.is_none_or(|name| Uuid::parse_str(name).is_err()) {
        return Err(NurError::InvalidInput);
    }
    let backup = directory.join(&publication.backup_filename);
    match fs::read(&backup).await {
        Ok(bytes) if bytes == publication.previous_master.as_bytes() => {}
        Ok(_) => {
            return Err(NurError::Conflict(
                "The HLS master backup has changed.".into(),
            ));
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {
            write_new(&backup, publication.previous_master.as_bytes()).await?;
        }
        Err(error) => return Err(error.into()),
    }
    match fs::read_to_string(directory.join("master.m3u8")).await {
        Ok(current) if current == publication.master => return Ok(()),
        Ok(current) if current != publication.previous_master => {
            return Err(NurError::Conflict(
                "The HLS master changed outside subtitle publication.".into(),
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    write_master(directory, &publication.master).await
}

// Callers hold the media advisory lock until this transaction finishes. Once
// committed, an intent is only completed forwards; an uncertain commit never
// causes an older master to be written over a newer operation.
pub async fn recover_publication_on(
    tx: &mut Transaction<'_, Postgres>,
    media_id: i32,
) -> Result<(), NurError> {
    let Some(publication) = handles::pending_subtitle_publication(tx, media_id).await? else {
        return Ok(());
    };
    if let Some(target) = handles::subtitle_target(tx, media_id).await? {
        let name = hls_directory_name(&target.filename).ok_or(NurError::InvalidInput)?;
        let directory = contained_storage_target(&target.path, name).await?;
        if fs::try_exists(&directory).await? {
            apply_publication(&directory, &publication).await?;
            let bytes = i64::try_from(directory_size(&directory).await?)
                .map_err(|_| NurError::InvalidInput)?;
            handles::update_hls_size(tx, media_id, bytes).await?;
        }
        // Missing HLS output is rebuilt from the committed subtitle originals
        // by the video worker, rather than blocking regeneration on an intent.
    }
    handles::finish_subtitle_publication(tx, media_id).await?;
    Ok(())
}

pub async fn recover_pending_publications(pool: &PgPool) -> Result<(), NurError> {
    for media_id in handles::pending_subtitle_media(pool).await? {
        if recover_pending_publication(pool, media_id).await.is_err() {
            // Errors may contain internal storage paths; log only the media ID.
            warn!(
                media_id,
                "Subtitle publication recovery failed; retained for retry"
            );
        }
    }
    Ok(())
}

async fn recover_pending_publication(pool: &PgPool, media_id: i32) -> Result<(), NurError> {
    let mut tx = handles::lock_media_files(pool, media_id).await?;
    recover_publication_on(&mut tx, media_id).await?;
    tx.commit().await?;
    Ok(())
}

// Called while the HLS publication transaction owns the media filesystem lock.
// All changes occur in staging; the existing directory remains live until the
// worker's normal publication and rollback mechanism takes over.
pub async fn prepare_hls_publication(
    staging: &Path,
    previous: &Path,
    tracks: &[MediaSubtitle],
    replacing: bool,
) -> Result<u64, NurError> {
    let master_path = staging.join("master.m3u8");
    let source = fs::read_to_string(&master_path).await?;
    let original_size = source.len() as u64;
    if replacing && fs::try_exists(previous).await? {
        let mut entries = fs::read_dir(previous).await?;
        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if entry.file_type().await?.is_file()
                && ((name.starts_with("master.m3u8.") && name.ends_with(".bak"))
                    || (name.starts_with("sub-")
                        && (name.ends_with(".vtt") || name.ends_with(".m3u8"))))
            {
                fs::copy(entry.path(), staging.join(name)).await?;
            }
        }
        match fs::read(previous.join("master.m3u8")).await {
            Ok(old_master) => {
                write_new(
                    &staging.join(format!("master.m3u8.{}.bak", Uuid::new_v4())),
                    &old_master,
                )
                .await?;
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    if !tracks.is_empty() {
        let (duration, mpegts) = timeline(staging, &source).await?;
        publish_tracks(staging, tracks, duration, mpegts).await?;
        let updated = render_master(&source, tracks)?;
        replace_master(staging, &source, &updated).await?;
    }
    let mut added = fs::metadata(&master_path)
        .await?
        .len()
        .saturating_sub(original_size);
    let mut entries = fs::read_dir(staging).await?;
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("sub-") || (name.starts_with("master.m3u8.") && name.ends_with(".bak"))
        {
            added = added
                .checked_add(entry.metadata().await?.len())
                .ok_or(NurError::InvalidInput)?;
        }
    }
    Ok(added)
}

pub async fn directory_size(directory: &Path) -> Result<u64, NurError> {
    let mut bytes = 0_u64;
    let mut entries = fs::read_dir(directory).await?;
    while let Some(entry) = entries.next_entry().await? {
        if entry.file_type().await?.is_file() {
            bytes = bytes
                .checked_add(entry.metadata().await?.len())
                .ok_or(NurError::InvalidInput)?;
        }
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MASTER: &str = "#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"audio\",URI=\"audio.m3u8\"\n#EXT-X-STREAM-INF:BANDWIDTH=1000,CODECS=\"avc1.64001e,mp4a.40.2\",AUDIO=\"audio\"\n720p.m3u8\n#EXT-X-STREAM-INF:BANDWIDTH=2000,CODECS=\"avc1.640028,mp4a.40.2\",AUDIO=\"audio\"\n1080p.m3u8\n";
    const VTT: &str = "WEBVTT\n\n00:00:01.000 --> 00:00:04.000\nHallo Welt\n";

    fn track() -> MediaSubtitle {
        MediaSubtitle {
            id: Uuid::new_v4().to_string(),
            language: "de-de".into(),
            label: "Deutsch".into(),
            is_default: false,
            webvtt: VTT.into(),
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn recovery_keeps_failed_intents_and_continues_with_other_media(pool: PgPool) {
        let broken: i32 = sqlx::query_scalar(
            "INSERT INTO media (filename, path) VALUES ('broken.mp4', '/uploads') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("broken media");
        let healthy: i32 = sqlx::query_scalar(
            "INSERT INTO media (filename, path) VALUES ('healthy.mp4', '/uploads') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("healthy media");
        sqlx::query("INSERT INTO media_video_variants (media_id, kind, profile, width, height, container, video_codec, filename, size) VALUES ($1, 'hls', 'hls', 1, 1, 'hls', 'h264', 'invalid.mp4', 0)")
            .bind(broken).execute(&pool).await.expect("invalid HLS target");
        for id in [broken, healthy] {
            sqlx::query("INSERT INTO media_subtitle_publications (media_id, previous_master, master, backup_filename) VALUES ($1, 'old', 'new', 'backup')")
                .bind(id).execute(&pool).await.expect("intent");
        }
        recover_pending_publications(&pool)
            .await
            .expect("startup continues");
        assert_eq!(
            handles::pending_subtitle_media(&pool)
                .await
                .expect("pending"),
            vec![broken]
        );
        recover_pending_publications(&pool)
            .await
            .expect("retry remains nonfatal");
        assert_eq!(
            handles::pending_subtitle_media(&pool)
                .await
                .expect("retained"),
            vec![broken]
        );
    }

    #[tokio::test]
    async fn publication_limit_counts_history_new_files_and_thumbnails_without_writing() {
        let directory = std::env::temp_dir().join(format!("nur-subtitle-limit-{}", Uuid::new_v4()));
        fs::create_dir(&directory).await.expect("directory");
        fs::write(directory.join("master.m3u8"), MASTER)
            .await
            .expect("master");
        fs::write(directory.join("master.m3u8.old.bak"), "history")
            .await
            .expect("history");
        let tracks = vec![track()];
        let updated = render_master(MASTER, &tracks).expect("render");
        let files = track_files(&tracks[0], 5.0, 90000).expect("files");
        let expected = MASTER.len() as u64
            + 7
            + 123
            + updated.len() as u64
            + files
                .iter()
                .map(|(_, content)| content.len() as u64)
                .sum::<u64>();
        assert!(
            check_publication_size(
                &directory,
                &tracks,
                (5.0, 90000),
                &updated,
                123,
                Some(expected - 1)
            )
            .await
            .is_err()
        );
        assert_eq!(
            directory_size(&directory).await.expect("size"),
            MASTER.len() as u64 + 7
        );
        check_publication_size(
            &directory,
            &tracks,
            (5.0, 90000),
            &updated,
            123,
            Some(expected),
        )
        .await
        .expect("exact limit");
        publish_tracks(&directory, &tracks, 5.0, 90000)
            .await
            .expect("tracks");
        check_publication_size(
            &directory,
            &tracks,
            (5.0, 90000),
            &updated,
            123,
            Some(expected),
        )
        .await
        .expect("reuse counted once");
        check_publication_size(&directory, &tracks, (5.0, 90000), &updated, u64::MAX, None)
            .await
            .expect("disabled limit");
        fs::remove_dir_all(directory).await.expect("cleanup");
    }

    #[test]
    fn validates_and_normalizes_webvtt_and_language() {
        let mut input = SubtitleUpload {
            language: " de-DE ".into(),
            label: " Deutsch ".into(),
            is_default: false,
            webvtt: VTT.replace('\n', "\r\n"),
        };
        validate_upload(&mut input).expect("valid upload");
        assert_eq!(input.language, "de-de");
        assert_eq!(input.webvtt, VTT);
        let mapped = VTT.replacen(
            "WEBVTT",
            "\u{feff}WEBVTT\nX-TIMESTAMP-MAP=LOCAL:00:00:00.000,MPEGTS:9999",
            1,
        );
        assert_eq!(validate_vtt(&mapped).expect("source map stripped"), VTT);
        assert!(validate_vtt("WEBVTT\n\n00:01.000 --> 00:02.000 align:start\ntext\n").is_ok());
    }

    #[test]
    fn rejects_bad_uploads_and_manifest_attribute_injection() {
        for vtt in [
            "not vtt",
            "WEBVTT\n\n",
            "WEBVTT\n\n00:00:04.000 --> 00:00:01.000\nbad",
            "WEBVTT\n\n00:60:00.000 --> 00:61:00.000\nbad",
            "WEBVTT\n\n00:00:01.000 --> 00:00:02.000",
            "WEBVTT\n\n00:00:01.000 --> 00:00:02.000\n\0",
        ] {
            assert!(validate_vtt(vtt).is_err(), "{vtt:?}");
        }
        assert!(validate_vtt(&"x".repeat(MAX_VTT_BYTES + 1)).is_err());
        for (language, label) in [
            ("../de", "Deutsch"),
            ("de", "bad\"label"),
            ("de", "bad\nlabel"),
            ("", "Deutsch"),
            ("de--de", "Deutsch"),
        ] {
            let mut input = SubtitleUpload {
                language: language.into(),
                label: label.into(),
                is_default: false,
                webvtt: VTT.into(),
            };
            assert!(validate_upload(&mut input).is_err());
        }
    }

    #[test]
    fn master_update_is_idempotent_and_preserves_audio_and_codec_attributes() {
        let tracks = vec![track()];
        let updated = render_master(MASTER, &tracks).expect("render master");
        assert_eq!(updated.matches("SUBTITLES=\"cms-subs\"").count(), 2);
        assert_eq!(updated.matches("#EXT-X-MEDIA:TYPE=SUBTITLES").count(), 1);
        assert!(updated.contains("CODECS=\"avc1.64001e,mp4a.40.2\",AUDIO=\"audio\""));
        assert_eq!(render_master(&updated, &tracks).expect("repeat"), updated);
        assert_eq!(render_master(&updated, &[]).expect("remove"), MASTER);
        let foreign = MASTER.to_string()
            + "#EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID=\"external\",URI=\"de.m3u8\"\n";
        assert!(render_master(&foreign, &tracks).is_err());
        assert!(render_master("#EXTM3U\n", &tracks).is_err());
    }

    #[tokio::test]
    async fn backs_up_exact_master_on_each_change_and_supports_rollback() {
        let directory = std::env::temp_dir().join(format!("nur-subtitles-{}", Uuid::new_v4()));
        fs::create_dir(&directory).await.expect("directory");
        fs::write(directory.join("master.m3u8"), MASTER)
            .await
            .expect("master");
        let tracks = vec![track()];
        publish_tracks(&directory, &tracks, 18.0, 7200)
            .await
            .expect("tracks");
        let vtt = fs::read_to_string(directory.join(format!("sub-{}.vtt", tracks[0].id)))
            .await
            .expect("vtt");
        assert!(vtt.contains("MPEGTS:7200"));
        let playlist = fs::read_to_string(directory.join(format!("sub-{}.m3u8", tracks[0].id)))
            .await
            .expect("playlist");
        assert!(playlist.contains("#EXT-X-TARGETDURATION:18\n"));
        assert!(playlist.contains("#EXTINF:18.000000,"));
        let updated = render_master(MASTER, &tracks).expect("updated");
        let first = replace_master(&directory, MASTER, &updated)
            .await
            .expect("replace");
        let second = replace_master(&directory, &updated, MASTER)
            .await
            .expect("remove");
        assert_ne!(first, second);
        assert_eq!(
            fs::read_to_string(directory.join(first))
                .await
                .expect("backup"),
            MASTER
        );
        assert_eq!(
            fs::read_to_string(directory.join(second))
                .await
                .expect("backup"),
            updated
        );
        write_master(&directory, &updated).await.expect("rollback");
        assert_eq!(
            fs::read_to_string(directory.join("master.m3u8"))
                .await
                .expect("live"),
            updated
        );
        fs::remove_dir_all(&directory).await.expect("cleanup");
    }

    #[tokio::test]
    async fn failed_publication_leaves_a_recoverable_backup() {
        let directory = std::env::temp_dir().join(format!("nur-subtitles-{}", Uuid::new_v4()));
        fs::create_dir(&directory).await.expect("directory");
        fs::create_dir(directory.join("master.m3u8"))
            .await
            .expect("block rename");
        assert!(replace_master(&directory, MASTER, "changed").await.is_err());
        let mut entries = fs::read_dir(&directory).await.expect("entries");
        let mut backups = 0;
        while let Some(entry) = entries.next_entry().await.expect("entry") {
            if entry.file_name().to_string_lossy().ends_with(".bak") {
                assert_eq!(
                    fs::read_to_string(entry.path()).await.expect("backup"),
                    MASTER
                );
                backups += 1;
            }
        }
        assert_eq!(backups, 1);
        fs::remove_dir_all(&directory).await.expect("cleanup");
    }
    #[tokio::test]
    #[ignore = "requires ffmpeg and ffprobe"]
    async fn ffmpeg_timeline_and_reprocessing_preserve_subtitles_and_backups() {
        let root = std::env::temp_dir().join(format!("nur-hls-subtitles-{}", Uuid::new_v4()));
        let old = root.join("old");
        let new = root.join("new");
        fs::create_dir_all(&old).await.expect("old");
        fs::create_dir(&new).await.expect("new");
        let output = Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=160x90:rate=25",
                "-t",
                "2",
                "-an",
                "-c:v",
                "libx264",
                "-g",
                "25",
                "-f",
                "hls",
                "-hls_time",
                "1",
                "-hls_playlist_type",
                "vod",
                "-hls_segment_type",
                "fmp4",
                "-hls_fmp4_init_filename",
                "init.mp4",
            ])
            .current_dir(&old)
            .arg("video.m3u8")
            .output()
            .await
            .expect("ffmpeg");
        assert!(output.status.success(), "ffmpeg fixture generation");
        let master = "#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-STREAM-INF:BANDWIDTH=200000,RESOLUTION=160x90\nvideo.m3u8\n";
        fs::write(old.join("master.m3u8"), master)
            .await
            .expect("master");
        let tracks = vec![track()];
        let (duration, mpegts) = timeline(&old, master).await.expect("timeline");
        assert!((duration - 2.0).abs() < 0.01);
        publish_tracks(&old, &tracks, duration, mpegts)
            .await
            .expect("tracks");
        let updated = render_master(master, &tracks).expect("updated");
        let backup = replace_master(&old, master, &updated)
            .await
            .expect("backup");
        let mut files = fs::read_dir(&old).await.expect("files");
        while let Some(entry) = files.next_entry().await.expect("entry") {
            let name = entry.file_name();
            if !name.to_string_lossy().starts_with("sub-")
                && !name.to_string_lossy().ends_with(".bak")
            {
                fs::copy(entry.path(), new.join(name))
                    .await
                    .expect("video fixture");
            }
        }
        // FFmpeg emits a fresh master without subtitle entries.
        fs::write(new.join("master.m3u8"), master)
            .await
            .expect("new master");
        let mut revised = tracks.clone();
        revised[0].id = Uuid::new_v4().to_string();
        assert!(
            prepare_hls_publication(&new, &old, &revised, true)
                .await
                .expect("prepare")
                > 0
        );
        assert_eq!(
            fs::read_to_string(new.join(&backup))
                .await
                .expect("old backup"),
            master
        );
        let current = fs::read_to_string(new.join("master.m3u8"))
            .await
            .expect("new master");
        assert!(current.contains(&format!("sub-{}.m3u8", revised[0].id)));
        assert!(new.join(format!("sub-{}.vtt", tracks[0].id)).is_file());
        assert!(new.join(format!("sub-{}.vtt", revised[0].id)).is_file());
        let mut entries = fs::read_dir(&new).await.expect("entries");
        let mut previous_master_preserved = false;
        while let Some(entry) = entries.next_entry().await.expect("entry") {
            if entry.file_name().to_string_lossy().ends_with(".bak")
                && fs::read_to_string(entry.path()).await.expect("backup") == updated
            {
                previous_master_preserved = true;
            }
        }
        assert!(previous_master_preserved);
        fs::remove_dir_all(root).await.expect("cleanup");
    }
    #[test]
    fn accepts_webvtt_spaces_and_tabs_but_requires_timing_separators() {
        for separator in [" ", "  ", "\t", " \t "] {
            let vtt =
                format!("WEBVTT\n\n00:00:01.000{separator}-->{separator}00:00:02.000\nText\n");
            assert!(validate_vtt(&vtt).is_ok(), "separator {separator:?}");
        }
        for timing in [
            "00:00:01.000-->00:00:02.000",
            "00:00:01.000 -->00:00:02.000",
            "00:00:01.000--> 00:00:02.000",
        ] {
            assert!(validate_vtt(&format!("WEBVTT\n\n{timing}\nText\n")).is_err());
        }
    }

    #[tokio::test]
    async fn resumes_publication_before_and_after_master_rename_without_duplicate_backups() {
        let directory = std::env::temp_dir().join(format!("nur-publication-{}", Uuid::new_v4()));
        fs::create_dir(&directory).await.expect("directory");
        fs::write(directory.join("master.m3u8"), MASTER)
            .await
            .expect("master");
        let updated = render_master(MASTER, &[track()]).expect("updated");
        let backup_filename = backup_master(&directory, MASTER).await.expect("backup");
        // Aborting before the database commit leaves the live master untouched.
        assert_eq!(
            fs::read_to_string(directory.join("master.m3u8"))
                .await
                .expect("live"),
            MASTER
        );
        let publication = SubtitlePublication {
            previous_master: MASTER.into(),
            master: updated.clone(),
            backup_filename: backup_filename.clone(),
        };
        // Simulate a restart after the durable intent committed but before rename.
        apply_publication(&directory, &publication)
            .await
            .expect("recover before rename");
        assert_eq!(
            fs::read_to_string(directory.join("master.m3u8"))
                .await
                .expect("live"),
            updated
        );
        // Simulate a restart after rename but before clearing the durable intent.
        apply_publication(&directory, &publication)
            .await
            .expect("recover after rename");
        assert_eq!(
            fs::read_to_string(directory.join(&backup_filename))
                .await
                .expect("backup"),
            MASTER
        );
        let mut entries = fs::read_dir(&directory).await.expect("entries");
        let mut backups = 0;
        while let Some(entry) = entries.next_entry().await.expect("entry") {
            backups += usize::from(entry.file_name().to_string_lossy().ends_with(".bak"));
        }
        assert_eq!(backups, 1);
        fs::remove_dir_all(directory).await.expect("cleanup");
    }

    #[tokio::test]
    async fn stale_publication_cannot_overwrite_newer_master_or_foreign_paths() {
        let directory = std::env::temp_dir().join(format!("nur-publication-{}", Uuid::new_v4()));
        fs::create_dir(&directory).await.expect("directory");
        let newer = MASTER.to_string() + "# newer master\n";
        fs::write(directory.join("master.m3u8"), &newer)
            .await
            .expect("newer");
        let publication = SubtitlePublication {
            previous_master: MASTER.into(),
            master: "older update".into(),
            backup_filename: format!("master.m3u8.{}.bak", Uuid::new_v4()),
        };
        assert!(apply_publication(&directory, &publication).await.is_err());
        assert_eq!(
            fs::read_to_string(directory.join("master.m3u8"))
                .await
                .expect("live"),
            newer
        );
        let invalid = SubtitlePublication {
            backup_filename: "../outside.bak".into(),
            ..publication
        };
        assert!(apply_publication(&directory, &invalid).await.is_err());
        fs::remove_dir_all(directory).await.expect("cleanup");
    }

    #[tokio::test]
    async fn publication_failure_can_be_retried_from_same_intent() {
        let directory = std::env::temp_dir().join(format!("nur-publication-{}", Uuid::new_v4()));
        fs::create_dir(&directory).await.expect("directory");
        fs::create_dir(directory.join("master.m3u8"))
            .await
            .expect("block publication");
        let publication = SubtitlePublication {
            previous_master: MASTER.into(),
            master: MASTER.to_string() + "# updated\n",
            backup_filename: format!("master.m3u8.{}.bak", Uuid::new_v4()),
        };
        assert!(apply_publication(&directory, &publication).await.is_err());
        fs::remove_dir(directory.join("master.m3u8"))
            .await
            .expect("unblock");
        apply_publication(&directory, &publication)
            .await
            .expect("retry");
        assert_eq!(
            fs::read_to_string(directory.join("master.m3u8"))
                .await
                .expect("live"),
            publication.master
        );
        assert_eq!(
            fs::read_to_string(directory.join(&publication.backup_filename))
                .await
                .expect("backup"),
            MASTER
        );
        fs::remove_dir_all(directory).await.expect("cleanup");
    }

    #[tokio::test]
    async fn rebuilds_when_previous_hls_directory_or_master_is_missing() {
        let root = std::env::temp_dir().join(format!("nur-rebuild-{}", Uuid::new_v4()));
        let staging = root.join("staging");
        let previous = root.join("previous");
        fs::create_dir_all(&staging).await.expect("staging");
        fs::write(staging.join("master.m3u8"), MASTER)
            .await
            .expect("master");
        assert_eq!(
            prepare_hls_publication(&staging, &previous, &[], true)
                .await
                .expect("missing directory"),
            0
        );
        fs::create_dir(&previous).await.expect("previous");
        let history = format!("master.m3u8.{}.bak", Uuid::new_v4());
        fs::write(previous.join(&history), "old backup")
            .await
            .expect("backup");
        assert!(
            prepare_hls_publication(&staging, &previous, &[], true)
                .await
                .expect("missing master")
                > 0
        );
        assert_eq!(
            fs::read_to_string(staging.join(history))
                .await
                .expect("retained backup"),
            "old backup"
        );
        assert_eq!(
            fs::read_to_string(staging.join("master.m3u8"))
                .await
                .expect("live"),
            MASTER
        );
        fs::remove_dir_all(root).await.expect("cleanup");
    }
}
