use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    net::{IpAddr, Ipv4Addr},
    path::PathBuf,
    sync::{Arc, Barrier, Mutex},
    thread,
    time::{Duration, Instant},
};

use tokio::sync::Semaphore;

use super::{
    bindings,
    component::{PluginComponent, Runtime, acquire_runtime_permit},
    mail_limiter::{
        MailPermissions, PublicMailRateLimiter, allow_mail_for_request, allow_public_mail,
        allow_public_mail_for_client,
    },
};
use crate::Error;

#[test]
fn rejects_work_when_runtime_capacity_is_exhausted() {
    let semaphore = Arc::new(Semaphore::new(1));
    let _permit = acquire_runtime_permit(Arc::clone(&semaphore)).expect("capacity is available");

    assert!(matches!(
        acquire_runtime_permit(semaphore),
        Err(Error::Busy)
    ));
}

#[test]
fn mail_permissions_are_scoped_by_target_and_recipient_mode() {
    let permissions = MailPermissions {
        targets: Arc::new(HashSet::from(["contact".into(), "orders".into()])),
        dynamic_recipient_targets: Arc::new(HashSet::from(["orders".into()])),
        trusted_template_targets: Arc::new(HashSet::from(["contact".into()])),
    };

    assert!(permissions.allows("contact", false, true));
    assert!(!permissions.allows("contact", true, true));
    assert!(permissions.allows("orders", true, false));
    assert!(!permissions.allows("orders", false, true));
    assert!(!permissions.allows("unknown", false, false));
}

#[test]
fn public_mail_is_limited_once_per_ip() {
    let mut limiter = PublicMailRateLimiter {
        sent: HashMap::new(),
        expirations: VecDeque::new(),
        window: Duration::from_secs(180),
        max_clients: 10_000,
    };
    let now = Instant::now();
    let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);

    assert!(allow_public_mail(&mut limiter, "echo", "mail", ip, now));
    assert!(!allow_public_mail(&mut limiter, "echo", "mail", ip, now));
    assert!(allow_public_mail(
        &mut limiter,
        "other-plugin",
        "mail",
        ip,
        now,
    ));
    assert!(allow_public_mail(
        &mut limiter,
        "echo",
        "other-mail-route",
        ip,
        now,
    ));
    assert!(allow_public_mail(
        &mut limiter,
        "echo",
        "mail",
        IpAddr::V4(Ipv4Addr::new(192, 0, 2, 10)),
        now,
    ));
    assert!(allow_public_mail(
        &mut limiter,
        "echo",
        "mail",
        ip,
        now + Duration::from_secs(180),
    ));
}

#[test]
fn one_request_can_send_three_messages_after_one_public_reservation() {
    let mut remaining = 3;
    let mut authorized = None;
    let mut reservations = 0;

    for _ in 0..3 {
        assert!(allow_mail_for_request(
            &mut remaining,
            true,
            &mut authorized,
            || {
                reservations += 1;
                true
            },
        ));
    }

    assert!(!allow_mail_for_request(
        &mut remaining,
        true,
        &mut authorized,
        || true,
    ));
    assert_eq!(reservations, 1);
}

#[test]
fn protected_routes_have_the_same_per_request_mail_limit() {
    let mut remaining = 3;
    let mut authorized = None;

    for _ in 0..3 {
        assert!(allow_mail_for_request(
            &mut remaining,
            false,
            &mut authorized,
            || false,
        ));
    }

    assert!(!allow_mail_for_request(
        &mut remaining,
        false,
        &mut authorized,
        || false,
    ));
}

#[test]
fn rejected_public_reservation_is_reused_without_retrying_the_limiter() {
    let mut remaining = 3;
    let mut authorized = None;
    let mut reservations = 0;

    for _ in 0..2 {
        assert!(!allow_mail_for_request(
            &mut remaining,
            true,
            &mut authorized,
            || {
                reservations += 1;
                false
            },
        ));
    }

    assert_eq!(reservations, 1);
}

#[test]
fn parallel_requests_cannot_bypass_the_public_limit() {
    let limiter = Arc::new(Mutex::new(PublicMailRateLimiter {
        sent: HashMap::new(),
        expirations: VecDeque::new(),
        window: Duration::from_secs(180),
        max_clients: 10_000,
    }));
    let barrier = Arc::new(Barrier::new(8));
    let now = Instant::now();
    let handles = (0..8)
        .map(|_| {
            let limiter = Arc::clone(&limiter);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                allow_public_mail(
                    &mut limiter.lock().expect("limiter lock is available"),
                    "echo",
                    "mail",
                    IpAddr::V4(Ipv4Addr::LOCALHOST),
                    now,
                )
            })
        })
        .collect::<Vec<_>>();
    let allowed = handles
        .into_iter()
        .map(|handle| handle.join().expect("limiter worker completes"))
        .filter(|allowed| *allowed)
        .count();

    assert_eq!(allowed, 1);
}

#[test]
fn public_mail_limiter_fails_closed_at_capacity() {
    let mut limiter = PublicMailRateLimiter {
        sent: HashMap::new(),
        expirations: VecDeque::new(),
        window: Duration::from_secs(180),
        max_clients: 1,
    };
    let now = Instant::now();

    assert!(allow_public_mail(
        &mut limiter,
        "echo",
        "mail",
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        now,
    ));
    assert!(!allow_public_mail(
        &mut limiter,
        "echo",
        "mail",
        IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)),
        now,
    ));
    assert!(allow_public_mail(
        &mut limiter,
        "echo",
        "mail",
        IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)),
        now + Duration::from_secs(180),
    ));
}

#[test]
fn public_mail_fails_closed_without_a_client_ip() {
    let limiter = Arc::new(Mutex::new(PublicMailRateLimiter {
        sent: HashMap::new(),
        expirations: VecDeque::new(),
        window: Duration::from_secs(180),
        max_clients: 10_000,
    }));
    let now = Instant::now();

    assert!(!allow_public_mail_for_client(
        &limiter, "echo", "mail", None, now,
    ));
    assert!(allow_public_mail_for_client(
        &limiter,
        "echo",
        "mail",
        Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
        now,
    ));
}

#[tokio::test]
#[ignore = "requires a prebuilt wasm32-wasip2 echo example"]
async fn invokes_built_echo_component_when_available() {
    let module = example_component("echo", "nur_cms_echo_plugin");
    let pool =
        sqlx::PgPool::connect_lazy("postgres://localhost/nur_cms").expect("test pool initializes");
    let runtime = Runtime::new(pool, None).expect("runtime initializes");
    let component = wasmtime::component::Component::from_file(&runtime.engine, module)
        .expect("example component loads");
    let plugin = PluginComponent {
        id: "echo".into(),
        component,
        runtime,
        mail_permissions: Default::default(),
        storage_directories: Arc::new(Vec::new()),
    };
    let response = plugin
        .call(
            bindings::nur::cms::types::Request {
                route_id: "root".into(),
                method: "GET".into(),
                path: "/plugin-echo".into(),
                path_params: Vec::new(),
                query: None,
                headers: Vec::new(),
                body: Vec::new(),
                identity: None,
            },
            true,
            Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
        )
        .await
        .expect("example request succeeds");

    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"Hello from a nur-cms root plugin route");
}

#[tokio::test]
#[ignore = "requires a prebuilt wasm32-wasip2 community-site example"]
async fn loads_a_component_with_the_content_import_when_available() {
    let module = example_component("community-site", "nur_cms_community_site_plugin");
    let pool =
        sqlx::PgPool::connect_lazy("postgres://localhost/nur_cms").expect("test pool initializes");
    let runtime = Runtime::new(pool, None).expect("runtime initializes");
    let component = wasmtime::component::Component::from_file(&runtime.engine, module)
        .expect("example component loads");
    let plugin = PluginComponent {
        id: "community-site".into(),
        component,
        runtime,
        mail_permissions: Default::default(),
        storage_directories: Arc::new(Vec::new()),
    };
    let result = plugin
        .call(
            bindings::nur::cms::types::Request {
                route_id: "missing".into(),
                method: "GET".into(),
                path: "/missing".into(),
                path_params: Vec::new(),
                query: None,
                headers: Vec::new(),
                body: Vec::new(),
                identity: None,
            },
            true,
            Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
        )
        .await;

    assert!(matches!(result, Err(Error::PluginNotFound)));
}

#[tokio::test]
#[ignore = "requires a prebuilt wasm32-wasip2 Vue admin example"]
async fn invokes_built_vue_admin_component_when_available() {
    let module = example_component("vue-admin", "nur_cms_vue_admin_plugin");
    let pool =
        sqlx::PgPool::connect_lazy("postgres://localhost/nur_cms").expect("test pool initializes");
    let runtime = Runtime::new(pool, None).expect("runtime initializes");
    let component = wasmtime::component::Component::from_file(&runtime.engine, module)
        .expect("Vue admin example component loads");
    let plugin = PluginComponent {
        id: "vue-admin".into(),
        component,
        runtime,
        mail_permissions: Default::default(),
        storage_directories: Arc::new(Vec::new()),
    };
    let response = plugin
        .call(
            bindings::nur::cms::types::Request {
                route_id: "ping".into(),
                method: "GET".into(),
                path: "/api/p/vue-admin/ping".into(),
                path_params: Vec::new(),
                query: None,
                headers: Vec::new(),
                body: Vec::new(),
                identity: Some(bindings::nur::cms::types::Identity {
                    user_id: 1,
                    roles: vec!["admin".into()],
                }),
            },
            false,
            Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
        )
        .await
        .expect("Vue admin example request succeeds");

    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"Authenticated plugin request succeeded.");
}

fn example_component(example: &str, artifact: &str) -> PathBuf {
    let target = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join(example)
        .join("target/wasm32-wasip2");
    let release = target.join("release").join(format!("{artifact}.wasm"));
    if release.is_file() {
        return release;
    }

    fs::read_dir(target.join("debug/deps"))
        .expect("WASIp2 example test component is built")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.extension().and_then(|extension| extension.to_str()) == Some("wasm")
                && path
                    .file_stem()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(artifact))
        })
        .expect("WASIp2 example test component exists")
}
