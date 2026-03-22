use std::fs;
use std::path::Path;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// Git file modes
#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)]
pub enum GitMode {
    #[serde(rename = "100644")]
    RegularFile,
    #[serde(rename = "100755")]
    ExecutableFile,
    #[serde(rename = "040000")]
    Directory,
    #[serde(rename = "120000")]
    SymbolicLink,
}

impl std::fmt::Display for GitMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GitMode::RegularFile => write!(f, "100644"),
            GitMode::ExecutableFile => write!(f, "100755"),
            GitMode::Directory => write!(f, "040000"),
            GitMode::SymbolicLink => write!(f, "120000"),
        }
    }
}

/// A blob object ready for tree creation
#[derive(Debug, Clone, Serialize)]
pub struct GitBlob {
    pub path: String,
    pub mode: GitMode,
    #[serde(rename = "type")]
    pub object_type: String,
    pub sha: String,
}

// --- API response types ---

#[derive(Deserialize)]
struct RefObject {
    sha: String,
    #[serde(rename = "type")]
    object_type: String,
}

#[derive(Deserialize)]
struct RefResponse {
    object: RefObject,
}

#[derive(Deserialize)]
struct TagObject {
    sha: String,
}

#[derive(Deserialize)]
struct TagResponse {
    object: TagObject,
}

#[derive(Deserialize)]
struct TreeRef {
    sha: String,
}

#[derive(Deserialize)]
struct CommitResponse {
    tree: TreeRef,
}

#[derive(Deserialize)]
struct GetCommitResponse {
    sha: String,
}

// Wrapper for endpoints that return { sha: ... }
#[derive(Deserialize)]
struct ShaResponse {
    sha: String,
}

#[derive(Deserialize)]
struct UpdateRefResponse {
    object: RefObject,
}

// --- Public functions ---

/// Build commit message from inline string or file contents.
pub fn build_commit_message(message: &str, file: &str) -> Result<String, String> {
    let output = if !file.is_empty() {
        fs::read_to_string(file).map_err(|e| format!("Failed to read message file: {e}"))?
    } else {
        message.to_string()
    };

    if output.is_empty() {
        Err("Commit message is empty".to_string())
    } else {
        Ok(output)
    }
}

/// Normalize a ref to `heads/<ref>` or `tags/<ref>` format.
pub fn normalize_ref(git_ref: &str) -> String {
    if git_ref.starts_with("heads/") || git_ref.starts_with("tags/") {
        git_ref.to_string()
    } else if let Some(stripped) = git_ref.strip_prefix("refs/") {
        stripped.to_string()
    } else {
        format!("heads/{git_ref}")
    }
}

/// Determine the git file mode for a path.
pub fn get_file_mode(file: &Path, follow_symlinks: bool) -> Result<GitMode, String> {
    let metadata = if follow_symlinks {
        fs::symlink_metadata(file)
    } else {
        fs::metadata(file)
    };

    let metadata = metadata.map_err(|e| format!("Failed to stat {}: {e}", file.display()))?;

    if metadata.is_symlink() {
        Ok(GitMode::SymbolicLink)
    } else if metadata.is_dir() {
        Ok(GitMode::Directory)
    } else if metadata.is_file() {
        // On Unix, check executable bit
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o111 != 0 {
                return Ok(GitMode::ExecutableFile);
            }
        }
        Ok(GitMode::RegularFile)
    } else {
        Err(format!("Unknown file mode for {}", file.display()))
    }
}

/// GitHub API client for git operations.
pub struct GitHubApi {
    client: Client,
    base_url: String,
    owner: String,
    repo: String,
    token: String,
    max_retries: u32,
}

impl GitHubApi {
    pub fn new(base_url: &str, owner: &str, repo: &str, token: &str, max_retries: u32) -> Self {
        let client = Client::new();
        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            owner: owner.to_string(),
            repo: repo.to_string(),
            token: token.to_string(),
            max_retries,
        }
    }

    /// Make a GET request with retry logic.
    async fn get(&self, path: &str) -> Result<reqwest::Response, String> {
        let url = format!(
            "{}/repos/{}/{}/{}",
            self.base_url, self.owner, self.repo, path
        );
        self.request_with_retry(self.client.get(&url)).await
    }

    /// Make a POST request with retry logic.
    async fn post(&self, path: &str, body: &impl Serialize) -> Result<reqwest::Response, String> {
        let url = format!(
            "{}/repos/{}/{}/{}",
            self.base_url, self.owner, self.repo, path
        );
        self.request_with_retry(self.client.post(&url).json(body))
            .await
    }

    /// Make a PATCH request with retry logic.
    async fn patch(&self, path: &str, body: &impl Serialize) -> Result<reqwest::Response, String> {
        let url = format!(
            "{}/repos/{}/{}/{}",
            self.base_url, self.owner, self.repo, path
        );
        self.request_with_retry(self.client.patch(&url).json(body))
            .await
    }

    async fn request_with_retry(
        &self,
        builder: reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, String> {
        let mut last_err = String::new();

        for attempt in 0..=self.max_retries {
            let request = builder
                .try_clone()
                .ok_or("Failed to clone request")?
                .header("Authorization", format!("Bearer {}", self.token))
                .header("Accept", "application/vnd.github+json")
                .header("User-Agent", "verified-bot-commit")
                .header("X-GitHub-Api-Version", "2022-11-28");

            match request.send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        return Ok(resp);
                    }

                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();

                    // Handle rate limiting
                    if status.as_u16() == 429 || status.as_u16() == 403 {
                        if attempt < self.max_retries {
                            eprintln!(
                                "::warning::Rate limited (attempt {}/{}), retrying...",
                                attempt + 1,
                                self.max_retries
                            );
                            tokio::time::sleep(std::time::Duration::from_secs(2u64.pow(attempt)))
                                .await;
                            continue;
                        }
                    }

                    last_err = format!("HTTP {status}: {body}");
                    if attempt < self.max_retries {
                        eprintln!(
                            "::warning::Request failed (attempt {}/{}): {last_err}",
                            attempt + 1,
                            self.max_retries
                        );
                        tokio::time::sleep(std::time::Duration::from_secs(2u64.pow(attempt))).await;
                        continue;
                    }
                }
                Err(e) => {
                    last_err = e.to_string();
                    if attempt < self.max_retries {
                        eprintln!(
                            "::warning::Request error (attempt {}/{}): {last_err}",
                            attempt + 1,
                            self.max_retries
                        );
                        tokio::time::sleep(std::time::Duration::from_secs(2u64.pow(attempt))).await;
                        continue;
                    }
                }
            }
        }

        Err(last_err)
    }

    /// Get the commit SHA for a ref.
    pub async fn get_ref(&self, git_ref: &str) -> Result<String, String> {
        let resp = self.get(&format!("git/ref/{git_ref}")).await?;
        let data: RefResponse = resp.json().await.map_err(|e| e.to_string())?;

        match data.object.object_type.as_str() {
            "tag" => {
                let resp = self.get(&format!("git/tags/{}", data.object.sha)).await?;
                let tag: TagResponse = resp.json().await.map_err(|e| e.to_string())?;
                Ok(tag.object.sha)
            }
            "commit" => Ok(data.object.sha),
            other => Err(format!("Unsupported ref type: {other}")),
        }
    }

    /// Get the tree SHA from a commit.
    pub async fn get_tree(&self, commit_sha: &str) -> Result<String, String> {
        let resp = self.get(&format!("git/commits/{commit_sha}")).await?;
        let data: CommitResponse = resp.json().await.map_err(|e| e.to_string())?;
        Ok(data.tree.sha)
    }

    /// Create a blob from a local file.
    pub async fn create_blob(
        &self,
        file: &str,
        workspace: &str,
        follow_symlinks: bool,
    ) -> Result<GitBlob, String> {
        let location = Path::new(workspace).join(file);
        let mode = get_file_mode(&location, follow_symlinks)?;
        let content = fs::read(&location)
            .map_err(|e| format!("Failed to read {}: {e}", location.display()))?;
        let encoded = BASE64.encode(&content);

        let body = serde_json::json!({
            "encoding": "base64",
            "content": encoded,
        });

        let resp = self.post("git/blobs", &body).await?;
        let data: ShaResponse = resp.json().await.map_err(|e| e.to_string())?;

        Ok(GitBlob {
            path: file.to_string(),
            object_type: "blob".to_string(),
            mode,
            sha: data.sha,
        })
    }

    /// Create a tree from blobs.
    pub async fn create_tree(&self, blobs: &[GitBlob], base_tree: &str) -> Result<String, String> {
        let body = serde_json::json!({
            "base_tree": base_tree,
            "tree": blobs,
        });

        let resp = self.post("git/trees", &body).await?;
        let data: ShaResponse = resp.json().await.map_err(|e| e.to_string())?;
        Ok(data.sha)
    }

    /// Create a commit.
    pub async fn create_commit(
        &self,
        tree: &str,
        parent: &str,
        message: &str,
    ) -> Result<String, String> {
        let body = serde_json::json!({
            "parents": [parent],
            "message": message,
            "tree": tree,
        });

        let resp = self.post("git/commits", &body).await?;
        let data: GetCommitResponse = resp.json().await.map_err(|e| e.to_string())?;
        Ok(data.sha)
    }

    /// Update a ref to point to a new SHA.
    pub async fn update_ref(
        &self,
        git_ref: &str,
        sha: &str,
        force: bool,
    ) -> Result<String, String> {
        let body = serde_json::json!({
            "sha": sha,
            "force": force,
        });

        let resp = self.patch(&format!("git/refs/{git_ref}"), &body).await?;
        let data: UpdateRefResponse = resp.json().await.map_err(|e| e.to_string())?;
        Ok(data.object.sha)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_commit_message_inline() {
        let msg = build_commit_message("Some message", "").unwrap();
        assert_eq!(msg, "Some message");
    }

    #[test]
    fn test_build_commit_message_empty() {
        let result = build_commit_message("", "");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Commit message is empty");
    }

    #[test]
    fn test_normalize_ref() {
        assert_eq!(normalize_ref("test"), "heads/test");
        assert_eq!(normalize_ref("heads/test"), "heads/test");
        assert_eq!(normalize_ref("refs/heads/test"), "heads/test");
        assert_eq!(normalize_ref("feat/test"), "heads/feat/test");
        assert_eq!(normalize_ref("heads/feat/test"), "heads/feat/test");
        assert_eq!(normalize_ref("refs/heads/refs"), "heads/refs");
        assert_eq!(normalize_ref("refs/tags/test-tag"), "tags/test-tag");
        assert_eq!(normalize_ref("tags/v1.0"), "tags/v1.0");
    }
}
