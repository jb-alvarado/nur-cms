use std::{
    fs,
    io::Error as IoError,
    path::Path,
    sync::{Arc, Mutex},
};

use super::PluginStorage;
use crate::Error;

impl PluginStorage {
    pub(super) fn scan_plugin_size(&self, plugin_id: &str) -> Result<u64, Error> {
        let public = storage_size(&self.public_root.join(plugin_id))?;
        let private = self
            .private_root
            .as_ref()
            .map(|root| storage_size(&root.join(plugin_id)))
            .transpose()?
            .unwrap_or(0);
        Ok(public.saturating_add(private))
    }

    pub(super) fn cached_plugin_size(&self, plugin_id: &str) -> Result<u64, Error> {
        if let Some(size) = self
            .plugin_sizes
            .lock()
            .map_err(|_| storage_lock_error())?
            .get(plugin_id)
            .copied()
        {
            return Ok(size);
        }

        let size = self.scan_plugin_size(plugin_id)?;
        self.set_cached_plugin_size(plugin_id, size)?;

        Ok(size)
    }

    pub(super) fn set_cached_plugin_size(&self, plugin_id: &str, size: u64) -> Result<(), Error> {
        self.plugin_sizes
            .lock()
            .map_err(|_| storage_lock_error())?
            .insert(plugin_id.to_owned(), size);

        Ok(())
    }

    pub(super) fn operation_lock(&self, plugin_id: &str) -> Result<Arc<Mutex<()>>, Error> {
        let mut locks = self
            .operation_locks
            .lock()
            .map_err(|_| storage_lock_error())?;

        Ok(Arc::clone(
            locks
                .entry(plugin_id.to_owned())
                .or_insert_with(|| Arc::new(Mutex::new(()))),
        ))
    }
}

pub(super) fn storage_lock_error() -> Error {
    Error::Io(IoError::other("plugin storage lock is poisoned"))
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
