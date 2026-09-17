use std::{
    fs::{self, OpenOptions},
    io::{Error as IoError, ErrorKind, Write},
    path::{Component, Path, PathBuf},
    process,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime},
};

use chrono::{Datelike, Local};

use super::StorageDirectory;
use crate::Error;

// Resumable uploads append `.uploading.json.tmp` to the target name.
const MAX_FILE_NAME_BYTES: usize = 236;
static TEMPORARY_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(super) fn render_path(template: &str) -> String {
    let now = Local::now();
    template
        .replace("{year}", &now.year().to_string())
        .replace("{month}", &format!("{:02}", now.month()))
}

pub(super) fn validate_file(directory: &StorageDirectory, filename: &str) -> Result<(), Error> {
    validate_filename(filename).map_err(Error::PluginBadRequest)?;
    validate_extension(directory, filename).map_err(Error::PluginBadRequest)
}

pub(super) fn validate_filename(filename: &str) -> Result<(), String> {
    let mut components = Path::new(filename).components();
    let single_normal_component =
        matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none();

    if filename.is_empty()
        || filename.len() > MAX_FILE_NAME_BYTES
        || !single_normal_component
        || filename.chars().any(char::is_control)
    {
        return Err("invalid plugin storage filename".into());
    }

    Ok(())
}

pub(super) fn validate_stored_path(directory: &StorageDirectory, path: &str) -> Result<(), String> {
    let template_segments: Vec<_> = directory.path.split('/').collect();
    let path_segments: Vec<_> = path.split('/').collect();

    if path_segments.len() != template_segments.len() + 1 {
        return Err("invalid plugin storage path".into());
    }

    for (template, actual) in template_segments.iter().zip(&path_segments) {
        let valid = match *template {
            "{year}" => actual.len() == 4 && actual.bytes().all(|byte| byte.is_ascii_digit()),
            "{month}" => {
                actual.len() == 2
                    && actual.bytes().all(|byte| byte.is_ascii_digit())
                    && actual
                        .parse::<u8>()
                        .is_ok_and(|month| (1..=12).contains(&month))
            }
            literal => literal == *actual,
        };
        if !valid {
            return Err("invalid plugin storage path".into());
        }
    }

    let filename = path_segments.last().copied().unwrap_or_default();

    validate_filename(filename)?;
    validate_extension(directory, filename)
}

pub(super) fn validate_extension(
    directory: &StorageDirectory,
    filename: &str,
) -> Result<(), String> {
    let extension = Path::new(filename)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| "plugin storage filename has no permitted extension".to_string())?;

    if !directory.extensions.contains(&extension) {
        return Err("plugin storage file extension is not permitted".into());
    }

    Ok(())
}

pub(super) fn write_temporary(root: &Path, bytes: &[u8]) -> Result<PathBuf, Error> {
    for _ in 0..100 {
        let sequence = TEMPORARY_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temporary = root.join(format!(
            ".nur-upload-{}-{sequence}.uploading",
            process::id()
        ));

        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(mut file) => {
                if let Err(error) = file.write_all(bytes).and_then(|_| file.sync_all()) {
                    let _ = fs::remove_file(&temporary);
                    return Err(Error::Io(error));
                }
                return Ok(temporary);
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(Error::Io(error)),
        }
    }

    Err(Error::Io(IoError::new(
        ErrorKind::AlreadyExists,
        "could not allocate a temporary plugin storage file",
    )))
}

pub(super) fn regular_file_size(path: &Path) -> Result<u64, Error> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() && !metadata.file_type().is_symlink() => {
            Ok(metadata.len())
        }
        Ok(_) => Err(Error::PluginBadRequest(
            "invalid plugin storage path".into(),
        )),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(0),
        Err(error) => Err(Error::Io(error)),
    }
}

pub(super) fn safe_directory_path(
    base: &Path,
    relative: &Path,
    create: bool,
) -> Result<PathBuf, Error> {
    let mut current = base.to_path_buf();

    for component in relative.components() {
        let Component::Normal(component) = component else {
            return Err(Error::PluginBadRequest(
                "invalid plugin storage path".into(),
            ));
        };
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() => {
            }
            Ok(_) => {
                return Err(Error::PluginBadRequest(
                    "invalid plugin storage path".into(),
                ));
            }
            Err(error) if error.kind() == ErrorKind::NotFound && create => {
                match fs::create_dir(&current) {
                    Ok(()) => {}
                    Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                        let metadata = fs::symlink_metadata(&current).map_err(Error::Io)?;
                        if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
                            return Err(Error::PluginBadRequest(
                                "invalid plugin storage path".into(),
                            ));
                        }
                    }
                    Err(error) => return Err(Error::Io(error)),
                }
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {
                return Err(Error::PluginNotFound);
            }
            Err(error) => return Err(Error::Io(error)),
        }
    }

    Ok(current)
}

pub(super) fn cleanup_incomplete_files(root: &Path, maximum_age: Duration) -> Result<(), Error> {
    if !root.exists() {
        return Ok(());
    }

    let now = SystemTime::now();
    let mut pending = vec![root.to_path_buf()];

    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).map_err(Error::Io)? {
            let entry = entry.map_err(Error::Io)?;
            let file_type = entry.file_type().map_err(Error::Io)?;

            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                pending.push(entry.path());
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            let path = entry.path();
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let is_upload = name.ends_with(".uploading");
            let is_metadata = name.ends_with(".uploading.json");
            let is_temporary_metadata = name.ends_with(".uploading.json.tmp");

            if !is_upload && !is_metadata && !is_temporary_metadata {
                continue;
            }
            let modified = entry
                .metadata()
                .map_err(Error::Io)?
                .modified()
                .map_err(Error::Io)?;
            let stale = now.duration_since(modified).unwrap_or_default() >= maximum_age;
            let orphaned_metadata = is_metadata && {
                let upload_name = name.strip_suffix(".json").unwrap_or_default();
                !path.with_file_name(upload_name).exists()
            };

            if stale || orphaned_metadata {
                match fs::remove_file(&path) {
                    Ok(()) => {}
                    Err(error) if error.kind() == ErrorKind::NotFound => {}
                    Err(error) => return Err(Error::Io(error)),
                }
            }
        }
    }

    Ok(())
}

pub(super) fn ensure_regular_target(root: &Path, target: &Path) -> Result<(), Error> {
    if !target.starts_with(root) {
        return Err(Error::PluginBadRequest(
            "invalid plugin storage path".into(),
        ));
    }

    if let Ok(metadata) = fs::symlink_metadata(target)
        && (!metadata.file_type().is_file() || metadata.file_type().is_symlink())
    {
        return Err(Error::PluginBadRequest(
            "invalid plugin storage path".into(),
        ));
    }

    Ok(())
}

pub(super) fn append_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}

pub(super) fn percent_encode_path(path: &str) -> String {
    path.bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'/' => {
                format!("{}", byte as char).into_bytes()
            }
            _ => format!("%{byte:02X}").into_bytes(),
        })
        .map(char::from)
        .collect()
}
