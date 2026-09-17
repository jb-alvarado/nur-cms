use std::{collections::BTreeMap, env, process};

use super::{
    AdminManifest, AdminMenuItem, CacheManifest, MailManifest, Manifest, RouteManifest, RouteScope,
    StorageDirectoryManifest, StorageManifest, StorageUpload, StorageVisibility, contained_path,
    discovery::validate_asset_tree,
    schema_name, valid_plugin_id,
    validation::{
        valid_admin_entry, valid_admin_menu_item, valid_admin_style, valid_custom_element_name,
        valid_plugin_name, valid_route_id, validate_mail_permissions, validate_manifest,
        validate_storage,
    },
};

#[test]
fn missing_plugin_paths_include_context() {
    let root = env::temp_dir();
    let relative = format!("nur-cms-missing-plugin-path-{}", process::id());
    let error = contained_path(&root, &relative, "example", "module")
        .expect_err("missing path is rejected")
        .to_string();

    assert!(error.contains("plugin 'example' module"));
    assert!(error.contains(&root.join(relative).display().to_string()));
}

fn route(access: &str) -> RouteManifest {
    RouteManifest {
        id: "test".into(),
        method: "GET".into(),
        path: "/".into(),
        scope: RouteScope::Plugin,
        access: access.into(),
        cache: None,
    }
}

#[test]
fn accepts_single_and_multiple_roles() {
    assert_eq!(route("author").roles().unwrap(), ["author"]);
    assert_eq!(route("admin,author").roles().unwrap(), ["admin", "author"]);
    assert!(route("public").roles().unwrap().is_empty());
}

#[test]
fn rejects_public_combined_with_roles() {
    assert!(route("public,author").roles().is_err());
}

#[test]
fn admin_entries_and_custom_element_names_are_restricted() {
    assert!(valid_admin_entry("admin/echo.js"));
    assert!(valid_admin_entry("admin/echo.mjs"));
    assert!(!valid_admin_entry("admin/echo.css"));
    assert!(!valid_admin_entry("../echo.js"));
    assert!(valid_admin_style("admin/echo.css"));
    assert!(!valid_admin_style("admin/echo.js"));
    assert!(valid_custom_element_name("nur-cms-echo"));
    assert!(!valid_custom_element_name("NurCmsEcho"));
    assert!(!valid_custom_element_name("font-face"));
}

#[test]
fn admin_access_requires_authenticated_roles() {
    let mut admin = AdminManifest {
        entry: Some("admin.js".into()),
        element: Some("nur-cms-test".into()),
        access: "admin,author".into(),
        styles: Vec::new(),
        menu: Vec::new(),
    };
    assert_eq!(admin.roles("test").unwrap(), ["admin", "author"]);

    admin.access = "public".into();
    assert!(admin.roles("test").is_err());
}

#[test]
fn admin_menu_uses_safe_relative_paths() {
    let mut item = AdminMenuItem {
        label: "Example".into(),
        labels: BTreeMap::new(),
        path: "/settings".into(),
        icon: Some("bi-puzzle".into()),
        access: None,
    };
    assert!(valid_admin_menu_item(&item));

    item.path = "/admin/plugins/example/settings".into();
    assert!(!valid_admin_menu_item(&item));
    item.path = "/admin/p/example/settings".into();
    assert!(!valid_admin_menu_item(&item));
    item.path = "configuration".into();
    assert!(!valid_admin_menu_item(&item));
    item.path = "/settings/../other".into();
    assert!(!valid_admin_menu_item(&item));
}

#[test]
fn admin_menu_access_inherits_and_cannot_expand_admin_access() {
    let mut admin = AdminManifest {
        entry: Some("admin.js".into()),
        element: Some("nur-cms-test".into()),
        access: "admin,stat".into(),
        styles: Vec::new(),
        menu: vec![AdminMenuItem {
            label: "Products".into(),
            labels: BTreeMap::from([("de".into(), "Produkte".into())]),
            path: "/products".into(),
            icon: None,
            access: None,
        }],
    };

    assert_eq!(
        admin.menu_roles(&admin.menu[0], "example").unwrap(),
        ["admin", "stat"]
    );
    assert!(admin.validate_menu_access("example").is_ok());

    admin.menu[0].access = Some("author".into());
    assert!(admin.validate_menu_access("example").is_err());

    admin.menu[0].access = Some("invalid role".into());
    assert!(admin.validate_menu_access("example").is_err());
}

#[test]
fn echo_example_uses_a_valid_current_manifest() {
    let manifest: Manifest =
        toml_edit::de::from_str(include_str!("../../examples/echo/plugin.toml"))
            .expect("echo manifest can be deserialized");

    validate_manifest(&manifest).expect("echo manifest is valid");
}

#[test]
fn vue_admin_example_uses_a_valid_current_manifest() {
    let manifest: Manifest =
        toml_edit::de::from_str(include_str!("../../examples/vue-admin/plugin.toml"))
            .expect("Vue admin manifest can be deserialized");

    validate_manifest(&manifest).expect("Vue admin manifest is valid");
}

#[test]
fn mail_permissions_are_explicit_and_dynamic_targets_are_a_subset() {
    let permissions = MailManifest {
        targets: vec!["contact".into(), "orders".into()],
        dynamic_recipient_targets: vec!["orders".into()],
        trusted_template_targets: vec!["orders".into()],
    };
    assert!(validate_mail_permissions(&permissions, "example").is_ok());

    let undeclared_dynamic_target = MailManifest {
        targets: vec!["contact".into()],
        dynamic_recipient_targets: vec!["orders".into()],
        trusted_template_targets: Vec::new(),
    };
    assert!(validate_mail_permissions(&undeclared_dynamic_target, "example").is_err());

    let duplicate_target = MailManifest {
        targets: vec!["contact".into(), "contact".into()],
        dynamic_recipient_targets: Vec::new(),
        trusted_template_targets: Vec::new(),
    };
    assert!(validate_mail_permissions(&duplicate_target, "example").is_err());

    let undeclared_trusted_template_target = MailManifest {
        targets: vec!["contact".into()],
        dynamic_recipient_targets: Vec::new(),
        trusted_template_targets: vec!["orders".into()],
    };
    assert!(validate_mail_permissions(&undeclared_trusted_template_target, "example").is_err());

    let duplicate_trusted_template_target = MailManifest {
        targets: vec!["contact".into()],
        dynamic_recipient_targets: Vec::new(),
        trusted_template_targets: vec!["contact".into(), "contact".into()],
    };
    assert!(validate_mail_permissions(&duplicate_trusted_template_target, "example").is_err());
}

#[test]
fn plugin_ids_produce_distinct_safe_schema_names() {
    assert!(valid_plugin_id("my-plugin"));
    assert!(!valid_plugin_id("my_plugin"));
    assert_eq!(schema_name("my-plugin"), "nur_plugin_my_plugin");
}

#[test]
fn plugin_display_names_are_bounded_and_safe_for_the_menu() {
    assert!(valid_plugin_name("Community Site"));
    assert!(!valid_plugin_name(" Community Site"));
    assert!(!valid_plugin_name("line\nbreak"));
    assert!(!valid_plugin_name(&"x".repeat(81)));
}

#[test]
fn route_ids_are_bounded_and_log_safe() {
    assert!(valid_route_id("public-feed.v1"));
    assert!(!valid_route_id("line\nbreak"));
    assert!(!valid_route_id(&"x".repeat(81)));
}

#[test]
fn storage_directories_keep_uploads_and_visibility_separate() {
    let storage = StorageManifest {
        directories: vec![
            StorageDirectoryManifest {
                id: "public-exports".into(),
                path: "exports/{year}/{month}".into(),
                extensions: vec!["pdf".into(), "docx".into()],
                visibility: StorageVisibility::Public,
                upload: StorageUpload::None,
                access: "admin,author".into(),
            },
            StorageDirectoryManifest {
                id: "customer-files".into(),
                path: "customer-files".into(),
                extensions: vec!["odt".into()],
                visibility: StorageVisibility::Private,
                upload: StorageUpload::Link,
                access: "admin".into(),
            },
        ],
    };

    validate_storage(&storage, "example").expect("storage manifest is valid");
}

#[test]
fn storage_directories_reject_unsafe_paths_and_duplicates() {
    let unsafe_path = StorageManifest {
        directories: vec![StorageDirectoryManifest {
            id: "files".into(),
            path: "../files".into(),
            extensions: vec!["pdf".into()],
            visibility: StorageVisibility::Private,
            upload: StorageUpload::Authenticated,
            access: "admin".into(),
        }],
    };
    assert!(validate_storage(&unsafe_path, "example").is_err());

    let duplicate_id = StorageManifest {
        directories: vec![
            StorageDirectoryManifest {
                id: "files".into(),
                path: "one".into(),
                extensions: vec!["pdf".into()],
                visibility: StorageVisibility::Public,
                upload: StorageUpload::None,
                access: "admin".into(),
            },
            StorageDirectoryManifest {
                id: "files".into(),
                path: "two".into(),
                extensions: vec!["pdf".into()],
                visibility: StorageVisibility::Public,
                upload: StorageUpload::None,
                access: "admin".into(),
            },
        ],
    };
    assert!(validate_storage(&duplicate_id, "example").is_err());

    let active_content = StorageManifest {
        directories: vec![StorageDirectoryManifest {
            id: "files".into(),
            path: "files".into(),
            extensions: vec!["html".into(), "svg".into()],
            visibility: StorageVisibility::Public,
            upload: StorageUpload::None,
            access: "admin".into(),
        }],
    };
    assert!(validate_storage(&active_content, "example").is_err());
}

#[cfg(unix)]
#[test]
fn asset_trees_reject_symbolic_links() {
    use std::{fs, os::unix::fs::symlink, time::SystemTime};

    let unique = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("system clock is after epoch")
        .as_nanos();
    let root = env::temp_dir().join(format!("nur-cms-plugin-assets-{}-{unique}", process::id()));
    fs::create_dir_all(&root).expect("test asset directory is created");
    fs::write(root.join("site.css"), "body {}").expect("regular test asset is created");
    assert!(validate_asset_tree(&root, "example").is_ok());

    symlink("/etc/passwd", root.join("outside")).expect("test symbolic link is created");
    assert!(validate_asset_tree(&root, "example").is_err());

    fs::remove_dir_all(root).expect("test asset directory is removed");
}

#[test]
fn plugin_cache_has_bounded_settings() {
    let cache = CacheManifest {
        ttl_seconds: 300,
        max_entries: 128,
    };
    assert!((1..=86_400).contains(&cache.ttl_seconds));
    assert!((1..=10_000).contains(&cache.max_entries));
}

#[test]
fn caches_only_eligible_routes_by_default() {
    assert!(route("public").cache_enabled(true).unwrap());
    assert!(!route("author").cache_enabled(true).unwrap());

    let mut post_route = route("public");
    post_route.method = "POST".into();
    assert!(!post_route.cache_enabled(true).unwrap());

    let mut protected_cached = route("author");
    protected_cached.cache = Some(true);
    assert!(protected_cached.cache_enabled(true).is_err());
}
