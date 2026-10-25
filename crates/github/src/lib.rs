//! Direct, read-only GitHub REST client. No Echo backend or browser cookies.
use browser_core::live_folders::{LiveItem, PullRequestFilter};
use echo_ai::credentials::{CredentialStore, Secret};
use serde_json::Value;
use std::{collections::HashMap, sync::Arc, time::Duration};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Cancelled,
    Credentials,
    Unauthorized,
    Permissions,
    Network,
    InvalidResponse,
    Incomplete,
    RateLimited(u64),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Cancelled => "Cancelled",
            Self::Credentials => "The system credential store is unavailable",
            Self::Unauthorized => "Reconnect GitHub",
            Self::Permissions => "Your GitHub token cannot access this resource",
            Self::Network => "Could not connect to GitHub",
            Self::InvalidResponse => "GitHub returned an invalid response",
            Self::Incomplete => "GitHub returned incomplete results",
            Self::RateLimited(_) => "GitHub rate limit exceeded",
        })
    }
}
impl std::error::Error for Error {}
pub struct Response {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: String,
}
pub trait Transport: Send + Sync {
    fn get(&self, path: &str, token: &Secret, etag: Option<&str>) -> Result<Response, Error>;
}
pub struct RestTransport {
    agent: ureq::Agent,
}
impl Default for RestTransport {
    fn default() -> Self {
        Self {
            agent: ureq::Agent::config_builder()
                .timeout_global(Some(Duration::from_secs(20)))
                .max_redirects(0)
                .http_status_as_error(false)
                .build()
                .into(),
        }
    }
}
impl Transport for RestTransport {
    fn get(&self, path: &str, token: &Secret, etag: Option<&str>) -> Result<Response, Error> {
        // Only internal relative paths can receive a credential. Never follow a
        // response-supplied origin or redirect with the Authorization header.
        if !path.starts_with('/') || path.starts_with("//") {
            return Err(Error::InvalidResponse);
        }
        let mut request = self
            .agent
            .get(format!("https://api.github.com{path}"))
            .header("Authorization", format!("Bearer {}", token.expose()))
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "Echo-Live-Folders")
            .header("X-GitHub-Api-Version", "2022-11-28");
        if let Some(etag) = etag {
            request = request.header("If-None-Match", etag);
        }
        let mut response = request.call().map_err(|_| Error::Network)?;
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .filter_map(|(k, v)| Some((k.as_str().to_owned(), v.to_str().ok()?.to_owned())))
            .collect();
        let body = response
            .body_mut()
            .with_config()
            .limit(8 * 1024 * 1024)
            .read_to_string()
            .map_err(|_| Error::InvalidResponse)?;
        Ok(Response {
            status,
            headers,
            body,
        })
    }
}
#[derive(Clone)]
struct Cached {
    etag: Option<String>,
    body: Value,
    next: bool,
}
pub struct Client {
    transport: Arc<dyn Transport>,
    store: Arc<dyn CredentialStore>,
    cache: HashMap<(String, String), Cached>,
    not_before: u64,
    poll_after: HashMap<(String, String), u64>,
}
impl Client {
    pub fn new(transport: Arc<dyn Transport>, store: Arc<dyn CredentialStore>) -> Self {
        Self {
            transport,
            store,
            cache: HashMap::new(),
            not_before: 0,
            poll_after: HashMap::new(),
        }
    }
    /// Validate first, then persist. A failed connection never replaces a key.
    pub fn connect(&mut self, id: &str, token: &Secret, now: u64) -> Result<String, Error> {
        let (user, _) = self.request(id, "/user", token, now, false)?;
        let login = user["login"]
            .as_str()
            .filter(|s| valid_login(s))
            .ok_or(Error::InvalidResponse)?
            .to_owned();
        self.store.set(id, token).map_err(|_| Error::Credentials)?;
        self.cache.retain(|(key, _), _| key != id);
        self.poll_after.retain(|(key, _), _| key != id);
        Ok(login)
    }
    pub fn disconnect(&mut self, id: &str) -> Result<(), Error> {
        self.store.delete(id).map_err(|_| Error::Credentials)?;
        self.cache.retain(|(key, _), _| key != id);
        self.poll_after.retain(|(key, _), _| key != id);
        Ok(())
    }
    fn request(
        &mut self,
        id: &str,
        path: &str,
        token: &Secret,
        now: u64,
        conditional: bool,
    ) -> Result<(Value, bool), Error> {
        if now < self.not_before {
            return Err(Error::RateLimited(self.not_before));
        }
        let key = (id.to_owned(), path.to_owned());
        if self
            .poll_after
            .get(&key)
            .is_some_and(|deadline| now < *deadline)
        {
            return self
                .cache
                .get(&key)
                .map(|c| (c.body.clone(), c.next))
                .ok_or(Error::RateLimited(self.poll_after[&key]));
        }
        let prior = conditional.then(|| self.cache.get(&key)).flatten();
        let response = self
            .transport
            .get(path, token, prior.and_then(|c| c.etag.as_deref()))?;
        if response.status == 401 {
            self.cache.retain(|(key, _), _| key != id);
            return Err(Error::Unauthorized);
        }
        if response.status == 429
            || (response.status == 403
                && (response
                    .headers
                    .get("x-ratelimit-remaining")
                    .is_some_and(|s| s == "0")
                    || response.headers.contains_key("retry-after")))
        {
            let retry = response
                .headers
                .get("retry-after")
                .and_then(|s| s.parse::<u64>().ok())
                .map(|s| now.saturating_add(s));
            let reset = response
                .headers
                .get("x-ratelimit-reset")
                .and_then(|s| s.parse::<u64>().ok());
            self.not_before = retry
                .or(reset)
                .unwrap_or(now.saturating_add(60))
                .max(now.saturating_add(1));
            return Err(Error::RateLimited(self.not_before));
        }
        if let Some(interval) = response
            .headers
            .get("x-poll-interval")
            .and_then(|s| s.parse::<u64>().ok())
        {
            self.poll_after
                .insert(key.clone(), now.saturating_add(interval));
        }
        if response.status == 304 {
            return prior
                .map(|c| (c.body.clone(), c.next))
                .ok_or(Error::InvalidResponse);
        }
        if response.status != 200 {
            return Err(if response.status == 403 {
                Error::Permissions
            } else {
                Error::Network
            });
        }
        let body: Value =
            serde_json::from_str(&response.body).map_err(|_| Error::InvalidResponse)?;
        let next = response
            .headers
            .get("link")
            .is_some_and(|s| s.split(',').any(|link| link.contains("rel=\"next\"")));
        if conditional {
            self.cache.insert(
                key,
                Cached {
                    etag: response.headers.get("etag").cloned(),
                    body: body.clone(),
                    next,
                },
            );
        }
        Ok((body, next))
    }
    pub fn poll(
        &mut self,
        id: &str,
        login: &str,
        filter: PullRequestFilter,
        now: u64,
    ) -> Result<Vec<LiveItem>, Error> {
        self.poll_with_cancel(id, login, filter, now, &|| false)
    }
    pub fn poll_with_cancel(
        &mut self,
        id: &str,
        login: &str,
        filter: PullRequestFilter,
        now: u64,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Vec<LiveItem>, Error> {
        if cancelled() {
            return Err(Error::Cancelled);
        }
        if !valid_login(login) {
            return Err(Error::InvalidResponse);
        }
        let token = self
            .store
            .get(id)
            .map_err(|_| Error::Credentials)?
            .ok_or(Error::Unauthorized)?;
        let subjects = if filter == PullRequestFilter::TeamReviewRequests {
            let mut teams = Vec::new();
            for page in 1..=10 {
                if cancelled() {
                    return Err(Error::Cancelled);
                }
                let (body, next) = self.request(
                    id,
                    &format!("/user/teams?per_page=100&page={page}"),
                    &token,
                    now,
                    true,
                )?;
                for team in body.as_array().ok_or(Error::InvalidResponse)? {
                    let organization = team["organization"]["login"]
                        .as_str()
                        .filter(|s| valid_login(s))
                        .ok_or(Error::InvalidResponse)?;
                    let slug = team["slug"]
                        .as_str()
                        .filter(|s| {
                            !s.is_empty()
                                && s.len() <= 100
                                && s.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
                        })
                        .ok_or(Error::InvalidResponse)?;
                    teams.push(format!("{organization}/{slug}"));
                }
                if !next {
                    break;
                }
                if page == 10 {
                    return Err(Error::Incomplete);
                }
            }
            teams
        } else {
            vec![login.to_owned()]
        };
        let mut all = Vec::new();
        for subject in subjects {
            all.extend(self.search(
                id,
                &token,
                &format!("is:pr is:open {}:{subject}", filter.qualifier()),
                now,
                cancelled,
            )?);
        }
        Ok(all)
    }
    fn search(
        &mut self,
        id: &str,
        token: &Secret,
        query: &str,
        now: u64,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Vec<LiveItem>, Error> {
        let mut items = Vec::new();
        for page in 1..=10 {
            if cancelled() {
                return Err(Error::Cancelled);
            }
            let params = url::form_urlencoded::Serializer::new(String::new())
                .append_pair("q", query)
                .append_pair("sort", "updated")
                .append_pair("order", "desc")
                .append_pair("per_page", "100")
                .append_pair("page", &page.to_string())
                .finish();
            let (body, next) =
                self.request(id, &format!("/search/issues?{params}"), token, now, true)?;
            if body["incomplete_results"].as_bool() != Some(false)
                || body["total_count"].as_u64().ok_or(Error::InvalidResponse)? > 1000
            {
                return Err(Error::Incomplete);
            }
            let rows = body["items"].as_array().ok_or(Error::InvalidResponse)?;
            for row in rows {
                if !row["pull_request"].is_object() {
                    return Err(Error::InvalidResponse);
                }
                let url = string(row, "html_url")?;
                let parsed = url::Url::parse(&url).map_err(|_| Error::InvalidResponse)?;
                if parsed.scheme() != "https"
                    || parsed.host_str() != Some("github.com")
                    || !parsed.username().is_empty()
                    || parsed.password().is_some()
                {
                    return Err(Error::InvalidResponse);
                }
                let repo_api = string(row, "repository_url")?;
                let repo = repo_api
                    .strip_prefix("https://api.github.com/repos/")
                    .filter(|s| s.split('/').count() == 2 && !s.contains('?') && !s.contains('#'))
                    .ok_or(Error::InvalidResponse)?
                    .to_owned();
                items.push(LiveItem {
                    id: row["id"].as_u64().ok_or(Error::InvalidResponse)?,
                    title: string(row, "title")?,
                    url,
                    repository: repo,
                    updated_at: string(row, "updated_at")?,
                    read_revision: None,
                });
            }
            if !next {
                return Ok(items);
            }
        }
        Err(Error::Incomplete)
    }
}
fn valid_login(login: &str) -> bool {
    !login.is_empty()
        && login.len() <= 39
        && login
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-')
}
fn string(value: &Value, key: &str) -> Result<String, Error> {
    value[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or(Error::InvalidResponse)
}

#[cfg(test)]
mod tests;
