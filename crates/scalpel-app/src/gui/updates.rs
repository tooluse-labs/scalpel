use std::cmp::Ordering;
use std::time::Duration;

const UPDATE_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct UpdateCheckResult {
    pub(crate) latest_tag: String,
    pub(crate) latest_version: String,
    pub(crate) release_name: Option<String>,
    pub(crate) release_url: String,
    pub(crate) published_at: Option<String>,
    pub(crate) is_newer: bool,
}

pub(crate) fn check_for_update(
    current_version: &str,
    api_url: &str,
) -> Result<UpdateCheckResult, String> {
    let user_agent = format!("Scalpel/{current_version}");
    let agent = ureq::AgentBuilder::new()
        .timeout(UPDATE_REQUEST_TIMEOUT)
        .build();
    let response = agent
        .get(api_url)
        .set("Accept", "application/vnd.github+json")
        .set("X-GitHub-Api-Version", "2022-11-28")
        .set("User-Agent", &user_agent)
        .call()
        .map_err(update_request_error_message)?;
    let body = response
        .into_string()
        .map_err(|err| format!("failed to read update response: {err}"))?;
    parse_update_response(current_version, &body)
}

fn update_request_error_message(err: ureq::Error) -> String {
    match err {
        ureq::Error::Status(code, response) => {
            let detail = response
                .into_string()
                .ok()
                .and_then(|body| short_json_message(&body))
                .unwrap_or_default();
            if detail.is_empty() {
                format!("GitHub release check failed with HTTP {code}")
            } else {
                format!("GitHub release check failed with HTTP {code}: {detail}")
            }
        }
        ureq::Error::Transport(err) => {
            format!(
                "could not connect to GitHub releases. Check your network connection or open Releases manually. Details: {err}"
            )
        }
    }
}

fn short_json_message(body: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    value
        .get("message")
        .and_then(serde_json::Value::as_str)
        .map(|message| truncate_for_status(message, 180))
}

pub(crate) fn parse_update_response(
    current_version: &str,
    body: &str,
) -> Result<UpdateCheckResult, String> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|err| format!("invalid release response: {err}"))?;
    let latest_tag = required_json_string(&value, "tag_name")?;
    let release_url = required_json_string(&value, "html_url")?;
    let release_name = optional_json_string(&value, "name");
    let published_at = optional_json_string(&value, "published_at");
    let latest_version = normalize_version_label(&latest_tag);
    let is_newer = compare_versions(&latest_tag, current_version)
        .is_some_and(|ordering| ordering == Ordering::Greater);

    Ok(UpdateCheckResult {
        latest_tag,
        latest_version,
        release_name,
        release_url,
        published_at,
        is_newer,
    })
}

fn required_json_string(value: &serde_json::Value, field: &str) -> Result<String, String> {
    optional_json_string(value, field).ok_or_else(|| format!("release response missing {field}"))
}

fn optional_json_string(value: &serde_json::Value, field: &str) -> Option<String> {
    value
        .get(field)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn normalize_version_label(version: &str) -> String {
    version
        .trim()
        .strip_prefix('v')
        .or_else(|| version.trim().strip_prefix('V'))
        .unwrap_or_else(|| version.trim())
        .to_string()
}

pub(crate) fn compare_versions(left: &str, right: &str) -> Option<Ordering> {
    let left = parse_version_parts(left)?;
    let right = parse_version_parts(right)?;
    let len = left.len().max(right.len());
    for index in 0..len {
        let left_part = left.get(index).copied().unwrap_or(0);
        let right_part = right.get(index).copied().unwrap_or(0);
        match left_part.cmp(&right_part) {
            Ordering::Equal => {}
            ordering => return Some(ordering),
        }
    }
    Some(Ordering::Equal)
}

fn parse_version_parts(version: &str) -> Option<Vec<u64>> {
    let core = normalize_version_label(version);
    let core = core
        .split(['-', '+'])
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let mut parts = Vec::new();
    for part in core.split('.') {
        if part.is_empty() || !part.chars().all(|ch| ch.is_ascii_digit()) {
            return None;
        }
        parts.push(part.parse().ok()?);
    }
    (!parts.is_empty()).then_some(parts)
}

fn truncate_for_status(text: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for ch in text.chars().take(max_chars) {
        out.push(ch);
    }
    if text.chars().count() > max_chars {
        out.push_str("...");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_github_latest_release_response() {
        let result = parse_update_response(
            "0.2.15",
            r#"{
                "tag_name": "v0.2.16",
                "name": "v0.2.16",
                "html_url": "https://github.com/tooluse-labs/scalpel/releases/tag/v0.2.16",
                "published_at": "2026-06-17T00:00:00Z"
            }"#,
        )
        .unwrap();

        assert_eq!(result.latest_tag, "v0.2.16");
        assert_eq!(result.latest_version, "0.2.16");
        assert!(result.is_newer);
        assert_eq!(result.release_name.as_deref(), Some("v0.2.16"));
    }

    #[test]
    fn compares_semver_like_tags() {
        assert_eq!(
            compare_versions("v0.2.16", "0.2.15"),
            Some(Ordering::Greater)
        );
        assert_eq!(compare_versions("0.2.15", "v0.2.15"), Some(Ordering::Equal));
        assert_eq!(compare_versions("0.2.14", "0.2.15"), Some(Ordering::Less));
        assert_eq!(compare_versions("0.10.0", "0.9.9"), Some(Ordering::Greater));
        assert_eq!(compare_versions("bad", "0.2.15"), None);
    }
}
