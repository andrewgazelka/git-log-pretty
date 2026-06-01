//! Resolve a git commit author to a GitHub user and fetch their avatar.
//!
//! Resolution is layered so the cheap, offline paths run first:
//!
//! 1. [`parse_noreply`] reads the login straight out of a
//!    `…@users.noreply.github.com` commit email, with no network.
//! 2. [`Client::resolve_commit`] asks GitHub who authored a specific commit in
//!    a repo (works whenever the email is linked to an account).
//! 3. [`Client::search_email`] looks the email up in the user search index
//!    (only finds it if the user made the email public).
//!
//! Once a login is known, [`Client::avatar_png`] downloads the avatar as PNG
//! from `https://github.com/<login>.png`, which always returns PNG regardless of
//! the format the user originally uploaded.

use std::io::Read;
use std::time::Duration;

use serde::Deserialize;

const API_VERSION: &str = "2022-11-28";
const USER_AGENT: &str = concat!("git-log-pretty/", env!("CARGO_PKG_VERSION"));

/// A resolved GitHub account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    pub login: String,
}

/// Failure talking to GitHub.
#[derive(Debug)]
pub enum Error {
    /// A transport or non-success HTTP status from a request.
    Http(Box<ureq::Error>),
    /// Reading or decoding a response body failed.
    Io(std::io::Error),
    /// A login failed validation, so no request was made.
    InvalidLogin(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Http(e) => write!(f, "github request failed: {e}"),
            Error::Io(e) => write!(f, "github response error: {e}"),
            Error::InvalidLogin(login) => write!(f, "not a valid github login: {login:?}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<ureq::Error> for Error {
    fn from(e: ureq::Error) -> Self {
        Error::Http(Box::new(e))
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

/// Parse a GitHub `noreply` commit email into a [`User`].
///
/// Handles both forms GitHub issues:
/// `49699333+octocat@users.noreply.github.com` and the older
/// `octocat@users.noreply.github.com`. Returns `None` for any other email.
pub fn parse_noreply(email: &str) -> Option<User> {
    let local = email
        .trim()
        .to_ascii_lowercase()
        .strip_suffix("@users.noreply.github.com")?
        .to_string();
    // Newer emails are "<id>+<login>"; the id half is not a login on its own.
    let login = match local.split_once('+') {
        Some((_, login)) => login,
        None => local.as_str(),
    };
    // The local part is attacker-controlled, so only accept a real login shape;
    // this is what keeps `/`, `?`, `#` out of the avatar URL later.
    if !is_valid_login(login) {
        return None;
    }
    Some(User {
        login: login.to_string(),
    })
}

/// Whether `login` is a syntactically valid GitHub username: 1–39 characters of
/// ASCII alphanumerics and hyphens, not starting or ending with a hyphen.
///
/// A valid login needs no URL or path encoding, so validating here lets callers
/// safely interpolate it. Used as a guard before any network or filesystem use.
pub fn is_valid_login(login: &str) -> bool {
    let bytes = login.as_bytes();
    if login.is_empty() || login.len() > 39 {
        return false;
    }
    if bytes[0] == b'-' || bytes[bytes.len() - 1] == b'-' {
        return false;
    }
    login.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// Parse `owner` and `repo` from a GitHub remote URL (https or ssh forms).
///
/// Returns `None` for non-GitHub remotes, so callers can skip the commit-author
/// lookup entirely off-GitHub.
pub fn parse_remote(url: &str) -> Option<(String, String)> {
    let url = url.trim();
    let url = url.strip_suffix(".git").unwrap_or(url);
    let rest = url
        .strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("http://github.com/"))
        .or_else(|| url.strip_prefix("ssh://git@github.com/"))
        .or_else(|| url.strip_prefix("git@github.com:"))?;
    let (owner, repo) = rest.split_once('/')?;
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some((owner.to_string(), repo.to_string()))
}

/// A GitHub HTTP client. Holds one connection pool and an optional token used
/// for the authenticated API lookups (raising rate limits and reaching private
/// repos). The avatar download needs no token.
pub struct Client {
    agent: ureq::Agent,
    token: Option<String>,
}

#[derive(Deserialize)]
struct CommitResponse {
    author: Option<Account>,
}

#[derive(Deserialize)]
struct SearchResponse {
    items: Vec<Account>,
}

#[derive(Deserialize)]
struct Account {
    login: String,
}

impl Client {
    /// Build a client. Pass a token (e.g. from `GITHUB_TOKEN` or `gh auth
    /// token`) to enable the API lookups; pass `None` to use only the public
    /// avatar endpoint.
    pub fn new(token: Option<String>) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(10))
            .build();
        Self { agent, token }
    }

    fn api_get(&self, url: &str) -> Result<ureq::Response, Box<ureq::Error>> {
        let mut req = self
            .agent
            .get(url)
            .set("User-Agent", USER_AGENT)
            .set("Accept", "application/vnd.github+json")
            .set("X-GitHub-Api-Version", API_VERSION);
        if let Some(token) = &self.token {
            req = req.set("Authorization", &format!("Bearer {token}"));
        }
        req.call().map_err(Box::new)
    }

    /// Resolve the GitHub login that authored `sha` in `owner/repo`.
    ///
    /// Returns `Ok(None)` when GitHub has no account linked to the commit's
    /// author email, or when the commit is not found.
    pub fn resolve_commit(
        &self,
        owner: &str,
        repo: &str,
        sha: &str,
    ) -> Result<Option<User>, Error> {
        let url = format!("https://api.github.com/repos/{owner}/{repo}/commits/{sha}");
        let resp = match self.api_get(&url) {
            Ok(resp) => resp,
            // A missing commit or unprocessable ref is "no answer", not an error.
            Err(e) if matches!(*e, ureq::Error::Status(404 | 422, _)) => return Ok(None),
            Err(e) => return Err(Error::Http(e)),
        };
        let parsed: CommitResponse = resp.into_json()?;
        Ok(parsed.author.map(|a| User { login: a.login }))
    }

    /// Resolve a login from a commit email via GitHub's user search.
    ///
    /// Only succeeds if the user has made that email public. Returns `Ok(None)`
    /// otherwise.
    pub fn search_email(&self, email: &str) -> Result<Option<User>, Error> {
        let query = percent_encode(&format!("{email} in:email"));
        let url = format!("https://api.github.com/search/users?q={query}&per_page=1");
        let resp = match self.api_get(&url) {
            Ok(resp) => resp,
            Err(e) if matches!(*e, ureq::Error::Status(422, _)) => return Ok(None),
            Err(e) => return Err(Error::Http(e)),
        };
        let parsed: SearchResponse = resp.into_json()?;
        Ok(parsed.items.into_iter().next().map(|a| User { login: a.login }))
    }

    /// Download `login`'s avatar as PNG bytes, sized to roughly `size_px` square.
    ///
    /// The `.png` endpoint transcodes to PNG server-side, so the result is
    /// always PNG even if the user uploaded a JPEG.
    pub fn avatar_png(&self, login: &str, size_px: u32) -> Result<Vec<u8>, Error> {
        // Defense in depth: every resolution path funnels through here, so this
        // single guard keeps an untrusted login out of the request target.
        if !is_valid_login(login) {
            return Err(Error::InvalidLogin(login.to_string()));
        }
        let url = format!("https://github.com/{login}.png?size={size_px}");
        let resp = self
            .agent
            .get(&url)
            .set("User-Agent", USER_AGENT)
            .call()?;
        let mut buf = Vec::new();
        resp.into_reader().read_to_end(&mut buf)?;
        Ok(buf)
    }
}

/// Percent-encode everything outside the unreserved set, so an email and search
/// qualifiers survive as a single query value.
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noreply_with_id() {
        assert_eq!(
            parse_noreply("49699333+octocat@users.noreply.github.com"),
            Some(User { login: "octocat".into() })
        );
    }

    #[test]
    fn noreply_without_id() {
        assert_eq!(
            parse_noreply("octocat@users.noreply.github.com"),
            Some(User { login: "octocat".into() })
        );
    }

    #[test]
    fn noreply_is_case_insensitive_on_domain() {
        assert_eq!(
            parse_noreply("Octocat@Users.Noreply.GitHub.com"),
            Some(User { login: "octocat".into() })
        );
    }

    #[test]
    fn non_noreply_email_is_none() {
        assert_eq!(parse_noreply("drew@x.ai"), None);
        assert_eq!(parse_noreply("nope"), None);
    }

    #[test]
    fn noreply_with_url_unsafe_login_is_rejected() {
        // A crafted local part must not produce a login that injects into a URL.
        assert_eq!(parse_noreply("a/b@users.noreply.github.com"), None);
        assert_eq!(parse_noreply("a?b@users.noreply.github.com"), None);
        assert_eq!(parse_noreply("a#b@users.noreply.github.com"), None);
        assert_eq!(parse_noreply("dependabot[bot]@users.noreply.github.com"), None);
    }

    #[test]
    fn valid_login_charset_and_edges() {
        assert!(is_valid_login("octocat"));
        assert!(is_valid_login("andrew-gazelka"));
        assert!(is_valid_login("a"));
        assert!(!is_valid_login(""));
        assert!(!is_valid_login("-lead"));
        assert!(!is_valid_login("trail-"));
        assert!(!is_valid_login("has space"));
        assert!(!is_valid_login("has/slash"));
        assert!(!is_valid_login(&"x".repeat(40)));
    }

    #[test]
    fn remote_https_and_ssh() {
        let want = Some(("andrewgazelka".to_string(), "git-log-pretty".to_string()));
        assert_eq!(parse_remote("https://github.com/andrewgazelka/git-log-pretty.git"), want);
        assert_eq!(parse_remote("https://github.com/andrewgazelka/git-log-pretty"), want);
        assert_eq!(parse_remote("git@github.com:andrewgazelka/git-log-pretty.git"), want);
        assert_eq!(parse_remote("ssh://git@github.com/andrewgazelka/git-log-pretty.git"), want);
    }

    #[test]
    fn remote_non_github_is_none() {
        assert_eq!(parse_remote("https://gitlab.com/foo/bar.git"), None);
    }

    #[test]
    fn percent_encode_query() {
        assert_eq!(percent_encode("drew@x.ai in:email"), "drew%40x.ai%20in%3Aemail");
    }
}
