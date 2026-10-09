use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use serde::Serialize;

use nur_core::config::settings;

use self::filesystem::{
    append_suffix, cleanup_incomplete_files, ensure_regular_target, percent_encode_path,
    regular_file_size, render_path, safe_directory_path, validate_file, validate_stored_path,
    write_temporary,
};
use self::quota::storage_lock_error;
use crate::{
    Error,
    manifest::{
        StorageDirectoryManifest, StorageUpload as ManifestStorageUpload,
        StorageVisibility as ManifestStorageVisibility, valid_plugin_id, valid_storage_extension,
        valid_storage_id, valid_storage_path,
    },
};

mod filesystem;
mod quota;

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

#[derive(Clone, Debug, Serialize)]
pub struct StoredFile {
    pub path: String,
    pub public_url: Option<String>,
}

impl PluginStorage {
    pub fn from_config() -> Result<Self, Error> {
        let config = settings();

        Self::from_roots(
            config.uploads.directory.clone(),
            config.plugins.storage.private_directory.clone(),
        )
    }

    fn from_roots(storage: PathBuf, private_root: Option<PathBuf>) -> Result<Self, Error> {
        fs::create_dir_all(&storage).map_err(Error::Io)?;
        let upload_root = fs::canonicalize(storage).map_err(Error::Io)?;
        let public_root = upload_root.join("p");

        fs::create_dir_all(&public_root).map_err(Error::Io)?;
        let public_root = fs::canonicalize(public_root).map_err(Error::Io)?;
        let private_root = private_root
            .map(|root| {
                fs::create_dir_all(&root).map_err(Error::Io)?;
                fs::canonicalize(root).map_err(Error::Io)
            })
            .transpose()?;

        if private_root.as_ref().is_some_and(|private_root| {
            private_root.starts_with(&upload_root)
                || upload_root.starts_with(private_root)
                || private_root.starts_with(&public_root)
                || public_root.starts_with(private_root)
        }) {
            return Err(Error::Manifest(
                "plugins.storage.private_directory and uploads.directory must be separate, non-overlapping directories"
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
                "plugin '{}' declares private storage '{}', but plugins.storage.private_directory is not configured",
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
        validate_file(directory, filename)?;

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

        validate_file(directory, filename)?;

        let operation_lock = self.operation_lock(&directory.plugin_id)?;
        let _operation = operation_lock.lock().map_err(|_| storage_lock_error())?;

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
        let _operation = operation_lock.lock().map_err(|_| storage_lock_error())?;

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
            if error.kind() == ErrorKind::NotFound {
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

        validate_file(directory, filename)?;

        let operation_lock = self.operation_lock(&directory.plugin_id)?;
        let _operation = operation_lock.lock().map_err(|_| storage_lock_error())?;

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
        validate_file(directory, filename)?;

        let operation_lock = self.operation_lock(&directory.plugin_id)?;
        let _operation = operation_lock.lock().map_err(|_| storage_lock_error())?;

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
        validate_file(directory, filename)?;

        let operation_lock = self.operation_lock(&directory.plugin_id)?;
        let _operation = operation_lock.lock().map_err(|_| storage_lock_error())?;

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
        let _operation = operation_lock.lock().map_err(|_| storage_lock_error())?;

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
                    "/uploads/p/{}/{}",
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
            StorageVisibility::Private => self.private_root.as_ref().ok_or_else(|| {
                Error::Manifest("plugins.storage.private_directory is not configured".into())
            })?,
        })
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
mod tests;
