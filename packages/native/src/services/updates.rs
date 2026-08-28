//! 检查更新：对比当前 Cargo 版本与 GitHub Releases / tags。
//! 不自动下载安装；发现新版本后打开公开发布页。

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::services::transport::Transport;

pub const GITHUB_REPO: &str = "k1nz/jiti";
const RELEASES_PATH: &str = "/releases?per_page=20";
const TAGS_PATH: &str = "/tags?per_page=20";

pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn releases_page_url() -> String {
    format!("https://github.com/{GITHUB_REPO}/releases")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum UpdateStatus {
    UpToDate,
    Available,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckResult {
    pub status: UpdateStatus,
    pub current_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

pub async fn check(transport: &dyn Transport) -> UpdateCheckResult {
    check_at(
        &format!("https://api.github.com/repos/{GITHUB_REPO}"),
        transport,
    )
    .await
}

pub async fn check_at(api_base: &str, transport: &dyn Transport) -> UpdateCheckResult {
    let current = current_version().to_string();
    match fetch_latest(api_base, transport).await {
        Ok(Some(latest)) => compare(&current, latest),
        Ok(None) => UpdateCheckResult {
            status: UpdateStatus::UpToDate,
            current_version: current,
            latest_version: None,
            release_url: Some(releases_page_url()),
            message: Some("尚未发布正式版本".into()),
        },
        Err(message) => UpdateCheckResult {
            status: UpdateStatus::Error,
            current_version: current,
            latest_version: None,
            release_url: Some(releases_page_url()),
            message: Some(message),
        },
    }
}

fn compare(current: &str, latest: RemoteVersion) -> UpdateCheckResult {
    let current_parsed = parse_version(current);
    let latest_parsed = parse_version(&latest.version);
    let newer = match (current_parsed.as_ref(), latest_parsed.as_ref()) {
        (Some(a), Some(b)) => b > a,
        _ => latest.version.trim_start_matches('v') != current.trim_start_matches('v'),
    };
    if newer {
        UpdateCheckResult {
            status: UpdateStatus::Available,
            current_version: current.into(),
            latest_version: Some(latest.version),
            release_url: Some(latest.url),
            message: None,
        }
    } else {
        UpdateCheckResult {
            status: UpdateStatus::UpToDate,
            current_version: current.into(),
            latest_version: Some(latest.version),
            release_url: Some(latest.url),
            message: None,
        }
    }
}

struct RemoteVersion {
    version: String,
    url: String,
}

async fn fetch_latest(
    api_base: &str,
    transport: &dyn Transport,
) -> Result<Option<RemoteVersion>, String> {
    let base = api_base.trim_end_matches('/');
    let headers = github_headers();
    if let Some(latest) = fetch_releases(base, &headers, transport).await? {
        return Ok(Some(latest));
    }
    fetch_tags(base, &headers, transport).await
}

fn github_headers() -> Vec<(String, String)> {
    vec![
        ("User-Agent".into(), format!("jiti/{}", current_version())),
        ("Accept".into(), "application/vnd.github+json".into()),
        ("X-GitHub-Api-Version".into(), "2022-11-28".into()),
    ]
}

fn header_refs(headers: &[(String, String)]) -> Vec<(&str, &str)> {
    headers
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect()
}

async fn fetch_releases(
    base: &str,
    headers: &[(String, String)],
    transport: &dyn Transport,
) -> Result<Option<RemoteVersion>, String> {
    let url = format!("{base}{RELEASES_PATH}");
    let refs = header_refs(headers);
    let resp = transport
        .get_with_headers(&url, &refs)
        .await
        .map_err(|e| e.to_string())?;
    let status = resp.status().as_u16();
    let body = resp.text().await.unwrap_or_default();
    if status == 404 {
        return Ok(None);
    }
    if !(200..300).contains(&status) {
        return Err(format!("GitHub Releases HTTP {status}"));
    }
    let releases: Vec<GithubRelease> =
        serde_json::from_str(&body).map_err(|e| format!("无法解析 GitHub Releases：{e}"))?;
    Ok(pick_latest(releases.into_iter().filter(|r| !r.draft).map(
        |r| RemoteVersion {
            version: strip_v(&r.tag_name),
            url: if r.html_url.trim().is_empty() {
                tag_url(&r.tag_name)
            } else {
                r.html_url
            },
        },
    )))
}

async fn fetch_tags(
    base: &str,
    headers: &[(String, String)],
    transport: &dyn Transport,
) -> Result<Option<RemoteVersion>, String> {
    let url = format!("{base}{TAGS_PATH}");
    let refs = header_refs(headers);
    let resp = transport
        .get_with_headers(&url, &refs)
        .await
        .map_err(|e| e.to_string())?;
    let status = resp.status().as_u16();
    let body = resp.text().await.unwrap_or_default();
    if status == 404 {
        return Ok(None);
    }
    if !(200..300).contains(&status) {
        return Err(format!("GitHub Tags HTTP {status}"));
    }
    let tags: Vec<GithubTag> =
        serde_json::from_str(&body).map_err(|e| format!("无法解析 GitHub Tags：{e}"))?;
    Ok(pick_latest(tags.into_iter().map(|t| RemoteVersion {
        version: strip_v(&t.name),
        url: tag_url(&t.name),
    })))
}

fn pick_latest(items: impl Iterator<Item = RemoteVersion>) -> Option<RemoteVersion> {
    items.max_by(
        |a, b| match (parse_version(&a.version), parse_version(&b.version)) {
            (Some(x), Some(y)) => x.cmp(&y),
            (Some(_), None) => Ordering::Greater,
            (None, Some(_)) => Ordering::Less,
            (None, None) => a.version.cmp(&b.version),
        },
    )
}

fn tag_url(tag: &str) -> String {
    format!("https://github.com/{GITHUB_REPO}/releases/tag/{tag}")
}

fn strip_v(tag: &str) -> String {
    tag.trim().trim_start_matches('v').to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedVersion {
    major: u64,
    minor: u64,
    patch: u64,
    pre_ident: Option<String>,
    pre_num: u64,
}

impl PartialOrd for ParsedVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ParsedVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            .then_with(|| match (&self.pre_ident, &other.pre_ident) {
                (None, None) => Ordering::Equal,
                (None, Some(_)) => Ordering::Greater,
                (Some(_), None) => Ordering::Less,
                (Some(a), Some(b)) => a.cmp(b).then(self.pre_num.cmp(&other.pre_num)),
            })
    }
}

fn parse_version(raw: &str) -> Option<ParsedVersion> {
    let s = raw.trim().trim_start_matches('v');
    if s.is_empty() {
        return None;
    }
    let (core, pre) = match s.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (s, None),
    };
    let mut nums = core.split('.');
    let major = nums.next()?.parse().ok()?;
    let minor = nums.next().unwrap_or("0").parse().ok()?;
    let patch = nums.next().unwrap_or("0").parse().ok()?;
    let (pre_ident, pre_num) = match pre {
        Some(pre) if !pre.is_empty() => {
            let (ident, num) = match pre.rsplit_once('.') {
                Some((ident, num)) => match num.parse::<u64>() {
                    Ok(n) => (ident.to_ascii_lowercase(), n),
                    Err(_) => (pre.to_ascii_lowercase(), 0),
                },
                None => {
                    let ident: String = pre
                        .chars()
                        .take_while(|c| c.is_ascii_alphabetic())
                        .collect();
                    let num: String = pre
                        .chars()
                        .skip_while(|c| c.is_ascii_alphabetic())
                        .collect();
                    (
                        if ident.is_empty() {
                            pre.to_ascii_lowercase()
                        } else {
                            ident.to_ascii_lowercase()
                        },
                        num.parse().unwrap_or(0),
                    )
                }
            };
            (Some(ident), num)
        }
        _ => (None, 0),
    };
    Some(ParsedVersion {
        major,
        minor,
        patch,
        pre_ident,
        pre_num,
    })
}

pub fn is_allowed_url(url: &str) -> bool {
    if url.contains(char::is_whitespace) || url.contains('\0') {
        return false;
    }
    let lower = url.to_ascii_lowercase();
    lower.starts_with("https://github.com/k1nz/jiti")
}

pub fn open_https_url(url: &str) -> Result<(), String> {
    let url = url.trim();
    if !is_allowed_url(url) {
        return Err("只允许打开本项目的 GitHub 发布页".into());
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
}

#[derive(Debug, Deserialize)]
struct GithubTag {
    name: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::transport::ReqwestTransport;

    #[test]
    fn rc_is_older_than_stable() {
        let rc = parse_version("0.5.0-rc.1").unwrap();
        let stable = parse_version("v0.5.0").unwrap();
        assert!(rc < stable);
        assert!(parse_version("0.5.0-rc.1").unwrap() < parse_version("0.5.0-rc.2").unwrap());
        assert!(parse_version("0.5.0").unwrap() < parse_version("0.6.0").unwrap());
    }

    #[test]
    fn compare_marks_newer_release_available() {
        let result = compare(
            "0.5.0-rc.1",
            RemoteVersion {
                version: "0.5.0".into(),
                url: "https://github.com/k1nz/jiti/releases/tag/v0.5.0".into(),
            },
        );
        assert_eq!(result.status, UpdateStatus::Available);
        assert_eq!(result.latest_version.as_deref(), Some("0.5.0"));
    }

    #[test]
    fn compare_same_version_is_up_to_date() {
        let result = compare(
            "0.5.0-rc.1",
            RemoteVersion {
                version: "0.5.0-rc.1".into(),
                url: "https://github.com/k1nz/jiti/releases/tag/v0.5.0-rc.1".into(),
            },
        );
        assert_eq!(result.status, UpdateStatus::UpToDate);
    }

    #[test]
    fn only_project_github_https_is_allowed() {
        assert!(is_allowed_url(
            "https://github.com/k1nz/jiti/releases/tag/v0.5.0"
        ));
        assert!(!is_allowed_url("http://github.com/k1nz/jiti"));
        assert!(!is_allowed_url("https://evil.example/"));
        assert!(!is_allowed_url("https://github.com/k1nz/jiti/releases\n"));
    }

    #[test]
    fn releases_mock_picks_highest_non_draft() {
        let mut server = mockito::Server::new();
        let releases = server
            .mock("GET", "/releases?per_page=20")
            .with_status(200)
            .with_body(
                r#"[{"tag_name":"v0.4.0","html_url":"https://github.com/k1nz/jiti/releases/tag/v0.4.0","draft":false},{"tag_name":"v0.5.0-rc.1","html_url":"https://github.com/k1nz/jiti/releases/tag/v0.5.0-rc.1","draft":false},{"tag_name":"v9.9.9","html_url":"https://github.com/k1nz/jiti/releases/tag/v9.9.9","draft":true}]"#,
            )
            .create();
        let out = tauri::async_runtime::block_on(async {
            check_at(&server.url(), &ReqwestTransport::default()).await
        });
        releases.assert();
        assert_eq!(out.status, UpdateStatus::UpToDate);
        assert_eq!(out.latest_version.as_deref(), Some("0.5.0-rc.1"));
    }

    #[test]
    fn empty_releases_falls_back_to_tags() {
        let mut server = mockito::Server::new();
        let releases = server
            .mock("GET", "/releases?per_page=20")
            .with_status(200)
            .with_body("[]")
            .create();
        let tags = server
            .mock("GET", "/tags?per_page=20")
            .with_status(200)
            .with_body(r#"[{"name":"v0.5.0-rc.1"}]"#)
            .create();
        let out = tauri::async_runtime::block_on(async {
            check_at(&server.url(), &ReqwestTransport::default()).await
        });
        releases.assert();
        tags.assert();
        assert_eq!(out.status, UpdateStatus::UpToDate);
        assert_eq!(out.latest_version.as_deref(), Some("0.5.0-rc.1"));
        assert!(out
            .release_url
            .as_deref()
            .unwrap_or("")
            .contains("v0.5.0-rc.1"));
    }

    #[test]
    fn network_error_is_graceful() {
        let out = tauri::async_runtime::block_on(async {
            check_at("http://127.0.0.1:1", &ReqwestTransport::default()).await
        });
        assert_eq!(out.status, UpdateStatus::Error);
        assert!(out.message.is_some());
        assert_eq!(out.current_version, current_version());
    }
}
