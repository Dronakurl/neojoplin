use anyhow::Result;
use neojoplin_tui::settings::SyncTarget;

pub fn resolve_sync_target(
    url: Option<String>,
    username: Option<String>,
    password: Option<String>,
    remote: Option<String>,
    configured_target: Option<SyncTarget>,
) -> Result<(String, String, String, String)> {
    if let Some(url) = url {
        let (base_url, configured_remote) = split_webdav_url(&url);
        return Ok((
            base_url,
            username.unwrap_or_default(),
            password.unwrap_or_default(),
            remote.unwrap_or(configured_remote),
        ));
    }

    let configured_target = configured_target.ok_or_else(|| {
        anyhow::anyhow!(
            "WebDAV URL is required or configure a sync target in the TUI settings first"
        )
    })?;

    let (base_url, configured_remote) = split_webdav_url(&configured_target.url);
    Ok((
        base_url,
        username.unwrap_or(configured_target.username),
        password.unwrap_or(configured_target.password),
        remote.unwrap_or(configured_remote),
    ))
}

pub fn split_webdav_url(full_url: &str) -> (String, String) {
    let trimmed = full_url.trim_end_matches('/');
    let scheme_end = trimmed.find("://").map(|i| i + 3).unwrap_or(0);
    if let Some(slash_pos) = trimmed[scheme_end..].rfind('/') {
        let abs_pos = scheme_end + slash_pos;
        let base = &trimmed[..abs_pos];
        let path = &trimmed[abs_pos..];
        if !path.is_empty() && path != "/" {
            return (base.to_string(), path.to_string());
        }
    }

    (trimmed.to_string(), "/neojoplin".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use neojoplin_tui::settings::SyncTargetType;

    fn configured_target(url: &str) -> SyncTarget {
        SyncTarget {
            id: "id".to_string(),
            name: "name".to_string(),
            target_type: SyncTargetType::WebDAV,
            url: url.to_string(),
            username: "cfg-user".to_string(),
            password: "cfg-pass".to_string(),
            remote_path: "/ignored-by-cli".to_string(),
            ignore_tls_errors: false,
        }
    }

    #[test]
    fn split_webdav_url_extracts_remote_path() {
        let (base, remote) = split_webdav_url("https://dav.example.com/root/folder");
        assert_eq!(base, "https://dav.example.com/root");
        assert_eq!(remote, "/folder");
    }

    #[test]
    fn split_webdav_url_defaults_remote_path_when_missing() {
        let (base, remote) = split_webdav_url("https://dav.example.com");
        assert_eq!(base, "https://dav.example.com");
        assert_eq!(remote, "/neojoplin");
    }

    #[test]
    fn split_webdav_url_ignores_trailing_slash() {
        let (base, remote) = split_webdav_url("https://dav.example.com/notes/");
        assert_eq!(base, "https://dav.example.com");
        assert_eq!(remote, "/notes");
    }

    #[test]
    fn resolve_sync_target_prefers_cli_args() {
        let resolved = resolve_sync_target(
            Some("https://dav.example.com/custom/path".to_string()),
            Some("arg-user".to_string()),
            Some("arg-pass".to_string()),
            Some("/explicit".to_string()),
            Some(configured_target("https://dav.example.com/from/settings")),
        )
        .expect("resolution should work");

        assert_eq!(
            resolved,
            (
                "https://dav.example.com/custom".to_string(),
                "arg-user".to_string(),
                "arg-pass".to_string(),
                "/explicit".to_string()
            )
        );
    }

    #[test]
    fn resolve_sync_target_uses_settings_when_url_not_provided() {
        let resolved = resolve_sync_target(
            None,
            None,
            None,
            None,
            Some(configured_target("https://dav.example.com/notes")),
        )
        .expect("resolution should work");

        assert_eq!(
            resolved,
            (
                "https://dav.example.com".to_string(),
                "cfg-user".to_string(),
                "cfg-pass".to_string(),
                "/notes".to_string()
            )
        );
    }

    #[test]
    fn resolve_sync_target_errors_without_cli_url_or_settings() {
        let err = resolve_sync_target(None, None, None, None, None).expect_err("expected error");
        assert!(err
            .to_string()
            .contains("WebDAV URL is required or configure a sync target"));
    }
}
