use sha2::{Digest, Sha256};
use tracing::error;
use uuid::Uuid;

use super::{HostState, PluginResult};
use crate::{
    Error,
    db::handles::file_links::{self, NewFileLink},
    runtime::bindings::{self, nur::cms::types::PluginError},
    storage::{BrowserUpload, PluginStorage, StorageDirectory, StorageVisibility},
};

const MIN_FILE_LINK_EXPIRY_SECONDS: u32 = 60;

fn maximum_file_link_expiry_seconds() -> u32 {
    nur_core::config::settings()
        .plugins
        .storage
        .file_links_max_age_hours
        * 60
        * 60
}

fn valid_link_expiry(expires_seconds: u32) -> PluginResult<i32> {
    if !(MIN_FILE_LINK_EXPIRY_SECONDS..=maximum_file_link_expiry_seconds())
        .contains(&expires_seconds)
    {
        return Err(PluginError::BadRequest("invalid file link expiry".into()));
    }

    i32::try_from(expires_seconds)
        .map_err(|_| PluginError::BadRequest("invalid file link expiry".into()))
}

impl bindings::nur::cms::storage::Host for HostState {
    fn write(
        &mut self,
        directory: String,
        filename: String,
        contents: Vec<u8>,
    ) -> PluginResult<bindings::nur::cms::storage::File> {
        self.consume_host_call()?;
        let directory = self.storage_directory(&directory)?;
        let storage = self.plugin_storage()?;

        let stored = storage
            .write(
                directory,
                &filename,
                &contents,
                self.storage_write_limit,
                self.storage_quota,
            )
            .map_err(|error| self.storage_error(error))?;

        Ok(bindings::nur::cms::storage::File {
            path: stored.path,
            public_url: stored.public_url,
        })
    }

    fn delete(&mut self, directory: String, path: String) -> PluginResult<()> {
        self.consume_host_call()?;
        let directory = self.storage_directory(&directory)?;
        let storage = self.plugin_storage()?;

        storage
            .delete(directory, &path)
            .map_err(|error| self.storage_error(error))
    }

    fn create_upload_link(
        &mut self,
        request: bindings::nur::cms::storage::UploadLinkRequest,
    ) -> PluginResult<bindings::nur::cms::storage::Link> {
        self.consume_host_call()?;
        let directory = self.storage_directory(&request.directory)?;

        if directory.upload != BrowserUpload::Link {
            return Err(PluginError::Forbidden);
        }

        let storage = self.plugin_storage()?;

        storage
            .validate_upload_request(
                directory,
                &request.filename,
                request.max_size,
                self.storage_quota,
            )
            .map_err(|error| self.storage_error(error))?;

        let maximum_size = i64::try_from(request.max_size)
            .map_err(|_| PluginError::BadRequest("invalid upload size".into()))?;
        let expires_seconds = valid_link_expiry(request.expires_seconds)?;

        self.create_file_link(
            "upload",
            directory,
            request.filename,
            Some(maximum_size),
            expires_seconds,
        )
    }

    fn create_download_link(
        &mut self,
        request: bindings::nur::cms::storage::DownloadLinkRequest,
    ) -> PluginResult<bindings::nur::cms::storage::Link> {
        self.consume_host_call()?;
        let directory = self.storage_directory(&request.directory)?;

        if directory.visibility != StorageVisibility::Private {
            return Err(PluginError::Forbidden);
        }

        let storage = self.plugin_storage()?;

        storage
            .file_path(directory, &request.path)
            .map_err(|error| self.storage_error(error))?;

        self.create_file_link(
            "download",
            directory,
            request.path,
            None,
            valid_link_expiry(request.expires_seconds)?,
        )
    }
}

impl HostState {
    fn create_file_link(
        &self,
        purpose: &str,
        directory: &StorageDirectory,
        path: String,
        maximum_size: Option<i64>,
        expires_seconds: i32,
    ) -> PluginResult<bindings::nur::cms::storage::Link> {
        let token = Uuid::new_v4().simple().to_string();
        let token_hash = Sha256::digest(token.as_bytes()).to_vec();
        let plugin_id = self.plugin_id.clone();
        let directory_id = directory.id.clone();
        let purpose = purpose.to_owned();

        let result = self.tokio_handle.block_on(async {
            tokio::time::timeout(
                self.host_call_timeout,
                file_links::create(
                    &self.pool,
                    NewFileLink {
                        plugin_id: &plugin_id,
                        directory_id: &directory_id,
                        purpose: &purpose,
                        token_hash,
                        filename: path,
                        maximum_size,
                        expires_seconds,
                    },
                ),
            )
            .await
        });

        match result {
            Ok(Ok(_)) => Ok(bindings::nur::cms::storage::Link {
                url: format!("/api/p/{plugin_id}/files/{purpose}/{token}"),
            }),
            Ok(Err(error)) => {
                error!(plugin = %self.plugin_id, %error, "plugin file link creation failed");
                Err(PluginError::Failed("file link creation failed".into()))
            }
            Err(_) => Err(PluginError::Failed("file link creation timed out".into())),
        }
    }

    fn storage_directory(&self, id: &str) -> PluginResult<&StorageDirectory> {
        self.storage_directories
            .iter()
            .find(|directory| directory.id == id)
            .ok_or(PluginError::Forbidden)
    }

    fn plugin_storage(&self) -> PluginResult<&PluginStorage> {
        self.storage
            .as_ref()
            .ok_or_else(|| PluginError::Failed("plugin storage is unavailable".into()))
    }

    fn storage_error(&self, error: Error) -> PluginError {
        match error {
            Error::PluginBadRequest(_) => PluginError::BadRequest("invalid storage request".into()),
            Error::PluginNotFound => PluginError::NotFound,
            Error::Manifest(_) | Error::Io(_) => {
                error!(plugin = %self.plugin_id, %error, "plugin storage operation failed");
                PluginError::Failed("storage operation failed".into())
            }
            _ => PluginError::Failed("storage operation failed".into()),
        }
    }
}
