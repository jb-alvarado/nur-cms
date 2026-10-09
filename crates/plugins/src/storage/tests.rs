use std::{
    collections::HashMap,
    env, fs,
    path::PathBuf,
    process,
    sync::{Arc, Barrier, Mutex},
    thread,
    time::{Duration, SystemTime},
};

#[cfg(unix)]
use std::os::unix::fs::symlink;

use super::{
    BrowserUpload, PluginStorage, StorageDirectory, StorageVisibility, append_suffix, render_path,
    validate_stored_path,
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

fn temporary_storage() -> (PluginStorage, PathBuf) {
    let unique = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("system clock is after epoch")
        .as_nanos();
    let root = env::temp_dir().join(format!("nur-cms-plugin-storage-{}-{unique}", process::id()));
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
fn private_storage_must_not_overlap_the_entire_upload_root() {
    let (_, root) = temporary_storage();
    let uploads = root.join("uploads");
    for private in [
        uploads.clone(),
        uploads.join("private"),
        uploads.join("p/private"),
        root.clone(),
    ] {
        assert!(PluginStorage::from_roots(uploads.clone(), Some(private)).is_err());
    }
    assert!(PluginStorage::from_roots(uploads, Some(root.join("uploads-private"))).is_ok());
    fs::remove_dir_all(root).expect("test storage is removed");
}

#[cfg(unix)]
#[test]
fn private_storage_symlinks_cannot_point_into_uploads() {
    let (_, root) = temporary_storage();
    let uploads = root.join("uploads");
    fs::create_dir_all(uploads.join("private")).unwrap();
    let alias = root.join("private-alias");
    symlink(uploads.join("private"), &alias).unwrap();
    assert!(PluginStorage::from_roots(uploads, Some(alias)).is_err());
    fs::remove_dir_all(root).expect("test storage is removed");
}

#[cfg(unix)]
#[test]
fn private_storage_cannot_overlap_a_symlinked_public_plugin_root() {
    let (_, root) = temporary_storage();
    let uploads = root.join("uploads");
    let external = root.join("external");
    fs::create_dir_all(&uploads).unwrap();
    fs::create_dir_all(&external).unwrap();
    symlink(&external, uploads.join("p")).unwrap();
    assert!(PluginStorage::from_roots(uploads, Some(external.join("private"))).is_err());
    fs::remove_dir_all(root).expect("test storage is removed");
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
    let temporary = append_suffix(&target, ".uploading");
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
    let temporary = append_suffix(&target, ".uploading");
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
    let temporary = append_suffix(&target, ".uploading");
    let metadata = append_suffix(&temporary, ".json");
    fs::write(&temporary, b"12").expect("partial upload can be written");
    fs::write(&metadata, b"{}").expect("resume metadata can be written");

    storage
        .cleanup_incomplete_uploads(Duration::ZERO)
        .expect("stale upload cleanup succeeds");

    assert!(!temporary.exists());
    assert!(!metadata.exists());
    fs::remove_dir_all(root).expect("temporary storage is removed");
}

#[cfg(unix)]
#[test]
fn storage_rejects_symlinks_in_directory_paths() {
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
        workers.push(thread::spawn(move || {
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
        fs::read_dir(namespace.join(render_path(&directory().path)))
            .expect("storage directory can be read")
            .all(|entry| !entry
                .expect("valid entry")
                .file_name()
                .to_string_lossy()
                .ends_with(".uploading"))
    );
    fs::remove_dir_all(root).expect("temporary storage is removed");
}
