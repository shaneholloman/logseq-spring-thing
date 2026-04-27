use super::api::GitHubClient;
use super::types::GitHubFileBasicMetadata;
use crate::errors::VisionFlowResult;
use chrono::{DateTime, Utc};
use log::{debug, error, info, warn};
use serde_json::Value;
use std::sync::Arc;
use crate::utils::time;

#[derive(Clone)] 
pub struct EnhancedContentAPI {
    client: Arc<GitHubClient>,
}

impl EnhancedContentAPI {
    pub fn new(client: Arc<GitHubClient>) -> Self {
        Self { client }
    }

    /// List all markdown files using GitHub's Git Trees API (single API call).
    /// Returns all .md files under the configured base_path with their SHA hashes.
    /// This replaces the recursive Contents API approach that required one call per directory.
    pub async fn list_markdown_files_via_tree(
        &self,
    ) -> VisionFlowResult<Vec<GitHubFileBasicMetadata>> {
        let base_path = self.client.base_path().trim_matches('/').to_string();
        let branch = self.client.branch();

        // Git Trees API with recursive=1 returns the entire tree in one call
        let tree_url = format!(
            "https://api.github.com/repos/{}/{}/git/trees/{}?recursive=1",
            self.client.owner(),
            self.client.repo(),
            branch
        );

        info!("list_markdown_files_via_tree: Fetching tree from: {}", tree_url);

        let response = self
            .client
            .client()
            .get(&tree_url)
            .header("Authorization", format!("Bearer {}", self.client.token()))
            .header("Accept", "application/vnd.github+json")
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await?;
            error!(
                "list_markdown_files_via_tree: GitHub API error ({}): {}",
                status, error_text
            );
            return Err(format!(
                "GitHub Trees API error ({}): {}",
                status, error_text
            )
            .into());
        }

        let tree_data: Value = response.json().await?;
        let truncated = tree_data["truncated"].as_bool().unwrap_or(false);
        if truncated {
            warn!("list_markdown_files_via_tree: Tree response was truncated - some files may be missing");
        }

        let tree = tree_data["tree"]
            .as_array()
            .ok_or("GitHub Trees API returned no tree array")?;

        info!(
            "list_markdown_files_via_tree: Tree contains {} entries",
            tree.len()
        );

        let mut markdown_files = Vec::new();
        let base_prefix = if base_path.is_empty() {
            String::new()
        } else {
            format!("{}/", base_path)
        };

        for entry in tree {
            let entry_type = entry["type"].as_str().unwrap_or("");
            let entry_path = entry["path"].as_str().unwrap_or("");

            // Only process blob (file) entries that are .md files under base_path
            if entry_type != "blob" || !entry_path.ends_with(".md") {
                continue;
            }

            // Filter by base_path prefix
            if !base_path.is_empty() && !entry_path.starts_with(&base_prefix) {
                continue;
            }

            // Skip Logseq backup directories and non-content paths
            if entry_path.contains("/bak/")
                || entry_path.contains("/logseq/")
                || entry_path.contains("/.recycle/")
                || entry_path.contains("/journals/")
            {
                continue;
            }

            let sha = entry["sha"].as_str().unwrap_or("").to_string();
            let size = entry["size"].as_u64().unwrap_or(0);

            // Extract filename from path
            let name = entry_path
                .rsplit('/')
                .next()
                .unwrap_or(entry_path)
                .to_string();

            // Construct download URL from path
            let download_url = format!(
                "https://raw.githubusercontent.com/{}/{}/{}/{}",
                self.client.owner(),
                self.client.repo(),
                branch,
                entry_path
            );

            markdown_files.push(GitHubFileBasicMetadata {
                name,
                path: entry_path.to_string(),
                sha,
                size,
                download_url,
            });
        }

        info!(
            "list_markdown_files_via_tree: Found {} markdown files under '{}'",
            markdown_files.len(),
            base_path
        );
        Ok(markdown_files)
    }

    /// List markdown files via the Git Trees API for a specific base path.
    ///
    /// Unlike `list_markdown_files_via_tree()` which uses the client's configured
    /// base_path, this accepts an explicit path prefix. Used by the dual-graph
    /// sync loop to iterate over multiple graph directories.
    pub async fn list_markdown_files_via_tree_for_path(
        &self,
        base_path: &str,
    ) -> VisionFlowResult<Vec<GitHubFileBasicMetadata>> {
        let base_path = base_path.trim_matches('/').to_string();
        let branch = self.client.branch();

        let tree_url = format!(
            "https://api.github.com/repos/{}/{}/git/trees/{}?recursive=1",
            self.client.owner(),
            self.client.repo(),
            branch
        );

        info!("list_markdown_files_via_tree_for_path: Fetching tree for base '{}'", base_path);

        let response = self
            .client
            .client()
            .get(&tree_url)
            .header("Authorization", format!("Bearer {}", self.client.token()))
            .header("Accept", "application/vnd.github+json")
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await?;
            error!(
                "list_markdown_files_via_tree_for_path: GitHub API error ({}): {}",
                status, error_text
            );
            return Err(format!(
                "GitHub Trees API error ({}): {}",
                status, error_text
            )
            .into());
        }

        let tree_data: Value = response.json().await?;
        let truncated = tree_data["truncated"].as_bool().unwrap_or(false);
        if truncated {
            warn!("list_markdown_files_via_tree_for_path: Tree response was truncated");
        }

        let tree = tree_data["tree"]
            .as_array()
            .ok_or("GitHub Trees API returned no tree array")?;

        let mut markdown_files = Vec::new();
        let base_prefix = if base_path.is_empty() {
            String::new()
        } else {
            format!("{}/", base_path)
        };

        for entry in tree {
            let entry_type = entry["type"].as_str().unwrap_or("");
            let entry_path = entry["path"].as_str().unwrap_or("");

            if entry_type != "blob" || !entry_path.ends_with(".md") {
                continue;
            }

            if !base_path.is_empty() && !entry_path.starts_with(&base_prefix) {
                continue;
            }

            if entry_path.contains("/bak/")
                || entry_path.contains("/logseq/")
                || entry_path.contains("/.recycle/")
                || entry_path.contains("/journals/")
            {
                continue;
            }

            let sha = entry["sha"].as_str().unwrap_or("").to_string();
            let size = entry["size"].as_u64().unwrap_or(0);

            let name = entry_path
                .rsplit('/')
                .next()
                .unwrap_or(entry_path)
                .to_string();

            let download_url = format!(
                "https://raw.githubusercontent.com/{}/{}/{}/{}",
                self.client.owner(),
                self.client.repo(),
                branch,
                entry_path
            );

            markdown_files.push(GitHubFileBasicMetadata {
                name,
                path: entry_path.to_string(),
                sha,
                size,
                download_url,
            });
        }

        info!(
            "list_markdown_files_via_tree_for_path: Found {} markdown files under '{}'",
            markdown_files.len(),
            base_path
        );
        Ok(markdown_files)
    }

    pub fn list_markdown_files<'a>(
        &'a self,
        path: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = VisionFlowResult<Vec<GitHubFileBasicMetadata>>> + Send + 'a>> {
        Box::pin(async move {
            self.list_markdown_files_impl(path).await
        })
    }

    async fn list_markdown_files_impl(
        &self,
        path: &str,
    ) -> VisionFlowResult<Vec<GitHubFileBasicMetadata>> {
        let mut all_markdown_files = Vec::new();

        // GitHub Contents API returns all items in a single response (no pagination).
        // per_page/page params are ignored by this endpoint.
        let contents_url = GitHubClient::get_contents_url(&self.client, path).await;

        debug!("list_markdown_files: Fetching from: {}", contents_url);

        let response = self
            .client
            .client()
            .get(&contents_url)
            .header("Authorization", format!("Bearer {}", self.client.token()))
            .header("Accept", "application/vnd.github+json")
            .send()
            .await?;

        let status = response.status();
        debug!("list_markdown_files: Response status: {}", status);

        if !status.is_success() {
            let error_text = response.text().await?;
            error!(
                "list_markdown_files: GitHub API error for path '{}' ({}): {}",
                path, status, error_text
            );
            return Err(format!(
                "GitHub API error listing files for '{}' ({}): {}",
                path, status, error_text
            )
            .into());
        }

        let files: Vec<Value> = response.json().await?;
        info!(
            "list_markdown_files: Received {} items from GitHub for path '{}'",
            files.len(), path
        );

        for file in files {
            let file_type = file["type"].as_str().unwrap_or("unknown");
            let file_name = file["name"].as_str().unwrap_or("unnamed");

            if file_type == "file" && file_name.ends_with(".md") {
                debug!("list_markdown_files: Found markdown file: {}", file_name);
                all_markdown_files.push(GitHubFileBasicMetadata {
                    name: file_name.to_string(),
                    path: file["path"].as_str().unwrap_or("").to_string(),
                    sha: file["sha"].as_str().unwrap_or("").to_string(),
                    size: file["size"].as_u64().unwrap_or(0),
                    download_url: file["download_url"].as_str().unwrap_or("").to_string(),
                });
            } else if file_type == "dir" {
                let dir_path = file["path"].as_str().unwrap_or("");

                // Skip Logseq backup, recycle, and journal directories
                if dir_path.contains("/bak") || dir_path.contains("/logseq/")
                    || dir_path.contains("/.recycle") || dir_path.contains("/journals") {
                    debug!("list_markdown_files: Skipping excluded directory: {}", dir_path);
                    continue;
                }

                debug!("list_markdown_files: Recursively processing directory: {}", dir_path);

                match self.list_markdown_files(dir_path).await {
                    Ok(mut subdir_files) => {
                        let count = subdir_files.len();
                        debug!("list_markdown_files: Found {} files in subdirectory {}", count, dir_path);
                        all_markdown_files.append(&mut subdir_files);
                    }
                    Err(e) => {
                        warn!("list_markdown_files: Failed to process subdirectory {}: {}", dir_path, e);
                    }
                }
            }
        }

        info!(
            "list_markdown_files: Found {} markdown files total for path '{}'",
            all_markdown_files.len(), path
        );
        Ok(all_markdown_files)
    }

    
    pub async fn fetch_file_content(&self, download_url: &str) -> VisionFlowResult<String> {
        debug!("Fetching file content from: {}", download_url);
        let response = self
            .client
            .client()
            .get(download_url)
            .header("Authorization", format!("Bearer {}", self.client.token()))
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(format!("Failed to fetch file content: {}", error_text).into());
        }

        Ok(response.text().await?)
    }

    
    pub async fn get_file_content_last_modified(
        &self,
        file_path: &str,
        check_actual_changes: bool,
    ) -> VisionFlowResult<DateTime<Utc>> {
        let encoded_path = GitHubClient::get_full_path(&self.client, file_path).await;

        
        let commits_url = format!(
            "https://api.github.com/repos/{}/{}/commits",
            self.client.owner(),
            self.client.repo()
        );

        debug!("Fetching commits for path: {}", encoded_path);

        let response = self
            .client
            .client()
            .get(&commits_url)
            .header("Authorization", format!("Bearer {}", self.client.token()))
            .header("Accept", "application/vnd.github+json")
            .query(&[
                ("path", encoded_path.as_str()),
                ("ref", self.client.branch()),
                ("per_page", if check_actual_changes { "10" } else { "1" }),
            ])
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(format!("GitHub API error: {}", error_text).into());
        }

        let commits: Vec<Value> = response.json().await?;

        if commits.is_empty() {
            return Err(format!("No commit history found for {}", file_path).into());
        }

        
        if !check_actual_changes {
            return self.extract_commit_date(&commits[0]);
        }

        
        for commit in &commits {
            let sha = commit["sha"].as_str().ok_or("Missing commit SHA")?;

            if self.was_file_modified_in_commit(sha, &encoded_path).await? {
                debug!("File was actually modified in commit: {}", sha);
                return self.extract_commit_date(commit);
            } else {
                debug!(
                    "File was not modified in commit: {} (likely a merge commit)",
                    sha
                );
            }
        }

        
        warn!("No actual content changes found in recent commits, using oldest available");
        self.extract_commit_date(&commits[commits.len() - 1])
    }

    
    async fn was_file_modified_in_commit(
        &self,
        commit_sha: &str,
        file_path: &str,
    ) -> VisionFlowResult<bool> {
        let commit_url = format!(
            "https://api.github.com/repos/{}/{}/commits/{}",
            self.client.owner(),
            self.client.repo(),
            commit_sha
        );

        debug!("Checking commit {} for file changes", commit_sha);

        let response = self
            .client
            .client()
            .get(&commit_url)
            .header("Authorization", format!("Bearer {}", self.client.token()))
            .header("Accept", "application/vnd.github+json")
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            warn!("Failed to get commit details: {}", error_text);
            
            return Ok(true);
        }

        let commit_data: Value = response.json().await?;

        
        if let Some(files) = commit_data["files"].as_array() {
            for file in files {
                if let Some(filename) = file["filename"].as_str() {
                    
                    if filename == file_path
                        || filename.ends_with(&format!("/{}", file_path))
                        || filename == file_path.replace("%2F", "/")
                        || filename.ends_with(&format!("/{}", file_path.replace("%2F", "/")))
                    {
                        
                        let additions = file["additions"].as_u64().unwrap_or(0);
                        let deletions = file["deletions"].as_u64().unwrap_or(0);
                        let changes = file["changes"].as_u64().unwrap_or(0);

                        debug!(
                            "File {} in commit {}: +{} -{} (total: {} changes)",
                            filename, commit_sha, additions, deletions, changes
                        );

                        
                        return Ok(changes > 0);
                    }
                }
            }
        }

        
        Ok(false)
    }

    
    fn extract_commit_date(&self, commit: &Value) -> VisionFlowResult<DateTime<Utc>> {
        
        let date_str = commit["commit"]["committer"]["date"]
            .as_str()
            .or_else(|| commit["commit"]["author"]["date"].as_str())
            .ok_or("No commit date found")?;

        DateTime::parse_from_rfc3339(date_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| format!("Failed to parse date {}: {}", date_str, e).into())
    }

    
    pub async fn get_file_metadata_extended(
        &self,
        file_path: &str,
    ) -> VisionFlowResult<ExtendedFileMetadata> {
        let encoded_path = GitHubClient::get_full_path(&self.client, file_path).await;

        
        let contents_url = format!(
            "https://api.github.com/repos/{}/{}/contents/{}?ref={}",
            self.client.owner(),
            self.client.repo(),
            encoded_path,
            self.client.branch()
        );

        let response = self
            .client
            .client()
            .get(&contents_url)
            .header("Authorization", format!("Bearer {}", self.client.token()))
            .header("Accept", "application/vnd.github+json")
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(format!("Failed to get file metadata: {}", error_text).into());
        }

        let content_data: Value = response.json().await?;

        
        let last_content_modified = match self.get_file_content_last_modified(file_path, true).await
        {
            Ok(date) => date,
            Err(e) => {
                
                debug!(
                    "Could not get commit history for {}: {}. Using current time.",
                    file_path, e
                );
                time::now()
            }
        };

        Ok(ExtendedFileMetadata {
            name: content_data["name"].as_str().unwrap_or("").to_string(),
            path: content_data["path"].as_str().unwrap_or("").to_string(),
            sha: content_data["sha"].as_str().unwrap_or("").to_string(),
            size: content_data["size"].as_u64().unwrap_or(0),
            download_url: content_data["download_url"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            last_content_modified,
            file_type: content_data["type"].as_str().unwrap_or("file").to_string(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct ExtendedFileMetadata {
    pub name: String,
    pub path: String,
    pub sha: String,
    pub size: u64,
    pub download_url: String,
    pub last_content_modified: DateTime<Utc>,
    pub file_type: String,
}
