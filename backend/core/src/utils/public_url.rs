use axum::http::Uri;

/// Returns the configured canonical public CMS URL after applying the same
/// origin checks used for externally visible links.
pub fn configured_public_url() -> Option<String> {
    let value = std::env::var("NUR_PUBLIC_URL").ok();
    public_url_from_value(value.as_deref())
}

fn public_url_from_value(value: Option<&str>) -> Option<String> {
    value.and_then(normalize_public_url)
}

pub fn normalize_public_url(value: &str) -> Option<String> {
    let value = value.trim().trim_end_matches('/');
    let uri: Uri = value.parse().ok()?;
    if uri.authority()?.as_str().contains('@') {
        return None;
    }
    let host = uri.host()?;
    let https = uri.scheme_str() == Some("https");
    let local_http = uri.scheme_str() == Some("http") && matches!(host, "localhost" | "127.0.0.1");

    (https || local_http)
        .then_some(())
        .filter(|_| uri.query().is_none())
        .map(|_| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::{normalize_public_url, public_url_from_value};

    #[test]
    fn normalizes_https_and_local_http_urls() {
        assert_eq!(
            normalize_public_url(" https://cms.example.org/ "),
            Some("https://cms.example.org".into())
        );
        assert_eq!(
            normalize_public_url("http://localhost:8777/"),
            Some("http://localhost:8777".into())
        );
        assert_eq!(
            normalize_public_url("http://127.0.0.1:8777///"),
            Some("http://127.0.0.1:8777".into())
        );
    }

    #[test]
    fn rejects_missing_or_unsafe_public_urls() {
        assert_eq!(public_url_from_value(None), None);
        for value in [
            "",
            "cms.example.org",
            "http://cms.example.org",
            "http://0.0.0.0:8777",
            "https://cms.example.org/?token=secret",
            "https://user:password@cms.example.org",
        ] {
            assert_eq!(normalize_public_url(value), None, "accepted {value}");
        }
    }
}
