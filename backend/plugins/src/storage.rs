use std::{
    collections::HashMap,
    env, fs,
    fs::OpenOptions,
    io::Write,
    path::{Component, Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime},
};

use chrono::{Datelike, Local};

use crate::{
    Error,
    manifest::{
        StorageDirectoryManifest, StorageUpload as ManifestStorageUpload,
        StorageVisibility as ManifestStorageVisibility, valid_plugin_id, valid_storage_extension,
        valid_storage_id, valid_storage_path,
    },
};

// Resumable uploads append `.uploading.json.tmp` to the target name.
const MAX_FILE_NAME_BYTES: usize = 236;
static TEMPORARY_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageVisibility {
    Public,
    Private,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BrowserUpload {
    None,
    Authenticated,
    Link,
}

#[derive(Clone, Debug)]
pub struct StorageDirectory {
    pub plugin_id: String,
    pub id: String,
    pub path: String,
    pub extensions: Vec<String>,
    pub visibility: StorageVisibility,
    pub upload: BrowserUpload,
    pub roles: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct PluginStorage {
    public_root: PathBuf,
    private_root: Option<PathBuf>,
    operation_locks: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
    plugin_sizes: Arc<Mutex<HashMap<String, u64>>>,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct StoredFile {
    pub path: String,
    pub public_url: Option<String>,
}

impl PluginStorage {
    pub fn from_environment() -> Result<Self, Error> {
        let storage = env::var("STORAGE").unwrap_or_else(|_| "./uploads".into());
        let public_root = PathBuf::from(storage).join("plugins");
        let private_root = env::var("NUR_PLUGIN_STORAGE")
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .map(PathBuf::from);

        fs::create_dir_all(&public_root).map_err(Error::Io)?;
        let public_root = fs::canonicalize(public_root).map_err(Error::Io)?;
        let private_root = private_root
            .map(|root| {
                fs::create_dir_all(&root).map_err(Error::Io)?;
                fs::canonicalize(root).map_err(Error::Io)
            })
            .transpose()?;
        if private_root.as_ref().is_some_and(|private_root| {
            private_root.starts_with(&public_root) || public_root.starts_with(private_root)
        }) {
            return Err(Error::Manifest(
                "NUR_PLUGIN_STORAGE and STORAGE/plugins must be separate, non-overlapping directories"
                    .into(),
            ));
        }
        Ok(Self {
            public_root,
            private_root,
            operation_locks: Arc::new(Mutex::new(HashMap::new())),
            plugin_sizes: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub fn validate_directory(&self, directory: &StorageDirectory) -> Result<(), Error> {
        if !valid_plugin_id(&directory.plugin_id)
            || !valid_storage_id(&directory.id)
            || !valid_storage_path(&directory.path)
            || directory.extensions.is_empty()
            || directory
                .extensions
                .iter()
                .any(|extension| !valid_storage_extension(extension))
        {
            return Err(Error::Manifest(
                "plugin storage directory is not valid".into(),
            ));
        }
        if directory.visibility == StorageVisibility::Private && self.private_root.is_none() {
            return Err(Error::Manifest(format!(
                "plugin '{}' declares private storage '{}', but NUR_PLUGIN_STORAGE is not configured",
                directory.plugin_id, directory.id
            )));
        }
        Ok(())
    }

    pub fn validate_upload_request(
        &self,
        directory: &StorageDirectory,
        filename: &str,
        maximum_size: u64,
        maximum_plugin_size: u64,
    ) -> Result<(), Error> {
        self.validate_directory(directory)?;
        validate_filename(filename).map_err(Error::PluginBadRequest)?;
        validate_extension(directory, filename).map_err(Error::PluginBadRequest)?;
        if maximum_size == 0 || maximum_size > maximum_plugin_size {
            return Err(Error::PluginBadRequest("invalid plugin upload size".into()));
        }
        Ok(())
    }

    pub fn cleanup_incomplete_uploads(&self, maximum_age: Duration) -> Result<(), Error> {
        cleanup_incomplete_files(&self.public_root, maximum_age)?;
        if let Some(root) = &self.private_root {
            cleanup_incomplete_files(root, maximum_age)?;
        }
        Ok(())
    }

    pub fn write(
        &self,
        directory: &StorageDirectory,
        filename: &str,
        bytes: &[u8],
        maximum_size: usize,
        maximum_plugin_size: u64,
    ) -> Result<StoredFile, Error> {
        self.validate_directory(directory)?;
        if bytes.len() > maximum_size {
            return Err(Error::PluginBadRequest(
                "plugin storage write exceeds limit".into(),
            ));
        }
        validate_filename(filename).map_err(Error::PluginBadRequest)?;
        validate_extension(directory, filename).map_err(Error::PluginBadRequest)?;
        let operation_lock = self.operation_lock(&directory.plugin_id)?;
        let _operation = operation_lock
            .lock()
            .map_err(|_| Error::Io(std::io::Error::other("plugin storage lock is poisoned")))?;
        let root = self.create_directory_root(directory)?;
        let target = root.join(filename);
        ensure_regular_target(&root, &target)?;
        let current_size = regular_file_size(&target)?;
        let projected_size = self
            .cached_plugin_size(&directory.plugin_id)?
            .saturating_sub(current_size)
            .saturating_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
        if projected_size > maximum_plugin_size {
            return Err(Error::PluginBadRequest(
                "plugin storage quota exceeded".into(),
            ));
        }

        let temporary = write_temporary(&root, bytes)?;
        if let Err(error) = fs::rename(&temporary, &target) {
            let _ = fs::remove_file(&temporary);
            return Err(Error::Io(error));
        }
        self.set_cached_plugin_size(&directory.plugin_id, projected_size)?;
        self.stored_file(directory, filename)
    }

    pub fn delete(&self, directory: &StorageDirectory, path: &str) -> Result<(), Error> {
        self.validate_directory(directory)?;
        validate_stored_path(directory, path).map_err(Error::PluginBadRequest)?;
        let operation_lock = self.operation_lock(&directory.plugin_id)?;
        let _operation = operation_lock
            .lock()
            .map_err(|_| Error::Io(std::io::Error::other("plugin storage lock is poisoned")))?;
        let namespace = self.namespace_root(directory)?;
        let relative = Path::new(path);
        let parent = safe_directory_path(
            &namespace,
            relative.parent().unwrap_or_else(|| Path::new("")),
            false,
        )?;
        let target = parent.join(
            relative
                .file_name()
                .ok_or_else(|| Error::PluginBadRequest("invalid plugin storage path".into()))?,
        );
        ensure_regular_target(&parent, &target)?;
        let removed_size = regular_file_size(&target)?;
        let current_size = self.cached_plugin_size(&directory.plugin_id)?;
        fs::remove_file(target).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                Error::PluginNotFound
            } else {
                Error::Io(error)
            }
        })?;
        self.set_cached_plugin_size(
            &directory.plugin_id,
            current_size.saturating_sub(removed_size),
        )
    }

    pub fn file_path(&self, directory: &StorageDirectory, path: &str) -> Result<PathBuf, Error> {
        self.validate_directory(directory)?;
        validate_stored_path(directory, path).map_err(Error::PluginBadRequest)?;
        let namespace = self.namespace_root(directory)?;
        let relative = Path::new(path);
        let parent = safe_directory_path(
            &namespace,
            relative.parent().unwrap_or_else(|| Path::new("")),
            false,
        )?;
        let target = parent.join(
            relative
                .file_name()
                .ok_or_else(|| Error::PluginBadRequest("invalid plugin storage path".into()))?,
        );
        ensure_regular_target(&parent, &target)?;
        Ok(target)
    }

    pub fn prepare_resumable_upload(
        &self,
        directory: &StorageDirectory,
        filename: &str,
        total_size: u64,
        maximum_size: u64,
        maximum_plugin_size: u64,
    ) -> Result<PathBuf, Error> {
        self.validate_directory(directory)?;
        if total_size == 0 || total_size > maximum_size {
            return Err(Error::PluginBadRequest(
                "plugin upload exceeds size limit".into(),
            ));
        }
        validate_filename(filename).map_err(Error::PluginBadRequest)?;
        validate_extension(directory, filename).map_err(Error::PluginBadRequest)?;
        let operation_lock = self.operation_lock(&directory.plugin_id)?;
        let _operation = operation_lock
            .lock()
            .map_err(|_| Error::Io(std::io::Error::other("plugin storage lock is poisoned")))?;
        let root = self.create_directory_root(directory)?;
        let target = root.join(filename);
        ensure_regular_target(&root, &target)?;
        if target.exists() {
            return Err(Error::PluginBadRequest(
                "plugin storage file already exists".into(),
            ));
        }
        let temporary = append_suffix(&target, ".uploading");
        let reservation_exists = temporary.exists();
        let existing_reservation = regular_file_size(&temporary)?;
        if reservation_exists && existing_reservation != total_size {
            return Err(Error::PluginBadRequest(
                "plugin upload size does not match its reservation".into(),
            ));
        }
        let projected_size = self
            .cached_plugin_size(&directory.plugin_id)?
            .saturating_sub(existing_reservation)
            .saturating_add(total_size);
        if projected_size > maximum_plugin_size {
            return Err(Error::PluginBadRequest(
                "plugin storage quota exceeded".into(),
            ));
        }
        if !reservation_exists {
            let file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .map_err(Error::Io)?;
            if let Err(error) = file.set_len(total_size).and_then(|_| file.sync_all()) {
                let _ = fs::remove_file(&temporary);
                return Err(Error::Io(error));
            }
        }
        self.set_cached_plugin_size(&directory.plugin_id, projected_size)?;
        Ok(target)
    }

    pub fn complete_resumable_upload(
        &self,
        directory: &StorageDirectory,
        filename: &str,
        temporary: &Path,
        total_size: u64,
        maximum_plugin_size: u64,
    ) -> Result<StoredFile, Error> {
        self.validate_directory(directory)?;
        validate_filename(filename).map_err(Error::PluginBadRequest)?;
        validate_extension(directory, filename).map_err(Error::PluginBadRequest)?;
        let operation_lock = self.operation_lock(&directory.plugin_id)?;
        let _operation = operation_lock
            .lock()
            .map_err(|_| Error::Io(std::io::Error::other("plugin storage lock is poisoned")))?;
        let root = self.directory_root(directory)?;
        let target = root.join(filename);
        ensure_regular_target(&root, &target)?;
        let expected_temporary = append_suffix(&target, ".uploading");
        if temporary != expected_temporary {
            return Err(Error::PluginBadRequest(
                "invalid plugin upload temporary path".into(),
            ));
        }
        if target.exists() {
            return Err(Error::PluginBadRequest(
                "plugin storage file already exists".into(),
            ));
        }
        let actual_size = regular_file_size(temporary)?;
        if actual_size != total_size {
            return Err(Error::PluginBadRequest(
                "plugin upload size does not match".into(),
            ));
        }
        if self.cached_plugin_size(&directory.plugin_id)? > maximum_plugin_size {
            return Err(Error::PluginBadRequest(
                "plugin storage quota exceeded".into(),
            ));
        }
        fs::rename(temporary, &target).map_err(Error::Io)?;
        self.stored_file(directory, filename)
    }

    pub fn recover_resumable_upload(
        &self,
        directory: &StorageDirectory,
        filename: &str,
        total_size: u64,
    ) -> Result<Option<(StoredFile, PathBuf)>, Error> {
        self.validate_directory(directory)?;
        validate_filename(filename).map_err(Error::PluginBadRequest)?;
        validate_extension(directory, filename).map_err(Error::PluginBadRequest)?;
        let operation_lock = self.operation_lock(&directory.plugin_id)?;
        let _operation = operation_lock
            .lock()
            .map_err(|_| Error::Io(std::io::Error::other("plugin storage lock is poisoned")))?;
        let root = self.directory_root(directory)?;
        let target = root.join(filename);
        let temporary = append_suffix(&target, ".uploading");
        if temporary.exists() || !target.exists() {
            return Ok(None);
        }
        ensure_regular_target(&root, &target)?;
        if regular_file_size(&target)? != total_size {
            return Err(Error::PluginBadRequest(
                "finalized plugin upload size does not match".into(),
            ));
        }
        Ok(Some((self.stored_file(directory, filename)?, target)))
    }

    pub fn rollback_resumable_upload(
        &self,
        directory: &StorageDirectory,
        filename: &str,
        temporary: &Path,
    ) -> Result<(), Error> {
        let operation_lock = self.operation_lock(&directory.plugin_id)?;
        let _operation = operation_lock
            .lock()
            .map_err(|_| Error::Io(std::io::Error::other("plugin storage lock is poisoned")))?;
        let target = self.directory_root(directory)?.join(filename);
        let expected_temporary = append_suffix(&target, ".uploading");
        if temporary != expected_temporary {
            return Err(Error::PluginBadRequest(
                "invalid plugin upload temporary path".into(),
            ));
        }
        fs::rename(target, temporary).map_err(Error::Io)
    }

    fn stored_file(
        &self,
        directory: &StorageDirectory,
        filename: &str,
    ) -> Result<StoredFile, Error> {
        let relative = format!("{}/{}", render_path(&directory.path), filename);
        Ok(StoredFile {
            path: relative.clone(),
            public_url: (directory.visibility == StorageVisibility::Public).then(|| {
                format!(
                    "/uploads/plugins/{}/{}",
                    directory.plugin_id,
                    percent_encode_path(&relative)
                )
            }),
        })
    }

    fn directory_root(&self, directory: &StorageDirectory) -> Result<PathBuf, Error> {
        let relative = Path::new(&directory.plugin_id).join(render_path(&directory.path));
        safe_directory_path(self.storage_root(directory)?, &relative, false)
    }

    fn create_directory_root(&self, directory: &StorageDirectory) -> Result<PathBuf, Error> {
        let relative = Path::new(&directory.plugin_id).join(render_path(&directory.path));
        safe_directory_path(self.storage_root(directory)?, &relative, true)
    }

    fn namespace_root(&self, directory: &StorageDirectory) -> Result<PathBuf, Error> {
        safe_directory_path(
            self.storage_root(directory)?,
            Path::new(&directory.plugin_id),
            false,
        )
    }

    fn storage_root(&self, directory: &StorageDirectory) -> Result<&Path, Error> {
        Ok(match directory.visibility {
            StorageVisibility::Public => &self.public_root,
            StorageVisibility::Private => self
                .private_root
                .as_ref()
                .ok_or_else(|| Error::Manifest("NUR_PLUGIN_STORAGE is not configured".into()))?,
        })
    }

    fn scan_plugin_size(&self, plugin_id: &str) -> Result<u64, Error> {
        let public = storage_size(&self.public_root.join(plugin_id))?;
        let private = self
            .private_root
            .as_ref()
            .map(|root| storage_size(&root.join(plugin_id)))
            .transpose()?
            .unwrap_or(0);
        Ok(public.saturating_add(private))
    }

    fn cached_plugin_size(&self, plugin_id: &str) -> Result<u64, Error> {
        if let Some(size) = self
            .plugin_sizes
            .lock()
            .map_err(|_| Error::Io(std::io::Error::other("plugin storage lock is poisoned")))?
            .get(plugin_id)
            .copied()
        {
            return Ok(size);
        }
        let size = self.scan_plugin_size(plugin_id)?;
        self.set_cached_plugin_size(plugin_id, size)?;
        Ok(size)
    }

    fn set_cached_plugin_size(&self, plugin_id: &str, size: u64) -> Result<(), Error> {
        self.plugin_sizes
            .lock()
            .map_err(|_| Error::Io(std::io::Error::other("plugin storage lock is poisoned")))?
            .insert(plugin_id.to_owned(), size);
        Ok(())
    }

    fn operation_lock(&self, plugin_id: &str) -> Result<Arc<Mutex<()>>, Error> {
        let mut locks = self
            .operation_locks
            .lock()
            .map_err(|_| Error::Io(std::io::Error::other("plugin storage lock is poisoned")))?;
        Ok(Arc::clone(
            locks
                .entry(plugin_id.to_owned())
                .or_insert_with(|| Arc::new(Mutex::new(()))),
        ))
    }
}

impl StorageDirectory {
    pub(crate) fn from_manifest(
        plugin_id: &str,
        directory: &StorageDirectoryManifest,
    ) -> Result<Self, Error> {
        Ok(Self {
            plugin_id: plugin_id.into(),
            id: directory.id.clone(),
            path: directory.path.clone(),
            extensions: directory.extensions.clone(),
            visibility: directory.visibility.into(),
            upload: directory.upload.into(),
            roles: directory.roles(plugin_id)?,
        })
    }

    pub(crate) fn resolved_for_upload(&self) -> Self {
        let mut resolved = self.clone();
        resolved.path = render_path(&self.path);
        resolved
    }
}

fn render_path(template: &str) -> String {
    let now = Local::now();
    template
        .replace("{year}", &now.year().to_string())
        .replace("{month}", &format!("{:02}", now.month()))
}

fn validate_filename(filename: &str) -> Result<(), String> {
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

fn validate_stored_path(directory: &StorageDirectory, path: &str) -> Result<(), String> {
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

fn validate_extension(directory: &StorageDirectory, filename: &str) -> Result<(), String> {
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

fn write_temporary(root: &Path, bytes: &[u8]) -> Result<PathBuf, Error> {
    for _ in 0..100 {
        let sequence = TEMPORARY_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temporary = root.join(format!(
            ".nur-upload-{}-{sequence}.uploading",
            std::process::id()
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
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(Error::Io(error)),
        }
    }
    Err(Error::Io(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not allocate a temporary plugin storage file",
    )))
}

fn regular_file_size(path: &Path) -> Result<u64, Error> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() && !metadata.file_type().is_symlink() => {
            Ok(metadata.len())
        }
        Ok(_) => Err(Error::PluginBadRequest(
            "invalid plugin storage path".into(),
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(Error::Io(error)),
    }
}

fn storage_size(root: &Path) -> Result<u64, Error> {
    if !root.exists() {
        return Ok(0);
    }
    let root_metadata = fs::symlink_metadata(root).map_err(Error::Io)?;
    if root_metadata.file_type().is_symlink() || !root_metadata.file_type().is_dir() {
        return Err(Error::PluginBadRequest(
            "invalid plugin storage path".into(),
        ));
    }
    let mut size = 0u64;
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).map_err(Error::Io)? {
            let entry = entry.map_err(Error::Io)?;
            let file_type = entry.file_type().map_err(Error::Io)?;
            if file_type.is_symlink() || !file_type.is_file() && !file_type.is_dir() {
                return Err(Error::PluginBadRequest(
                    "invalid plugin storage path".into(),
                ));
            }
            let metadata = entry.metadata().map_err(Error::Io)?;
            if file_type.is_dir() {
                pending.push(entry.path());
            } else {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if !name.ends_with(".uploading.json") && !name.ends_with(".uploading.json.tmp") {
                    size = size.saturating_add(metadata.len());
                }
            }
        }
    }
    Ok(size)
}

fn safe_directory_path(base: &Path, relative: &Path, create: bool) -> Result<PathBuf, Error> {
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
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && create => {
                match fs::create_dir(&current) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
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
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(Error::PluginNotFound);
            }
            Err(error) => return Err(Error::Io(error)),
        }
    }
    Ok(current)
}

fn cleanup_incomplete_files(root: &Path, maximum_age: Duration) -> Result<(), Error> {
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
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(Error::Io(error)),
                }
            }
        }
    }
    Ok(())
}

fn ensure_regular_target(root: &Path, target: &Path) -> Result<(), Error> {
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

fn append_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}

fn percent_encode_path(path: &str) -> String {
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

impl From<ManifestStorageVisibility> for StorageVisibility {
    fn from(value: ManifestStorageVisibility) -> Self {
        match value {
            ManifestStorageVisibility::Public => Self::Public,
            ManifestStorageVisibility::Private => Self::Private,
        }
    }
}

impl From<ManifestStorageUpload> for BrowserUpload {
    fn from(value: ManifestStorageUpload) -> Self {
        match value {
            ManifestStorageUpload::None => Self::None,
            ManifestStorageUpload::Authenticated => Self::Authenticated,
            ManifestStorageUpload::Link => Self::Link,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        fs,
        sync::{Arc, Barrier, Mutex},
        time::SystemTime,
    };

    use super::{
        BrowserUpload, PluginStorage, StorageDirectory, StorageVisibility, validate_stored_path,
    };

    fn directory() -> StorageDirectory {
        StorageDirectory {
            plugin_id: "example".into(),
            id: "documents".into(),
            path: "documents/{year}/{month}".into(),
            extensions: vec!["pdf".into(), "docx".into(), "odt".into()],
            visibility: StorageVisibility::Public,
            upload: BrowserUpload::None,
            roles: vec!["admin".into()],
        }
    }

    fn temporary_storage() -> (PluginStorage, std::path::PathBuf) {
        let unique = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("system clock is after epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "nur-cms-plugin-storage-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("temporary storage is created");
        (
            PluginStorage {
                public_root: root.clone(),
                private_root: None,
                operation_locks: Arc::new(Mutex::new(HashMap::new())),
                plugin_sizes: Arc::new(Mutex::new(HashMap::new())),
            },
            root,
        )
    }

    #[test]
    fn concrete_paths_remain_valid_after_their_month_has_passed() {
        let directory = directory();

        assert!(validate_stored_path(&directory, "documents/2024/12/report.pdf").is_ok());
        assert!(validate_stored_path(&directory, "documents/2024/13/report.pdf").is_err());
        assert!(validate_stored_path(&directory, "documents/2024/12/report.html").is_err());
        assert!(validate_stored_path(&directory, "documents/../../report.pdf").is_err());
        let resolved = directory.resolved_for_upload();
        assert!(!resolved.path.contains("{year}"));
        assert!(!resolved.path.contains("{month}"));
    }

    #[test]
    fn enforces_the_per_plugin_quota() {
        let (storage, root) = temporary_storage();
        let result = storage.write(&directory(), "report.pdf", b"123456", 1024, 5);

        assert!(result.is_err());
        fs::remove_dir_all(root).expect("temporary storage is removed");
    }

    #[test]
    fn resumable_uploads_are_finalized_without_double_counting_temporary_bytes() {
        let (storage, root) = temporary_storage();
        let mut directory = directory();
        directory.path = "documents".into();
        let target = storage
            .prepare_resumable_upload(&directory, "report.pdf", 5, 5, 5)
            .expect("upload can be prepared");
        let temporary = super::append_suffix(&target, ".uploading");
        fs::write(&temporary, b"12345").expect("temporary upload can be written");

        let stored = storage
            .complete_resumable_upload(&directory, "report.pdf", &temporary, 5, 5)
            .expect("temporary upload can be finalized at the exact quota");
        assert_eq!(
            fs::read(&target).expect("stored file can be read"),
            b"12345"
        );
        assert!(stored.path.ends_with("documents/report.pdf"));

        storage
            .rollback_resumable_upload(&directory, "report.pdf", &temporary)
            .expect("finalization can be rolled back before link consumption");
        assert!(!target.exists());
        assert_eq!(
            fs::read(&temporary).expect("temporary file is restored"),
            b"12345"
        );
        fs::remove_dir_all(root).expect("temporary storage is removed");
    }

    #[test]
    fn resumable_uploads_reserve_quota_before_chunks_arrive() {
        let (storage, root) = temporary_storage();
        let mut directory = directory();
        directory.path = "documents".into();

        storage
            .prepare_resumable_upload(&directory, "first.pdf", 6, 10, 10)
            .expect("first upload reserves its declared size");
        assert!(
            storage
                .prepare_resumable_upload(&directory, "second.pdf", 5, 10, 10)
                .is_err(),
            "a second reservation cannot exceed the plugin quota"
        );
        fs::remove_dir_all(root).expect("temporary storage is removed");
    }

    #[test]
    fn finalized_resumable_uploads_can_be_recovered() {
        let (storage, root) = temporary_storage();
        let mut directory = directory();
        directory.path = "documents".into();
        let target = storage
            .prepare_resumable_upload(&directory, "report.pdf", 5, 5, 10)
            .expect("upload can be prepared");
        let temporary = super::append_suffix(&target, ".uploading");
        fs::write(&temporary, b"12345").expect("temporary upload can be written");
        storage
            .complete_resumable_upload(&directory, "report.pdf", &temporary, 5, 10)
            .expect("upload can be finalized");

        let (stored, recovered_target) = storage
            .recover_resumable_upload(&directory, "report.pdf", 5)
            .expect("recovery check succeeds")
            .expect("finalized upload is recognized");
        assert_eq!(recovered_target, target);
        assert_eq!(stored.path, "documents/report.pdf");
        fs::remove_dir_all(root).expect("temporary storage is removed");
    }

    #[test]
    fn stale_resumable_upload_artifacts_are_removed() {
        let (storage, root) = temporary_storage();
        let mut directory = directory();
        directory.path = "documents".into();
        let target = storage
            .prepare_resumable_upload(&directory, "report.pdf", 5, 5, 10)
            .expect("upload can be prepared");
        let temporary = super::append_suffix(&target, ".uploading");
        let metadata = super::append_suffix(&temporary, ".json");
        fs::write(&temporary, b"12").expect("partial upload can be written");
        fs::write(&metadata, b"{}").expect("resume metadata can be written");

        storage
            .cleanup_incomplete_uploads(std::time::Duration::ZERO)
            .expect("stale upload cleanup succeeds");

        assert!(!temporary.exists());
        assert!(!metadata.exists());
        fs::remove_dir_all(root).expect("temporary storage is removed");
    }

    #[cfg(unix)]
    #[test]
    fn storage_rejects_symlinks_in_directory_paths() {
        use std::os::unix::fs::symlink;

        let (storage, root) = temporary_storage();
        let external = root.with_extension("outside");
        fs::create_dir_all(root.join("example")).expect("plugin namespace can be created");
        fs::create_dir_all(&external).expect("external directory can be created");
        symlink(&external, root.join("example/documents")).expect("test symlink can be created");
        let mut directory = directory();
        directory.path = "documents".into();

        assert!(
            storage
                .write(&directory, "report.pdf", b"contents", 1024, 1024)
                .is_err()
        );
        assert!(!external.join("report.pdf").exists());
        fs::remove_dir_all(root).expect("temporary storage is removed");
        fs::remove_dir_all(external).expect("external directory is removed");
    }

    #[cfg(unix)]
    #[test]
    fn concurrent_writes_do_not_share_a_temporary_file() {
        let (storage, root) = temporary_storage();
        let storage = Arc::new(storage);
        let barrier = Arc::new(Barrier::new(2));
        let mut workers = Vec::new();
        for contents in [b"first".as_slice(), b"second".as_slice()] {
            let storage = Arc::clone(&storage);
            let barrier = Arc::clone(&barrier);
            let contents = contents.to_vec();
            workers.push(std::thread::spawn(move || {
                barrier.wait();
                storage.write(&directory(), "report.pdf", &contents, 1024, 1024)
            }));
        }
        for worker in workers {
            worker
                .join()
                .expect("storage worker does not panic")
                .expect("concurrent write succeeds");
        }

        let stored = storage
            .write(&directory(), "final.pdf", b"final", 1024, 1024)
            .expect("final write succeeds");
        assert!(stored.path.ends_with("/final.pdf"));
        let namespace = root.join("example");
        assert!(
            fs::read_dir(namespace.join(super::render_path(&directory().path)))
                .expect("storage directory can be read")
                .all(|entry| !entry
                    .expect("valid entry")
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".uploading"))
        );
        fs::remove_dir_all(root).expect("temporary storage is removed");
    }
}
