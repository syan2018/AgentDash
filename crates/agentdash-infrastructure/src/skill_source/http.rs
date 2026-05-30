//! HTTP implementation of the [`RemoteSkillSource`] SPI port.
//!
//! Owns the `reqwest` client plus the provider-specific traversal for GitHub,
//! ClawHub, and skills.sh. Returns untyped file bodies; the application layer
//! applies its own content-typing rules. Size/count limits are enforced here
//! on raw byte/text lengths, matching the previous in-application behavior.

use std::collections::VecDeque;

use agentdash_spi::platform::skill_source::{
    RemoteSkillFetch, RemoteSkillFile, RemoteSkillFileBody, RemoteSkillKind, RemoteSkillSource,
    RemoteSkillSourceError,
};
use async_trait::async_trait;
use reqwest::Url;
use serde::Deserialize;

type Error = RemoteSkillSourceError;

const MAX_REMOTE_SKILL_FILE_COUNT: usize = 48;
const MAX_REMOTE_SKILL_FILE_SIZE_BYTES: usize = 256 * 1024;
const MAX_REMOTE_SKILL_TOTAL_SIZE_BYTES: usize = 1024 * 1024;

fn bad_request(message: impl Into<String>) -> Error {
    RemoteSkillSourceError::BadRequest(message.into())
}

fn url_encode(s: &str) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => {
                write!(out, "%{byte:02X}").unwrap();
            }
        }
    }
    out
}

/// Remote skill source backed by `reqwest`.
#[derive(Debug, Default, Clone)]
pub struct HttpRemoteSkillSource;

impl HttpRemoteSkillSource {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl RemoteSkillSource for HttpRemoteSkillSource {
    async fn fetch(&self, url: &str) -> Result<RemoteSkillFetch, RemoteSkillSourceError> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|error| RemoteSkillSourceError::Internal(error.to_string()))?;

        match detect_import_source(url)? {
            DetectedImportSource::Github(source) => {
                let normalized_url = source.normalized_url.clone();
                let files = fetch_github_skill_files(&client, &source).await?;
                Ok(RemoteSkillFetch {
                    kind: RemoteSkillKind::Github,
                    normalized_url,
                    files,
                })
            }
            DetectedImportSource::Clawhub { normalized_url } => {
                let files = fetch_clawhub_skill_files(&client, &normalized_url).await?;
                Ok(RemoteSkillFetch {
                    kind: RemoteSkillKind::Clawhub,
                    normalized_url,
                    files,
                })
            }
            DetectedImportSource::SkillsSh { normalized_url } => {
                let files = fetch_skills_sh_skill_files(&client, &normalized_url).await?;
                Ok(RemoteSkillFetch {
                    kind: RemoteSkillKind::SkillsSh,
                    normalized_url,
                    files,
                })
            }
        }
    }
}

// ─── source detection ────────────────────────────────────────────────────────

#[derive(Debug)]
enum DetectedImportSource {
    Github(GithubSkillSource),
    Clawhub { normalized_url: String },
    SkillsSh { normalized_url: String },
}

fn detect_import_source(raw: &str) -> Result<DetectedImportSource, Error> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(bad_request("远端 Skill URL 不能为空"));
    }

    let with_scheme = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };

    let url = Url::parse(&with_scheme).map_err(|_| bad_request("远端 Skill URL 格式非法"))?;

    let host = url.host_str().unwrap_or("").to_lowercase();
    match host.as_str() {
        "github.com" | "www.github.com" => {
            let source = parse_github_skill_source(trimmed)?;
            Ok(DetectedImportSource::Github(source))
        }
        "clawhub.ai" | "www.clawhub.ai" | "clawhub.com" | "www.clawhub.com" => {
            Ok(DetectedImportSource::Clawhub {
                normalized_url: with_scheme,
            })
        }
        "skills.sh" | "www.skills.sh" => Ok(DetectedImportSource::SkillsSh {
            normalized_url: with_scheme,
        }),
        _ => Err(bad_request(format!(
            "不支持的来源: {host}（支持 github.com / clawhub.ai / skills.sh）"
        ))),
    }
}

// ─── GitHub types ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
struct GithubSkillSource {
    owner: String,
    repo: String,
    ref_name: Option<String>,
    skill_dir: String,
    normalized_url: String,
}

#[derive(Debug, Deserialize)]
struct GithubRepoInfo {
    default_branch: String,
}

#[derive(Debug, Deserialize)]
struct GithubContentEntry {
    path: String,
    #[serde(rename = "type")]
    entry_type: String,
    size: Option<u64>,
    download_url: Option<String>,
}

fn parse_github_skill_source(raw: &str) -> Result<GithubSkillSource, Error> {
    let url = Url::parse(raw.trim()).map_err(|_| bad_request("远端 Skill URL 必须是合法 URL"))?;
    if url.scheme() != "https"
        || !matches!(url.host_str(), Some("github.com") | Some("www.github.com"))
    {
        return Err(bad_request("GitHub URL 必须以 https://github.com/ 开头"));
    }

    let segments = url
        .path_segments()
        .map(|segments| segments.filter(|part| !part.is_empty()).collect::<Vec<_>>())
        .unwrap_or_default();
    if segments.len() < 2 {
        return Err(bad_request("GitHub URL 必须包含 owner/repo"));
    }
    let owner = segments[0].to_string();
    let repo = segments[1].trim_end_matches(".git").to_string();
    let mut ref_name = None;
    let mut skill_dir = String::new();

    match segments.get(2).copied() {
        None => {}
        Some("tree") => {
            let Some(ref_segment) = segments.get(3) else {
                return Err(bad_request("GitHub tree URL 缺少 ref"));
            };
            ref_name = Some((*ref_segment).to_string());
            skill_dir = segments
                .get(4..)
                .unwrap_or_default()
                .join("/")
                .trim_matches('/')
                .to_string();
        }
        Some("blob") => {
            let Some(ref_segment) = segments.get(3) else {
                return Err(bad_request("GitHub blob URL 缺少 ref"));
            };
            ref_name = Some((*ref_segment).to_string());
            let blob_path = segments
                .get(4..)
                .unwrap_or_default()
                .join("/")
                .trim_matches('/')
                .to_string();
            if !blob_path.ends_with("SKILL.md") {
                return Err(bad_request("GitHub blob URL 必须指向 SKILL.md"));
            }
            skill_dir = blob_path
                .strip_suffix("SKILL.md")
                .unwrap_or("")
                .trim_matches('/')
                .to_string();
        }
        Some(other) => {
            return Err(bad_request(format!(
                "不支持的 GitHub Skill URL 路径: {other}"
            )));
        }
    }

    Ok(GithubSkillSource {
        owner,
        repo,
        ref_name,
        skill_dir,
        normalized_url: raw.trim().to_string(),
    })
}

async fn fetch_github_skill_files(
    client: &reqwest::Client,
    source: &GithubSkillSource,
) -> Result<Vec<RemoteSkillFile>, Error> {
    let ref_name = match &source.ref_name {
        Some(ref_name) => ref_name.clone(),
        None => fetch_default_branch(client, source).await?,
    };
    let mut entries = Vec::new();
    collect_github_directory_entries(client, source, &ref_name, &source.skill_dir, &mut entries)
        .await?;
    if entries.len() > MAX_REMOTE_SKILL_FILE_COUNT {
        return Err(bad_request(format!(
            "远端 Skill 文件数量不能超过 {MAX_REMOTE_SKILL_FILE_COUNT}"
        )));
    }
    if !entries.iter().any(|entry| {
        entry.entry_type == "file"
            && relative_github_path(&source.skill_dir, &entry.path) == "SKILL.md"
    }) {
        return Err(bad_request("GitHub 目录下未找到 SKILL.md"));
    }

    let mut total_size = 0usize;
    let mut files = Vec::new();
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    for entry in entries
        .into_iter()
        .filter(|entry| entry.entry_type == "file")
    {
        let relative_path = relative_github_path(&source.skill_dir, &entry.path);
        if relative_path.is_empty() {
            continue;
        }
        let declared_size = entry.size.unwrap_or(0) as usize;
        if declared_size > MAX_REMOTE_SKILL_FILE_SIZE_BYTES {
            return Err(bad_request(format!("远端 Skill 文件过大: {relative_path}")));
        }
        let download_url = entry
            .download_url
            .ok_or_else(|| bad_request(format!("GitHub 文件缺少 download_url: {}", entry.path)))?;
        let bytes = fetch_github_file(client, &download_url, &relative_path).await?;
        total_size = total_size.saturating_add(bytes.len());
        if total_size > MAX_REMOTE_SKILL_TOTAL_SIZE_BYTES {
            return Err(bad_request(format!(
                "远端 Skill 总大小不能超过 {} KB",
                MAX_REMOTE_SKILL_TOTAL_SIZE_BYTES / 1024
            )));
        }
        files.push(RemoteSkillFile {
            path: relative_path,
            body: RemoteSkillFileBody::Bytes(bytes),
        });
    }
    Ok(files)
}

async fn fetch_default_branch(
    client: &reqwest::Client,
    source: &GithubSkillSource,
) -> Result<String, Error> {
    let url = format!(
        "https://api.github.com/repos/{}/{}",
        source.owner, source.repo
    );
    let repo = github_get(client, &url).await?;
    let repo: GithubRepoInfo = serde_json::from_value(repo)
        .map_err(|error| bad_request(format!("GitHub repo 响应解析失败: {error}")))?;
    Ok(repo.default_branch)
}

async fn collect_github_directory_entries(
    client: &reqwest::Client,
    source: &GithubSkillSource,
    ref_name: &str,
    dir_path: &str,
    entries: &mut Vec<GithubContentEntry>,
) -> Result<(), Error> {
    let mut pending = VecDeque::from([dir_path.trim_matches('/').to_string()]);
    while let Some(current_dir) = pending.pop_front() {
        let url = if current_dir.is_empty() {
            format!(
                "https://api.github.com/repos/{}/{}/contents?ref={}",
                source.owner, source.repo, ref_name
            )
        } else {
            format!(
                "https://api.github.com/repos/{}/{}/contents/{}?ref={}",
                source.owner, source.repo, current_dir, ref_name
            )
        };
        let value = github_get(client, &url).await?;
        let list = value
            .as_array()
            .ok_or_else(|| bad_request("GitHub Skill URL 必须指向目录"))?;
        for item in list {
            let entry: GithubContentEntry = serde_json::from_value(item.clone())
                .map_err(|error| bad_request(format!("GitHub contents 响应解析失败: {error}")))?;
            if entry.entry_type == "dir" {
                pending.push_back(entry.path);
            } else {
                entries.push(entry);
            }
            if entries.len() > MAX_REMOTE_SKILL_FILE_COUNT {
                return Err(bad_request(format!(
                    "远端 Skill 文件数量不能超过 {MAX_REMOTE_SKILL_FILE_COUNT}"
                )));
            }
        }
    }
    Ok(())
}

async fn github_get(client: &reqwest::Client, url: &str) -> Result<serde_json::Value, Error> {
    let response = client
        .get(url)
        .header(reqwest::header::USER_AGENT, "AgentDash")
        .send()
        .await
        .map_err(|error| bad_request(format!("GitHub 请求失败: {error}")))?;
    if !response.status().is_success() {
        return Err(bad_request(format!(
            "GitHub 请求失败: HTTP {}",
            response.status()
        )));
    }
    response
        .json()
        .await
        .map_err(|error| bad_request(format!("GitHub 响应解析失败: {error}")))
}

async fn fetch_github_file(
    client: &reqwest::Client,
    download_url: &str,
    path: &str,
) -> Result<Vec<u8>, Error> {
    let response = client
        .get(download_url)
        .header(reqwest::header::USER_AGENT, "AgentDash")
        .send()
        .await
        .map_err(|error| bad_request(format!("下载 GitHub 文件失败: {path}: {error}")))?;
    if !response.status().is_success() {
        return Err(bad_request(format!(
            "下载 GitHub 文件失败: {path}: HTTP {}",
            response.status()
        )));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|error| bad_request(format!("读取 GitHub 文件失败: {path}: {error}")))?;
    if bytes.len() > MAX_REMOTE_SKILL_FILE_SIZE_BYTES {
        return Err(bad_request(format!("远端 Skill 文件过大: {path}")));
    }
    Ok(bytes.to_vec())
}

fn relative_github_path(base_dir: &str, path: &str) -> String {
    let base = base_dir.trim_matches('/');
    let normalized = path.trim_matches('/');
    if base.is_empty() {
        normalized.to_string()
    } else {
        normalized
            .strip_prefix(base)
            .unwrap_or(normalized)
            .trim_matches('/')
            .to_string()
    }
}

// ─── ClawHub import ──────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ClawhubGetSkillResponse {
    skill: ClawhubSkill,
    #[serde(rename = "latestVersion")]
    latest_version: Option<ClawhubLatestVersion>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ClawhubSkill {
    slug: String,
    #[serde(rename = "displayName")]
    display_name: Option<String>,
    summary: Option<String>,
    tags: Option<std::collections::HashMap<String, String>>,
}

#[derive(Debug, Deserialize)]
struct ClawhubLatestVersion {
    version: String,
}

#[derive(Debug, Deserialize)]
struct ClawhubVersionDetailResponse {
    version: ClawhubVersionDetail,
}

#[derive(Debug, Deserialize)]
struct ClawhubVersionDetail {
    files: Vec<ClawhubFileEntry>,
}

#[derive(Debug, Deserialize)]
struct ClawhubFileEntry {
    path: String,
}

fn parse_clawhub_slug(raw: &str) -> Result<String, Error> {
    let url = Url::parse(raw).map_err(|_| bad_request("ClawHub URL 格式非法"))?;
    let parts: Vec<&str> = url
        .path()
        .trim_matches('/')
        .split('/')
        .filter(|p| !p.is_empty())
        .collect();
    // clawhub.ai/{owner}/{slug} → slug is last segment
    if parts.len() == 2 {
        return Ok(parts[1].to_string());
    }
    if parts.len() == 1 && !parts[0].is_empty() {
        return Ok(parts[0].to_string());
    }
    Err(bad_request("ClawHub URL 缺少 skill slug"))
}

async fn fetch_clawhub_skill_files(
    client: &reqwest::Client,
    raw_url: &str,
) -> Result<Vec<RemoteSkillFile>, Error> {
    let slug = parse_clawhub_slug(raw_url)?;
    let api_base = "https://clawhub.ai/api/v1";

    // 1. Fetch skill metadata
    let skill_resp: ClawhubGetSkillResponse = client
        .get(format!("{api_base}/skills/{}", url_encode(&slug)))
        .send()
        .await
        .map_err(|e| bad_request(format!("ClawHub 请求失败: {e}")))?
        .error_for_status()
        .map_err(|e| bad_request(format!("ClawHub skill 未找到: {slug} ({e})")))?
        .json()
        .await
        .map_err(|e| bad_request(format!("ClawHub 响应解析失败: {e}")))?;

    // 2. Determine latest version
    let latest_version = skill_resp
        .skill
        .tags
        .as_ref()
        .and_then(|tags| tags.get("latest").cloned())
        .or(skill_resp.latest_version.map(|v| v.version));

    // 3. Fetch file list from version detail
    let file_paths: Vec<String> = if let Some(ref version) = latest_version {
        let version_url = format!(
            "{api_base}/skills/{}/versions/{}",
            url_encode(&slug),
            url_encode(version)
        );
        match client.get(&version_url).send().await {
            Ok(resp) if resp.status().is_success() => {
                let detail: ClawhubVersionDetailResponse =
                    resp.json().await.unwrap_or(ClawhubVersionDetailResponse {
                        version: ClawhubVersionDetail { files: vec![] },
                    });
                detail.version.files.into_iter().map(|f| f.path).collect()
            }
            _ => vec!["SKILL.md".to_string()],
        }
    } else {
        vec!["SKILL.md".to_string()]
    };

    // 4. Download each file
    let mut files = Vec::new();
    let mut total_size = 0usize;
    for fp in &file_paths {
        let mut file_url = format!(
            "{api_base}/skills/{}/file?path={}",
            url_encode(&slug),
            url_encode(fp)
        );
        if let Some(ref version) = latest_version {
            file_url.push_str(&format!("&version={}", url_encode(version)));
        }

        let resp = client.get(&file_url).send().await;
        let body = match resp {
            Ok(r) if r.status().is_success() => r.text().await.ok(),
            _ => None,
        };

        let Some(content) = body else {
            if fp == "SKILL.md" {
                return Err(bad_request(format!(
                    "ClawHub 导入失败: 无法下载 SKILL.md ({slug})"
                )));
            }
            continue;
        };

        if content.len() > MAX_REMOTE_SKILL_FILE_SIZE_BYTES {
            return Err(bad_request(format!("ClawHub 文件过大: {fp}")));
        }
        total_size = total_size.saturating_add(content.len());
        if total_size > MAX_REMOTE_SKILL_TOTAL_SIZE_BYTES {
            return Err(bad_request(format!(
                "ClawHub 导入总大小超出限制 ({} KB)",
                MAX_REMOTE_SKILL_TOTAL_SIZE_BYTES / 1024
            )));
        }

        files.push(RemoteSkillFile {
            path: fp.clone(),
            body: RemoteSkillFileBody::Text(content),
        });
    }

    if !files.iter().any(|f| f.path == "SKILL.md") {
        return Err(bad_request(format!("ClawHub skill 缺少 SKILL.md: {slug}")));
    }
    Ok(files)
}

// ─── skills.sh import (GitHub-backed) ────────────────────────────────────────

/// skills.sh URL format: https://skills.sh/{owner}/{repo}/{skill-name}
fn parse_skills_sh_parts(raw: &str) -> Result<(String, String, String), Error> {
    let url = Url::parse(raw).map_err(|_| bad_request("skills.sh URL 格式非法"))?;
    let parts: Vec<&str> = url
        .path()
        .trim_matches('/')
        .split('/')
        .filter(|p| !p.is_empty())
        .collect();
    if parts.len() != 3 {
        return Err(bad_request(format!(
            "skills.sh URL 需要 skills.sh/{{owner}}/{{repo}}/{{skill-name}} 格式，当前路径: {}",
            url.path()
        )));
    }
    Ok((
        parts[0].to_string(),
        parts[1].to_string(),
        parts[2].to_string(),
    ))
}

async fn fetch_skills_sh_skill_files(
    client: &reqwest::Client,
    raw_url: &str,
) -> Result<Vec<RemoteSkillFile>, Error> {
    let (owner, repo, skill_name) = parse_skills_sh_parts(raw_url)?;

    // Get default branch
    let default_branch = fetch_skills_sh_default_branch(client, &owner, &repo).await;
    let raw_prefix = format!("https://raw.githubusercontent.com/{owner}/{repo}/{default_branch}");

    // Try candidate paths for SKILL.md
    let candidate_dirs = [
        format!("skills/{skill_name}"),
        format!(".claude/skills/{skill_name}"),
        format!("plugin/skills/{skill_name}"),
        skill_name.clone(),
    ];

    let mut skill_md_content: Option<String> = None;
    let mut skill_dir = String::new();
    for dir in &candidate_dirs {
        let url = format!("{raw_prefix}/{dir}/SKILL.md");
        if let Ok(content) = fetch_raw_text_file(client, &url).await {
            skill_md_content = Some(content);
            skill_dir = dir.clone();
            break;
        }
    }

    // Try root-level SKILL.md as fallback
    if skill_md_content.is_none() {
        let url = format!("{raw_prefix}/SKILL.md");
        if let Ok(content) = fetch_raw_text_file(client, &url).await {
            skill_md_content = Some(content);
            skill_dir = String::new();
        }
    }

    let skill_md_content = skill_md_content.ok_or_else(|| {
        bad_request(format!(
            "skills.sh 导入失败: 在 {owner}/{repo} 中未找到 skill `{skill_name}` 的 SKILL.md"
        ))
    })?;

    let mut total_size = skill_md_content.len();
    let mut files = vec![RemoteSkillFile {
        path: "SKILL.md".to_string(),
        body: RemoteSkillFileBody::Text(skill_md_content),
    }];

    // List supporting files via GitHub API
    let github_source = GithubSkillSource {
        owner,
        repo,
        ref_name: Some(default_branch.clone()),
        skill_dir: skill_dir.clone(),
        normalized_url: raw_url.to_string(),
    };
    let mut entries = Vec::new();
    if collect_github_directory_entries(
        client,
        &github_source,
        &default_branch,
        &skill_dir,
        &mut entries,
    )
    .await
    .is_ok()
    {
        for entry in entries.into_iter().filter(|e| e.entry_type == "file") {
            let relative_path = relative_github_path(&skill_dir, &entry.path);
            if relative_path.is_empty() || relative_path == "SKILL.md" {
                continue;
            }
            if let Some(download_url) = entry.download_url {
                if let Ok(bytes) = fetch_github_file(client, &download_url, &relative_path).await {
                    total_size = total_size.saturating_add(bytes.len());
                    if total_size > MAX_REMOTE_SKILL_TOTAL_SIZE_BYTES {
                        break;
                    }
                    files.push(RemoteSkillFile {
                        path: relative_path,
                        body: RemoteSkillFileBody::Bytes(bytes),
                    });
                }
            }
        }
    }

    Ok(files)
}

async fn fetch_skills_sh_default_branch(
    client: &reqwest::Client,
    owner: &str,
    repo: &str,
) -> String {
    let url = format!("https://api.github.com/repos/{owner}/{repo}");
    match github_get(client, &url).await {
        Ok(value) => value
            .get("default_branch")
            .and_then(|v| v.as_str())
            .unwrap_or("main")
            .to_string(),
        Err(_) => "main".to_string(),
    }
}

/// Generic raw file fetch (unlike `fetch_github_file` which returns specific errors)
async fn fetch_raw_text_file(client: &reqwest::Client, url: &str) -> Result<String, Error> {
    let response = client
        .get(url)
        .header(reqwest::header::USER_AGENT, "AgentDash")
        .send()
        .await
        .map_err(|e| bad_request(format!("下载失败: {e}")))?;
    if !response.status().is_success() {
        return Err(bad_request(format!("HTTP {}", response.status())));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|e| bad_request(format!("读取响应失败: {e}")))?;
    if bytes.len() > MAX_REMOTE_SKILL_FILE_SIZE_BYTES {
        return Err(bad_request("文件过大"));
    }
    String::from_utf8(bytes.to_vec()).map_err(|_| bad_request("文件非 UTF-8 文本"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_github_source_extracts_owner_repo() {
        let repo = parse_github_skill_source("https://github.com/acme/skills")
            .expect("should parse repo url");
        assert_eq!(repo.owner, "acme");
        assert_eq!(repo.repo, "skills");
        assert_eq!(repo.ref_name, None);
        assert_eq!(repo.skill_dir, "");
    }

    #[test]
    fn parse_github_source_extracts_tree_ref_and_dir() {
        let tree =
            parse_github_skill_source("https://github.com/acme/skills/tree/main/research/writer")
                .expect("should parse tree url");
        assert_eq!(tree.ref_name.as_deref(), Some("main"));
        assert_eq!(tree.skill_dir, "research/writer");
    }

    #[test]
    fn parse_github_source_extracts_blob_skill_dir() {
        let blob = parse_github_skill_source(
            "https://github.com/acme/skills/blob/main/research/writer/SKILL.md",
        )
        .expect("should parse blob url");
        assert_eq!(blob.ref_name.as_deref(), Some("main"));
        assert_eq!(blob.skill_dir, "research/writer");
    }

    #[test]
    fn parse_github_source_rejects_blob_not_pointing_to_skill_md() {
        let err =
            parse_github_skill_source("https://github.com/acme/skills/blob/main/research/notes.md")
                .expect_err("blob must point to SKILL.md");
        assert!(matches!(err, RemoteSkillSourceError::BadRequest(_)));
    }
}
