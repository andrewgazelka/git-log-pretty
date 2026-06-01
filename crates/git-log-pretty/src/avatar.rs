//! Turn a commit author into avatar PNG bytes, resolving GitHub logins and
//! caching aggressively so a single `git log` view makes at most one network
//! request per unique author.

use std::collections::HashMap;
use std::path::PathBuf;

use git2::Repository;
use github::Client;

/// An author's avatar, ready to hand to the renderer.
pub struct Avatar {
    /// PNG-encoded image bytes.
    pub png: Vec<u8>,
    /// A stable per-author id so repeated commits reuse one transmitted image.
    pub id: u32,
}

pub struct AvatarResolver {
    client: Client,
    /// `owner/repo` parsed from the `origin` remote, if it is a GitHub remote.
    origin: Option<(String, String)>,
    /// Explicit `email -> login` overrides from `git config githubLogin.map`.
    identities: HashMap<String, String>,
    size_px: u32,
    /// Resolved `email -> login` (None = tried and failed), bounding API calls.
    login_cache: HashMap<String, Option<String>>,
    /// Downloaded `login -> png` (None = tried and failed).
    png_cache: HashMap<String, Option<Vec<u8>>>,
    /// Stable display ids assigned per login.
    login_ids: HashMap<String, u32>,
    next_id: u32,
    cache_dir: Option<PathBuf>,
}

impl AvatarResolver {
    pub fn new(repo: &Repository, size_px: u32) -> Self {
        let origin = repo
            .find_remote("origin")
            .ok()
            .and_then(|r| r.url().and_then(github::parse_remote));

        Self {
            client: Client::new(discover_token()),
            origin,
            identities: load_identities(repo),
            size_px,
            login_cache: HashMap::new(),
            png_cache: HashMap::new(),
            login_ids: HashMap::new(),
            next_id: 1,
            cache_dir: avatar_cache_dir(),
        }
    }

    /// Resolve and fetch the avatar for the author of `sha` (whose commit email
    /// is `email`). Returns `None` when the author cannot be mapped to a GitHub
    /// account or the avatar cannot be fetched.
    pub fn avatar_for(&mut self, email: &str, sha: &str) -> Option<Avatar> {
        let login = self.login_for(email, sha)?;
        let png = self.png_for(&login)?;
        let id = self.id_for(&login);
        Some(Avatar { png, id })
    }

    fn login_for(&mut self, email: &str, sha: &str) -> Option<String> {
        let email = email.trim();
        if email.is_empty() {
            return None;
        }
        if let Some(login) = self.identities.get(email) {
            return Some(login.clone());
        }
        if let Some(cached) = self.login_cache.get(email) {
            return cached.clone();
        }

        let resolved = github::parse_noreply(email)
            .map(|u| u.login)
            .or_else(|| self.resolve_via_commit(sha))
            .or_else(|| self.client.search_email(email).ok().flatten().map(|u| u.login));

        self.login_cache.insert(email.to_string(), resolved.clone());
        resolved
    }

    fn resolve_via_commit(&self, sha: &str) -> Option<String> {
        let (owner, repo) = self.origin.as_ref()?;
        self.client
            .resolve_commit(owner, repo, sha)
            .ok()
            .flatten()
            .map(|u| u.login)
    }

    fn png_for(&mut self, login: &str) -> Option<Vec<u8>> {
        if let Some(cached) = self.png_cache.get(login) {
            return cached.clone();
        }
        let bytes = self
            .read_disk(login)
            .or_else(|| match self.client.avatar_png(login, self.size_px) {
                Ok(bytes) => {
                    self.write_disk(login, &bytes);
                    Some(bytes)
                }
                Err(_) => None,
            });
        self.png_cache.insert(login.to_string(), bytes.clone());
        bytes
    }

    fn id_for(&mut self, login: &str) -> u32 {
        if let Some(id) = self.login_ids.get(login) {
            return *id;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.login_ids.insert(login.to_string(), id);
        id
    }

    fn disk_path(&self, login: &str) -> Option<PathBuf> {
        let sanitized: String = login
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect();
        Some(self.cache_dir.as_ref()?.join(format!("{sanitized}-{}.png", self.size_px)))
    }

    fn read_disk(&self, login: &str) -> Option<Vec<u8>> {
        std::fs::read(self.disk_path(login)?).ok()
    }

    fn write_disk(&self, login: &str, bytes: &[u8]) {
        let Some(path) = self.disk_path(login) else {
            return;
        };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, bytes);
    }
}

/// Find a GitHub token for the authenticated API lookups: env first, then the
/// `gh` CLI. Returns `None` if neither is available (the avatar download itself
/// needs no token).
fn discover_token() -> Option<String> {
    for key in ["GITHUB_TOKEN", "GH_TOKEN"] {
        if let Ok(v) = std::env::var(key) {
            if !v.trim().is_empty() {
                return Some(v.trim().to_string());
            }
        }
    }
    let out = std::process::Command::new("gh")
        .args(["auth", "token"])
        .output()
        .ok()?;
    if out.status.success() {
        let token = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !token.is_empty() {
            return Some(token);
        }
    }
    None
}

/// Read `email -> login` overrides from `git config githubLogin.map`, a multivar
/// where each value is `email=login`. Lets a user pin their own avatar when
/// their commit email is not linked to a GitHub account.
fn load_identities(repo: &Repository) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Ok(config) = repo.config() else {
        return map;
    };
    if let Ok(entries) = config.entries(Some("githublogin.map")) {
        let _ = entries.for_each(|entry| {
            if let Some(value) = entry.value() {
                if let Some((email, login)) = value.split_once('=') {
                    let (email, login) = (email.trim(), login.trim());
                    if !email.is_empty() && !login.is_empty() {
                        map.insert(email.to_string(), login.to_string());
                    }
                }
            }
        });
    }
    map
}

fn avatar_cache_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
    Some(base.join("git-log-pretty").join("avatars"))
}
