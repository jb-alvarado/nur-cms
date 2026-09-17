use std::{
    collections::{HashMap, HashSet, VecDeque},
    net::IpAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use crate::manifest::InstalledPlugin;

#[derive(Clone, Default)]
pub(super) struct MailPermissions {
    pub(super) targets: Arc<HashSet<String>>,
    pub(super) dynamic_recipient_targets: Arc<HashSet<String>>,
    pub(super) trusted_template_targets: Arc<HashSet<String>>,
}

impl MailPermissions {
    pub(super) fn from_plugin(plugin: &InstalledPlugin) -> Self {
        let mail = &plugin.manifest.mail;

        Self {
            targets: Arc::new(mail.targets.iter().cloned().collect()),
            dynamic_recipient_targets: Arc::new(
                mail.dynamic_recipient_targets.iter().cloned().collect(),
            ),
            trusted_template_targets: Arc::new(
                mail.trusted_template_targets.iter().cloned().collect(),
            ),
        }
    }

    pub(super) fn allows(
        &self,
        target: &str,
        dynamic_recipient: bool,
        trusted_template: bool,
    ) -> bool {
        self.targets.contains(target)
            && (!dynamic_recipient || self.dynamic_recipient_targets.contains(target))
            && (!trusted_template || self.trusted_template_targets.contains(target))
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct PublicMailKey {
    plugin_id: String,
    route_id: String,
    client_ip: IpAddr,
}

pub(super) struct PublicMailRateLimiter {
    pub(super) sent: HashMap<PublicMailKey, Instant>,
    pub(super) expirations: VecDeque<(Instant, PublicMailKey)>,
    pub(super) window: Duration,
    pub(super) max_clients: usize,
}

impl PublicMailRateLimiter {
    pub(super) fn new(window: Duration, max_clients: usize) -> Self {
        Self {
            sent: HashMap::new(),
            expirations: VecDeque::new(),
            window,
            max_clients,
        }
    }
}

pub(super) fn allow_mail_for_request(
    calls_remaining: &mut u8,
    public_route: bool,
    public_authorized: &mut Option<bool>,
    reserve_public_limit: impl FnOnce() -> bool,
) -> bool {
    let Some(remaining) = calls_remaining.checked_sub(1) else {
        return false;
    };

    *calls_remaining = remaining;

    if !public_route {
        return true;
    }

    *public_authorized.get_or_insert_with(reserve_public_limit)
}

pub(super) fn allow_public_mail(
    limiter: &mut PublicMailRateLimiter,
    plugin_id: &str,
    route_id: &str,
    client_ip: IpAddr,
    now: Instant,
) -> bool {
    while limiter
        .expirations
        .front()
        .is_some_and(|(expires_at, _)| *expires_at <= now)
    {
        let Some((expires_at, key)) = limiter.expirations.pop_front() else {
            break;
        };
        if limiter.sent.get(&key) == Some(&expires_at) {
            limiter.sent.remove(&key);
        }
    }

    let key = PublicMailKey {
        plugin_id: plugin_id.into(),
        route_id: route_id.into(),
        client_ip,
    };

    if limiter.sent.contains_key(&key) || limiter.sent.len() >= limiter.max_clients {
        return false;
    }

    let expires_at = now.checked_add(limiter.window).unwrap_or(now);
    limiter.sent.insert(key.clone(), expires_at);
    limiter.expirations.push_back((expires_at, key));

    true
}

pub(super) fn allow_public_mail_for_client(
    limiter: &Arc<Mutex<PublicMailRateLimiter>>,
    plugin_id: &str,
    route_id: &str,
    client_ip: Option<IpAddr>,
    now: Instant,
) -> bool {
    let Some(client_ip) = client_ip else {
        return false;
    };

    let Ok(mut limiter) = limiter.lock() else {
        return false;
    };

    allow_public_mail(&mut limiter, plugin_id, route_id, client_ip, now)
}
