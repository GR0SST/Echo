use super::*;
use std::{collections::VecDeque, sync::Mutex};
#[derive(Default)]
struct Store(Mutex<Option<String>>);
impl CredentialStore for Store {
    fn get(&self, _: &str) -> Result<Option<Secret>, echo_ai::credentials::CredentialError> {
        Ok(self.0.lock().unwrap().clone().map(Secret::new))
    }
    fn set(&self, _: &str, s: &Secret) -> Result<(), echo_ai::credentials::CredentialError> {
        *self.0.lock().unwrap() = Some(s.expose().into());
        Ok(())
    }
    fn delete(&self, _: &str) -> Result<(), echo_ai::credentials::CredentialError> {
        *self.0.lock().unwrap() = None;
        Ok(())
    }
}
#[derive(Default)]
struct Mock {
    replies: Mutex<VecDeque<Response>>,
    requests: Mutex<Vec<(String, Option<String>)>>,
}
impl Transport for Mock {
    fn get(&self, path: &str, token: &Secret, etag: Option<&str>) -> Result<Response, Error> {
        assert_eq!(token.expose(), "fixture-not-a-real-token");
        self.requests
            .lock()
            .unwrap()
            .push((path.into(), etag.map(str::to_owned)));
        Ok(self
            .replies
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected request"))
    }
}
fn reply(status: u16, body: Value, headers: &[(&str, &str)]) -> Response {
    Response {
        status,
        body: body.to_string(),
        headers: headers
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    }
}
fn result(id: u64) -> Value {
    serde_json::json!({"incomplete_results":false,"total_count":1,"items":[{"id":id,"title":"Review parser","html_url":"https://github.com/echo/test/pull/7","repository_url":"https://api.github.com/repos/echo/test","updated_at":"2026-10-25T00:00:00Z","pull_request":{}}]})
}
fn client(replies: Vec<Response>) -> (Client, Arc<Mock>, Arc<Store>) {
    let t = Arc::new(Mock {
        replies: Mutex::new(replies.into()),
        ..Default::default()
    });
    let s = Arc::new(Store(Mutex::new(Some("fixture-not-a-real-token".into()))));
    (Client::new(t.clone(), s.clone()), t, s)
}
#[test]
fn conditional_poll_uses_etag_and_retains_unchanged_results() {
    let (mut c, t, _) = client(vec![
        reply(200, result(7), &[("etag", "\"v1\"")]),
        reply(304, Value::Null, &[]),
    ]);
    let first = c
        .poll("id", "tester", PullRequestFilter::CreatedByMe, 10)
        .unwrap();
    let second = c
        .poll("id", "tester", PullRequestFilter::CreatedByMe, 70)
        .unwrap();
    assert_eq!(first, second);
    let r = t.requests.lock().unwrap();
    assert!(r[0].0.contains("author%3Atester"));
    assert_eq!(r[1].1.as_deref(), Some("\"v1\""));
}
#[test]
fn pagination_and_filter_requests_are_separate() {
    let (mut c, t, _) = client(vec![
        reply(
            200,
            result(7),
            &[(
                "link",
                "<https://api.github.com/search/issues?page=2>; rel=\"next\"",
            )],
        ),
        reply(200, result(8), &[]),
        reply(200, result(9), &[]),
    ]);
    assert_eq!(
        c.poll("id", "tester", PullRequestFilter::ReviewRequests, 1)
            .unwrap()
            .len(),
        2
    );
    c.poll("id", "tester", PullRequestFilter::Assigned, 2)
        .unwrap();
    let r = t.requests.lock().unwrap();
    assert!(r[1].0.ends_with("page=2"));
    assert!(r[2].0.contains("assignee%3Atester"));
    assert!(r[2].1.is_none());
}
#[test]
fn rate_limit_stops_requests_until_retry_after() {
    let (mut c, t, _) = client(vec![
        reply(429, Value::Null, &[("retry-after", "120")]),
        reply(200, result(1), &[]),
    ]);
    assert_eq!(
        c.poll("id", "tester", PullRequestFilter::Assigned, 1),
        Err(Error::RateLimited(121))
    );
    assert_eq!(
        c.poll("id", "tester", PullRequestFilter::Assigned, 120),
        Err(Error::RateLimited(121))
    );
    assert_eq!(t.requests.lock().unwrap().len(), 1);
    assert!(c
        .poll("id", "tester", PullRequestFilter::Assigned, 121)
        .is_ok());
}
#[test]
fn failed_authentication_never_stores_token_and_unauthorized_clears_cache() {
    let (mut c, _, s) = client(vec![reply(401, Value::Null, &[])]);
    *s.0.lock().unwrap() = None;
    assert_eq!(
        c.connect("id", &Secret::new("fixture-not-a-real-token".into()), 1),
        Err(Error::Unauthorized)
    );
    assert!(s.get("id").unwrap().is_none());
}
#[test]
fn connect_validates_identity_and_disconnect_removes_key() {
    let (mut c, _, s) = client(vec![reply(200, serde_json::json!({"login":"tester"}), &[])]);
    assert_eq!(
        c.connect("id", &Secret::new("fixture-not-a-real-token".into()), 1)
            .unwrap(),
        "tester"
    );
    c.disconnect("id").unwrap();
    assert!(s.get("id").unwrap().is_none());
}
#[test]
fn incomplete_or_hostile_results_cannot_replace_folder_snapshot() {
    let mut incomplete = result(1);
    incomplete["incomplete_results"] = true.into();
    let mut hostile = result(1);
    hostile["items"][0]["html_url"] = "https://evil.test/pull/1".into();
    let (mut c, _, _) = client(vec![reply(200, incomplete, &[]), reply(200, hostile, &[])]);
    assert_eq!(
        c.poll("id", "tester", PullRequestFilter::Assigned, 1),
        Err(Error::Incomplete)
    );
    assert_eq!(
        c.poll("id", "tester", PullRequestFilter::Assigned, 2),
        Err(Error::InvalidResponse)
    );
}
#[test]
fn team_review_requests_use_membership_and_exact_team_qualifier() {
    let (mut c, t, _) = client(vec![
        reply(
            200,
            serde_json::json!([{"organization":{"login":"echo"},"slug":"browser"}]),
            &[],
        ),
        reply(200, result(1), &[]),
    ]);
    assert_eq!(
        c.poll("id", "tester", PullRequestFilter::TeamReviewRequests, 1)
            .unwrap()
            .len(),
        1
    );
    assert!(t.requests.lock().unwrap()[1]
        .0
        .contains("team-review-requested%3Aecho%2Fbrowser"));
}
#[test]
fn poll_interval_is_endpoint_scoped_and_serves_cached_value() {
    let (mut c, t, _) = client(vec![
        reply(200, result(1), &[("x-poll-interval", "120")]),
        reply(200, result(2), &[]),
    ]);
    c.poll("id", "tester", PullRequestFilter::Assigned, 1)
        .unwrap();
    c.poll("id", "tester", PullRequestFilter::Assigned, 2)
        .unwrap();
    c.poll("id", "tester", PullRequestFilter::ReviewRequests, 2)
        .unwrap();
    assert_eq!(t.requests.lock().unwrap().len(), 2);
}
#[test]
fn cancelled_poll_never_starts_a_request() {
    let (mut c, t, _) = client(vec![]);
    assert_eq!(
        c.poll_with_cancel("id", "tester", PullRequestFilter::Assigned, 1, &|| true),
        Err(Error::Cancelled)
    );
    assert!(t.requests.lock().unwrap().is_empty());
}
