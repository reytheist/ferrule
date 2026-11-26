use std::cmp::Ordering;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use mail_core::error::CoreError;
use serde::{Deserialize, Serialize};
use tauri::Emitter;
use url::Url;

const DEFAULT_RELEASES_API_URL: &str =
    "https://api.github.com/repos/reytheist/ferrule/releases/latest";
const UPDATE_URL_ENV: &str = "FERRULE_UPDATE_URL";
pub const UPDATE_PROGRESS_EVENT: &str = "app://update-progress";

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateReleaseAsset {
    pub name: String,
    pub download_url: String,
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateReleaseInfo {
    pub version: String,
    pub tag_name: String,
    pub name: String,
    pub notes: String,
    pub release_url: String,
    pub published_at: Option<String>,
    pub asset: Option<UpdateReleaseAsset>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckResult {
    pub current_version: String,
    pub latest_version: Option<String>,
    pub update_available: bool,
    pub release: Option<UpdateReleaseInfo>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallUpdateInput {
    pub version: String,
    pub release_url: String,
    pub asset_url: Option<String>,
    pub asset_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum InstallUpdateOutcome {
    InstallerLaunched { file_path: String },
    OpenedReleasePage { url: String },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateDownloadProgress {
    pub version: String,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub percent: Option<u8>,
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    name: Option<String>,
    body: Option<String>,
    html_url: String,
    published_at: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Clone, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
    size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemVer {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub pre: Option<String>,
}

impl SemVer {
    pub fn parse(raw: &str) -> Option<Self> {
        let trimmed = raw.trim().trim_start_matches(['v', 'V']);
        let without_build = trimmed.split('+').next()?;
        let (core, pre) = match without_build.split_once('-') {
            Some((c, p)) if !p.is_empty() => (c, Some(p.to_string())),
            Some((c, _)) => (c, None),
            None => (without_build, None),
        };
        let mut parts = core.split('.');
        let major = parts.next()?.parse::<u64>().ok()?;
        let minor = parts.next().unwrap_or("0").parse::<u64>().ok()?;
        let patch = parts.next().unwrap_or("0").parse::<u64>().ok()?;
        if parts.next().is_some() {
            return None;
        }
        Some(Self {
            major,
            minor,
            patch,
            pre,
        })
    }

    pub fn canonical(&self) -> String {
        match &self.pre {
            Some(pre) => format!("{}.{}.{}-{}", self.major, self.minor, self.patch, pre),
            None => format!("{}.{}.{}", self.major, self.minor, self.patch),
        }
    }
}

impl Ord for SemVer {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            .then_with(|| match (&self.pre, &other.pre) {
                (None, None) => Ordering::Equal,
                (None, Some(_)) => Ordering::Greater,
                (Some(_), None) => Ordering::Less,
                (Some(a), Some(b)) => compare_prerelease(a, b),
            })
    }
}

impl PartialOrd for SemVer {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn compare_prerelease(a: &str, b: &str) -> Ordering {
    let a_parts: Vec<&str> = a.split('.').collect();
    let b_parts: Vec<&str> = b.split('.').collect();
    for (left, right) in a_parts.iter().zip(b_parts.iter()) {
        let ord = match (left.parse::<u64>(), right.parse::<u64>()) {
            (Ok(l), Ok(r)) => l.cmp(&r),
            (Ok(_), Err(_)) => Ordering::Less,
            (Err(_), Ok(_)) => Ordering::Greater,
            (Err(_), Err(_)) => left.cmp(right),
        };
        if ord != Ordering::Equal {
            return ord;
        }
    }
    a_parts.len().cmp(&b_parts.len())
}

pub fn is_newer_version(current: &str, candidate: &str) -> bool {
    match (SemVer::parse(current), SemVer::parse(candidate)) {
        (Some(cur), Some(cand)) => cand > cur,
        _ => false,
    }
}

fn asset_arch_bonus(lower_name: &str, arch: &str) -> i32 {
    let is_arm = matches!(arch, "aarch64" | "arm64");
    let is_x64 = matches!(arch, "x86_64" | "x64" | "amd64");
    let mentions_arm =
        lower_name.contains("arm64") || lower_name.contains("aarch64");
    let mentions_x64 = lower_name.contains("x64")
        || lower_name.contains("x86_64")
        || lower_name.contains("amd64");

    if is_arm {
        if mentions_arm {
            20
        } else if mentions_x64 {
            -40
        } else {
            0
        }
    } else if is_x64 {
        if mentions_x64 {
            20
        } else if mentions_arm {
            -40
        } else {
            0
        }
    } else {
        0
    }
}

fn select_release_asset(
    assets: &[GitHubAsset],
    os: &str,
    arch: &str,
) -> Option<UpdateReleaseAsset> {
    let mut best: Option<(i32, &GitHubAsset)> = None;

    for asset in assets {
        let lower = asset.name.to_ascii_lowercase();
        // Skip signature / checksum sidecar files.
        if lower.ends_with(".sig")
            || lower.ends_with(".sha256")
            || lower.ends_with(".sha256sum")
            || lower.ends_with(".asc")
            || lower.ends_with(".json")
        {
            continue;
        }

        let base_score = match os {
            "windows" => {
                if lower.ends_with("-setup.exe") || lower.ends_with("_setup.exe") {
                    100
                } else if lower.ends_with(".exe") {
                    90
                } else if lower.ends_with(".msi") {
                    85
                } else {
                    continue;
                }
            }
            "linux" => {
                if lower.ends_with(".appimage") {
                    100
                } else if lower.ends_with(".deb") {
                    85
                } else if lower.ends_with(".rpm") {
                    75
                } else {
                    continue;
                }
            }
            "macos" => {
                if lower.ends_with(".dmg") {
                    100
                } else if lower.ends_with(".app.tar.gz") {
                    80
                } else {
                    continue;
                }
            }
            _ => continue,
        };

        let score = base_score + asset_arch_bonus(&lower, arch);
        if best.is_none_or(|(best_score, _)| score > best_score) {
            best = Some((score, asset));
        }
    }

    best.map(|(_, asset)| UpdateReleaseAsset {
        name: asset.name.clone(),
        download_url: asset.browser_download_url.clone(),
        size_bytes: asset.size,
    })
}

fn allow_local_update_host() -> bool {
    std::env::var(UPDATE_URL_ENV)
        .ok()
        .is_some_and(|v| !v.trim().is_empty())
}

fn validate_update_url(raw: &str, allow_local: bool) -> Result<Url, CoreError> {
    let parsed = Url::parse(raw.trim()).map_err(|_| CoreError::InvalidInput {
        field: "url".into(),
        reason: "invalid update URL".into(),
    })?;

    let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
    let is_local = host == "localhost" || host == "127.0.0.1" || host == "::1";

    if allow_local && is_local && matches!(parsed.scheme(), "http" | "https") {
        return Ok(parsed);
    }

    if parsed.scheme() != "https" {
        return Err(CoreError::InvalidInput {
            field: "url".into(),
            reason: "update URL must use https".into(),
        });
    }

    let allowed_host = host == "github.com"
        || host == "api.github.com"
        || host.ends_with(".githubusercontent.com")
        || host == "githubusercontent.com";

    if !allowed_host {
        return Err(CoreError::InvalidInput {
            field: "url".into(),
            reason: format!("untrusted update host: {host}"),
        });
    }

    Ok(parsed)
}

fn sanitize_filename(name: &str) -> Option<String> {
    let candidate = Path::new(name)
        .file_name()
        .and_then(|s| s.to_str())?
        .trim();
    if candidate.is_empty() || candidate == "." || candidate == ".." {
        return None;
    }
    let cleaned: String = candidate
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() || cleaned.starts_with('.') {
        return None;
    }
    Some(cleaned)
}

fn parse_release_check(
    current_version: &str,
    release: GitHubRelease,
    os: &str,
    arch: &str,
) -> UpdateCheckResult {
    if release.draft || release.prerelease {
        return UpdateCheckResult {
            current_version: current_version.to_string(),
            latest_version: Some(current_version.to_string()),
            update_available: false,
            release: None,
        };
    }

    let parsed_version = SemVer::parse(&release.tag_name)
        .map(|v| v.canonical())
        .unwrap_or_else(|| release.tag_name.trim_start_matches(['v', 'V']).to_string());

    let update_available = is_newer_version(current_version, &release.tag_name);
    let asset = select_release_asset(&release.assets, os, arch);

    let info = UpdateReleaseInfo {
        version: parsed_version.clone(),
        tag_name: release.tag_name,
        name: release
            .name
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| format!("Ferrule v{parsed_version}")),
        notes: release.body.unwrap_or_default(),
        release_url: release.html_url,
        published_at: release.published_at,
        asset,
    };

    UpdateCheckResult {
        current_version: current_version.to_string(),
        latest_version: Some(parsed_version),
        update_available,
        release: if update_available { Some(info) } else { None },
    }
}

fn build_http_client(timeout: Duration) -> Result<reqwest::Client, CoreError> {
    reqwest::Client::builder()
        .user_agent(format!("Ferrule/{CURRENT_VERSION}"))
        .timeout(timeout)
        .build()
        .map_err(|err| CoreError::Network {
            reason: format!("failed to initialize HTTP client: {err}"),
            retryable: true,
        })
}

pub async fn check_for_updates() -> Result<UpdateCheckResult, CoreError> {
    let endpoint = std::env::var(UPDATE_URL_ENV)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_RELEASES_API_URL.to_string());

    let client = build_http_client(Duration::from_secs(15))?;
    let response = client
        .get(&endpoint)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .await
        .map_err(|err| CoreError::Network {
            reason: format!("could not reach update server: {err}"),
            retryable: true,
        })?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        // Repository has no published GitHub Release yet; treat as up to date.
        return Ok(UpdateCheckResult {
            current_version: CURRENT_VERSION.to_string(),
            latest_version: Some(CURRENT_VERSION.to_string()),
            update_available: false,
            release: None,
        });
    }

    if !response.status().is_success() {
        return Err(CoreError::Network {
            reason: format!("update server returned HTTP {}", response.status().as_u16()),
            retryable: response.status().is_server_error()
                || response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS,
        });
    }

    let release: GitHubRelease = response.json().await.map_err(|err| CoreError::Network {
        reason: format!("invalid release metadata: {err}"),
        retryable: true,
    })?;

    Ok(parse_release_check(
        CURRENT_VERSION,
        release,
        std::env::consts::OS,
        std::env::consts::ARCH,
    ))
}

fn launch_installer(path: &Path) -> Result<(), CoreError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let is_appimage = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.to_ascii_lowercase().ends_with(".appimage"));
        if is_appimage {
            let mut perms = fs::metadata(path)
                .map_err(|err| CoreError::Internal {
                    reason: format!("failed to inspect downloaded update: {err}"),
                })?
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(path, perms).map_err(|err| CoreError::Internal {
                reason: format!("failed to mark AppImage executable: {err}"),
            })?;
            std::process::Command::new(path)
                .spawn()
                .map_err(|err| CoreError::Internal {
                    reason: format!("failed to launch AppImage: {err}"),
                })?;
            return Ok(());
        }
    }

    open::that(path).map_err(|err| CoreError::Internal {
        reason: format!("failed to launch update installer: {err}"),
    })
}

pub async fn download_and_install_update(
    app: &tauri::AppHandle,
    input: InstallUpdateInput,
) -> Result<InstallUpdateOutcome, CoreError> {
    let allow_local = allow_local_update_host();

    let Some(asset_url_raw) = input
        .asset_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    else {
        let release_url = validate_update_url(&input.release_url, allow_local)?;
        open::that(release_url.as_str()).map_err(|err| CoreError::Internal {
            reason: format!("failed to open release page: {err}"),
        })?;
        return Ok(InstallUpdateOutcome::OpenedReleasePage {
            url: release_url.to_string(),
        });
    };

    let download_url = validate_update_url(asset_url_raw, allow_local)?;
    let fallback_name = download_url
        .path_segments()
        .and_then(|mut segs| segs.next_back())
        .unwrap_or("ferrule-update");
    let filename = input
        .asset_name
        .as_deref()
        .and_then(sanitize_filename)
        .or_else(|| sanitize_filename(fallback_name))
        .unwrap_or_else(|| format!("ferrule-{}.bin", input.version));

    let updates_dir: PathBuf = std::env::temp_dir().join("ferrule-updates");
    fs::create_dir_all(&updates_dir).map_err(|err| CoreError::Storage {
        reason: format!("failed to create updates temp folder: {err}"),
    })?;

    let dest_path = updates_dir.join(&filename);
    let part_path = updates_dir.join(format!("{filename}.part"));

    let client = build_http_client(Duration::from_secs(300))?;
    let mut response = client
        .get(download_url)
        .send()
        .await
        .map_err(|err| CoreError::Network {
            reason: format!("failed to start update download: {err}"),
            retryable: true,
        })?;

    if !response.status().is_success() {
        return Err(CoreError::Network {
            reason: format!(
                "update download failed with HTTP {}",
                response.status().as_u16()
            ),
            retryable: response.status().is_server_error(),
        });
    }

    let total_bytes = response.content_length();
    let _ = app.emit(
        UPDATE_PROGRESS_EVENT,
        UpdateDownloadProgress {
            version: input.version.clone(),
            downloaded_bytes: 0,
            total_bytes,
            percent: total_bytes.map(|_| 0),
        },
    );

    let file = File::create(&part_path).map_err(|err| CoreError::Storage {
        reason: format!("failed to create update file: {err}"),
    })?;
    let mut writer = BufWriter::new(file);
    let mut downloaded_bytes: u64 = 0;
    let mut last_emit = Instant::now();

    loop {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                if let Err(err) = writer.write_all(&chunk) {
                    let _ = fs::remove_file(&part_path);
                    return Err(CoreError::Storage {
                        reason: format!("failed to write update file: {err}"),
                    });
                }
                downloaded_bytes = downloaded_bytes.saturating_add(chunk.len() as u64);
                if last_emit.elapsed() >= Duration::from_millis(80) {
                    last_emit = Instant::now();
                    let percent = total_bytes
                        .filter(|&t| t > 0)
                        .map(|t| ((downloaded_bytes.saturating_mul(100)) / t).min(100) as u8);
                    let _ = app.emit(
                        UPDATE_PROGRESS_EVENT,
                        UpdateDownloadProgress {
                            version: input.version.clone(),
                            downloaded_bytes,
                            total_bytes,
                            percent,
                        },
                    );
                }
            }
            Ok(None) => break,
            Err(err) => {
                let _ = fs::remove_file(&part_path);
                return Err(CoreError::Network {
                    reason: format!("update download interrupted: {err}"),
                    retryable: true,
                });
            }
        }
    }

    if let Err(err) = writer.flush() {
        let _ = fs::remove_file(&part_path);
        return Err(CoreError::Storage {
            reason: format!("failed to flush update file: {err}"),
        });
    }
    drop(writer);

    if dest_path.exists() {
        let _ = fs::remove_file(&dest_path);
    }
    fs::rename(&part_path, &dest_path).map_err(|err| {
        let _ = fs::remove_file(&part_path);
        CoreError::Storage {
            reason: format!("failed to finalize update file: {err}"),
        }
    })?;

    let _ = app.emit(
        UPDATE_PROGRESS_EVENT,
        UpdateDownloadProgress {
            version: input.version.clone(),
            downloaded_bytes,
            total_bytes: total_bytes.or(Some(downloaded_bytes)),
            percent: Some(100),
        },
    );

    launch_installer(&dest_path)?;

    Ok(InstallUpdateOutcome::InstallerLaunched {
        file_path: dest_path.to_string_lossy().into_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semver_comparison_handles_prefixes_and_prereleases() {
        assert!(is_newer_version("0.2.1", "0.2.2"));
        assert!(is_newer_version("0.2.1", "v0.3.0"));
        assert!(is_newer_version("0.2.1", "V1.0.0"));
        assert!(!is_newer_version("0.2.1", "v0.2.1"));
        assert!(!is_newer_version("0.3.0", "v0.2.9"));
        assert!(is_newer_version("0.3.0-rc.1", "0.3.0"));
        assert!(!is_newer_version("0.3.0", "0.3.0-rc.1"));
        assert!(is_newer_version("0.2.1", "0.3.0-rc.1"));
        assert!(!is_newer_version("0.2.1", "not-a-version"));
    }

    #[test]
    fn asset_selection_prefers_platform_installer() {
        let assets = vec![
            GitHubAsset {
                name: "Ferrule_0.3.0_x64_en-US.msi".into(),
                browser_download_url: "https://github.com/reytheist/ferrule/releases/download/v0.3.0/Ferrule_0.3.0_x64_en-US.msi".into(),
                size: Some(12_000_000),
            },
            GitHubAsset {
                name: "Ferrule_0.3.0_x64-setup.exe".into(),
                browser_download_url: "https://github.com/reytheist/ferrule/releases/download/v0.3.0/Ferrule_0.3.0_x64-setup.exe".into(),
                size: Some(10_500_000),
            },
            GitHubAsset {
                name: "Ferrule_0.3.0_amd64.AppImage".into(),
                browser_download_url: "https://github.com/reytheist/ferrule/releases/download/v0.3.0/Ferrule_0.3.0_amd64.AppImage".into(),
                size: Some(18_000_000),
            },
            GitHubAsset {
                name: "Ferrule_0.3.0_amd64.deb".into(),
                browser_download_url: "https://github.com/reytheist/ferrule/releases/download/v0.3.0/Ferrule_0.3.0_amd64.deb".into(),
                size: Some(11_000_000),
            },
            GitHubAsset {
                name: "Ferrule_0.3.0_aarch64.dmg".into(),
                browser_download_url: "https://github.com/reytheist/ferrule/releases/download/v0.3.0/Ferrule_0.3.0_aarch64.dmg".into(),
                size: Some(14_000_000),
            },
            GitHubAsset {
                name: "Ferrule_0.3.0_x64-setup.exe.sig".into(),
                browser_download_url: "https://github.com/reytheist/ferrule/releases/download/v0.3.0/Ferrule_0.3.0_x64-setup.exe.sig".into(),
                size: Some(256),
            },
        ];

        let win = select_release_asset(&assets, "windows", "x86_64").unwrap();
        assert_eq!(win.name, "Ferrule_0.3.0_x64-setup.exe");

        let linux = select_release_asset(&assets, "linux", "x86_64").unwrap();
        assert_eq!(linux.name, "Ferrule_0.3.0_amd64.AppImage");

        let mac = select_release_asset(&assets, "macos", "aarch64").unwrap();
        assert_eq!(mac.name, "Ferrule_0.3.0_aarch64.dmg");
    }

    #[test]
    fn sanitize_filename_strips_traversal_and_unsafe_chars() {
        assert_eq!(
            sanitize_filename("../../evil/Ferrule 0.3.0.exe"),
            Some("Ferrule_0.3.0.exe".into())
        );
        assert_eq!(sanitize_filename(".."), None);
        assert_eq!(sanitize_filename(".hidden"), None);
    }

    #[test]
    fn validate_update_url_enforces_https_and_github_hosts() {
        assert!(validate_update_url(
            "https://github.com/reytheist/ferrule/releases/download/v0.3.0/Ferrule.exe",
            false
        )
        .is_ok());
        assert!(validate_update_url(
            "https://objects.githubusercontent.com/some-asset",
            false
        )
        .is_ok());
        assert!(validate_update_url("http://github.com/reytheist/ferrule", false).is_err());
        assert!(validate_update_url("https://evil.example.com/ferrule.exe", false).is_err());
        assert!(validate_update_url("http://127.0.0.1:8787/ferrule.exe", false).is_err());
        assert!(validate_update_url("http://127.0.0.1:8787/ferrule.exe", true).is_ok());
    }

    #[test]
    fn parse_release_check_ignores_drafts_and_reports_newer_release() {
        let release = GitHubRelease {
            tag_name: "v0.3.0".into(),
            name: Some("Ferrule 0.3.0".into()),
            body: Some("Faster sync and in-app updates.".into()),
            html_url: "https://github.com/reytheist/ferrule/releases/tag/v0.3.0".into(),
            published_at: Some("2026-11-20T12:00:00Z".into()),
            draft: false,
            prerelease: false,
            assets: vec![GitHubAsset {
                name: "Ferrule_0.3.0_x64-setup.exe".into(),
                browser_download_url: "https://github.com/reytheist/ferrule/releases/download/v0.3.0/Ferrule_0.3.0_x64-setup.exe".into(),
                size: Some(9_500_000),
            }],
        };

        let result = parse_release_check("0.2.1", release, "windows", "x86_64");
        assert!(result.update_available);
        assert_eq!(result.latest_version.as_deref(), Some("0.3.0"));
        assert_eq!(
            result.release.as_ref().and_then(|r| r.asset.as_ref()).map(|a| a.name.as_str()),
            Some("Ferrule_0.3.0_x64-setup.exe")
        );
    }
}
