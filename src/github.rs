use anyhow::{Context, Result, bail};

/// Parsed components of a GitHub file or directory URL.
#[derive(Debug, Clone)]
pub struct GitHubFileRef {
    pub owner: String,
    pub repo: String,
    pub git_ref: String,
    pub path: String,
}

impl GitHubFileRef {
    /// Build a raw.githubusercontent.com URL for a given path relative to this ref.
    pub fn raw_url(&self, path: &str) -> String {
        format!(
            "https://raw.githubusercontent.com/{}/{}/{}/{}",
            self.owner, self.repo, self.git_ref, path
        )
    }

    /// Raw URL for this ref's own path.
    pub fn raw_url_self(&self) -> String {
        self.raw_url(&self.path)
    }

    /// Return the parent directory path within the repo, or empty string for root.
    pub fn parent_dir(&self) -> &str {
        self.path.rsplit_once('/').map(|(parent, _)| parent).unwrap_or("")
    }

    /// Resolve a relative path (e.g. `./foo.md`) against this ref's parent directory.
    pub fn resolve_relative(&self, rel: &str) -> String {
        let stripped = rel.strip_prefix("./").unwrap_or(rel);
        let parent = self.parent_dir();
        if parent.is_empty() {
            stripped.to_string()
        } else {
            format!("{parent}/{stripped}")
        }
    }
}

/// Parse a GitHub URL into its components.
///
/// Accepts:
/// - `https://github.com/{owner}/{repo}/blob/{ref}/{path}` (file)
/// - `https://github.com/{owner}/{repo}/tree/{ref}/{path}` (directory)
pub fn parse_github_url(url: &str) -> Result<(GitHubFileRef, bool)> {
    let url = url.trim();
    let stripped = url
        .strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("http://github.com/"))
        .context("URL must start with https://github.com/")?;

    // Split into segments: owner / repo / (blob|tree) / ref / path...
    let segments: Vec<&str> = stripped.splitn(5, '/').collect();
    if segments.len() < 5 {
        bail!("Expected a GitHub file or folder URL like https://github.com/owner/repo/blob/main/path/to/file.yaml");
    }

    let owner = segments[0].to_string();
    let repo = segments[1].to_string();
    let kind = segments[2]; // "blob" or "tree"
    let git_ref = segments[3].to_string();
    let path = segments[4].trim_end_matches('/').to_string();

    let is_dir = match kind {
        "blob" => false,
        "tree" => true,
        other => bail!("Unexpected URL segment '{other}' — expected 'blob' (file) or 'tree' (folder)"),
    };

    if owner.is_empty() || repo.is_empty() || git_ref.is_empty() || path.is_empty() {
        bail!("Could not parse owner, repo, ref, or path from the URL");
    }

    Ok((GitHubFileRef { owner, repo, git_ref, path }, is_dir))
}

/// For a skill folder URL, derive the expected YAML filename by convention:
/// the last path component + `.yaml` (e.g. `skills/my-skill` -> `skills/my-skill/my-skill.yaml`).
pub fn derive_skill_yaml_path(dir_path: &str) -> String {
    let dir_name = dir_path
        .trim_end_matches('/')
        .rsplit_once('/')
        .map(|(_, name)| name)
        .unwrap_or(dir_path);
    format!("{}/{}.yaml", dir_path.trim_end_matches('/'), dir_name)
}

/// Convert a GitHub page URL to a downloadable ZIP URL suitable for
/// Kibana's plugin install endpoint. Returns `None` if the URL is not a
/// recognised GitHub page URL (in which case the caller should pass it
/// through unchanged, assuming it is already a direct download link).
///
/// Behaviour:
/// - `/blob/{ref}/{path}` pointing to a `.zip` file → raw download URL for that file
/// - `/tree/{ref}/{path}` (directory) → repo archive download at that ref
/// - `/{owner}/{repo}` (bare repo) → repo archive download assuming `main`
pub fn github_url_to_download_zip(url: &str) -> Option<String> {
    let url = url.trim();
    let stripped = url
        .strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("http://github.com/"))?;

    let segments: Vec<&str> = stripped.splitn(5, '/').collect();
    if segments.len() < 2 {
        return None;
    }

    let owner = segments[0];
    let repo = segments[1].trim_end_matches(".git");
    if owner.is_empty() || repo.is_empty() {
        return None;
    }

    if segments.len() >= 5 {
        let kind = segments[2];
        let git_ref = segments[3];
        let path = segments[4];

        if kind == "blob" && path.ends_with(".zip") {
            return Some(format!(
                "https://raw.githubusercontent.com/{owner}/{repo}/{git_ref}/{path}"
            ));
        }

        return Some(format!(
            "https://github.com/{owner}/{repo}/archive/{git_ref}.zip"
        ));
    }

    Some(format!(
        "https://github.com/{owner}/{repo}/archive/main.zip"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_blob_url() {
        let (r, is_dir) = parse_github_url(
            "https://github.com/myorg/myrepo/blob/main/tools/esql-user-lookup.yaml",
        )
        .unwrap();
        assert!(!is_dir);
        assert_eq!(r.owner, "myorg");
        assert_eq!(r.repo, "myrepo");
        assert_eq!(r.git_ref, "main");
        assert_eq!(r.path, "tools/esql-user-lookup.yaml");
        assert_eq!(
            r.raw_url_self(),
            "https://raw.githubusercontent.com/myorg/myrepo/main/tools/esql-user-lookup.yaml"
        );
    }

    #[test]
    fn parse_tree_url() {
        let (r, is_dir) = parse_github_url(
            "https://github.com/org/repo/tree/v1.0/skills/my-skill/",
        )
        .unwrap();
        assert!(is_dir);
        assert_eq!(r.git_ref, "v1.0");
        assert_eq!(r.path, "skills/my-skill");
    }

    #[test]
    fn derive_yaml_from_dir() {
        assert_eq!(
            derive_skill_yaml_path("skills/my-skill"),
            "skills/my-skill/my-skill.yaml"
        );
    }

    #[test]
    fn download_zip_from_blob_zip_file() {
        assert_eq!(
            github_url_to_download_zip(
                "https://github.com/mattnowzari/agent-builder-definitions/blob/main/plugins/esql-assistant.zip"
            ),
            Some("https://raw.githubusercontent.com/mattnowzari/agent-builder-definitions/main/plugins/esql-assistant.zip".to_string()),
        );
    }

    #[test]
    fn download_zip_from_tree_url() {
        assert_eq!(
            github_url_to_download_zip("https://github.com/org/repo/tree/main/plugins/my-plugin"),
            Some("https://github.com/org/repo/archive/main.zip".to_string()),
        );
    }

    #[test]
    fn download_zip_from_bare_repo() {
        assert_eq!(
            github_url_to_download_zip("https://github.com/org/repo"),
            Some("https://github.com/org/repo/archive/main.zip".to_string()),
        );
    }

    #[test]
    fn download_zip_non_github_returns_none() {
        assert_eq!(
            github_url_to_download_zip("https://example.com/plugin.zip"),
            None,
        );
    }

    #[test]
    fn download_zip_blob_non_zip_uses_archive() {
        assert_eq!(
            github_url_to_download_zip(
                "https://github.com/org/repo/blob/main/plugins/readme.md"
            ),
            Some("https://github.com/org/repo/archive/main.zip".to_string()),
        );
    }

    #[test]
    fn resolve_relative_paths() {
        let (r, _) = parse_github_url(
            "https://github.com/o/r/blob/main/skills/my-skill/my-skill.yaml",
        )
        .unwrap();
        assert_eq!(r.resolve_relative("./my-skill.md"), "skills/my-skill/my-skill.md");
        assert_eq!(r.resolve_relative("runbook.md"), "skills/my-skill/runbook.md");
    }
}
