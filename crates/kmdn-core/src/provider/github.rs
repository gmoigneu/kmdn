//! GitHub REST implementation. `base_url` defaults to api.github.com and is overridable for tests.

use reqwest::blocking::{Client, RequestBuilder, Response};
use reqwest::header::{ACCEPT, AUTHORIZATION, USER_AGENT};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use super::*;

pub const API_URL: &str = "https://api.github.com";
pub const DEVICE_CODE_URL: &str = "https://github.com/login/device/code";
pub const DEVICE_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
const UA: &str = "kmdn";

pub struct GitHub {
    client: Client,
    base_url: String,
    token: String,
    /// ETag cache for GET requests: url -> (etag, body). A 304 costs no rate-limit budget (D31).
    etags: std::sync::Mutex<std::collections::HashMap<String, (String, String)>>,
}

impl GitHub {
    pub fn new(token: &str) -> Self {
        Self::with_base_url(API_URL, token)
    }

    pub fn with_base_url(base_url: &str, token: &str) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            token: token.to_string(),
            etags: Default::default(),
        }
    }

    /// GET with a conditional request when this URL was fetched before. Returns the body text.
    fn get_text(&self, url: &str) -> Result<String> {
        let cached = self.etags.lock().ok().and_then(|m| m.get(url).cloned());
        let mut rb = self.req(self.client.get(url));
        if let Some((etag, _)) = &cached {
            rb = rb.header(reqwest::header::IF_NONE_MATCH, etag.clone());
        }
        let resp = rb.send()?;
        if resp.status() == reqwest::StatusCode::NOT_MODIFIED {
            if let Some((_, body)) = cached {
                return Ok(body);
            }
        }
        let resp = Self::check(resp)?;
        let etag = resp
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let body = resp.text()?;
        if let (Some(etag), Ok(mut m)) = (etag, self.etags.lock()) {
            if m.len() > 512 {
                m.clear();
            }
            m.insert(url.to_string(), (etag, body.clone()));
        }
        Ok(body)
    }

    /// One GraphQL request. Errors in the response body surface as `Decode`.
    fn graphql(&self, query: &str, variables: Value) -> Result<Value> {
        let url = format!("{}/graphql", self.base_url);
        let v: Value = self.send_json(
            self.client.post(url),
            &json!({ "query": query, "variables": variables }),
        )?;
        if let Some(errs) = v
            .get("errors")
            .and_then(Value::as_array)
            .filter(|e| !e.is_empty())
        {
            return Err(ProviderError::Decode(format!(
                "graphql: {}",
                errs[0]
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("error")
            )));
        }
        Ok(v.get("data").cloned().unwrap_or(Value::Null))
    }

    fn req(&self, rb: RequestBuilder) -> RequestBuilder {
        rb.header(USER_AGENT, UA)
            .header(ACCEPT, "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header(AUTHORIZATION, format!("Bearer {}", self.token))
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn check(resp: Response) -> Result<Response> {
        let status = resp.status();
        if status.is_success() {
            return Ok(resp);
        }
        let url = resp.url().to_string();
        let body = resp.text().unwrap_or_default();
        Err(ProviderError::Status {
            status: status.as_u16(),
            url,
            body: body.chars().take(500).collect(),
        })
    }

    fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let body = self.get_text(&self.url(path))?;
        serde_json::from_str::<T>(&body).map_err(|e| ProviderError::Decode(e.to_string()))
    }

    fn send_json<T: DeserializeOwned>(&self, rb: RequestBuilder, body: &Value) -> Result<T> {
        let resp = Self::check(self.req(rb).json(body).send()?)?;
        resp.json::<T>()
            .map_err(|e| ProviderError::Decode(e.to_string()))
    }

    fn post<T: DeserializeOwned>(&self, path: &str, body: &Value) -> Result<T> {
        self.send_json(self.client.post(self.url(path)), body)
    }

    fn patch<T: DeserializeOwned>(&self, path: &str, body: &Value) -> Result<T> {
        self.send_json(self.client.patch(self.url(path)), body)
    }

    fn put_no_body(&self, path: &str, body: &Value) -> Result<()> {
        Self::check(
            self.req(self.client.put(self.url(path)))
                .json(body)
                .send()?,
        )?;
        Ok(())
    }

    fn all_pages<T: DeserializeOwned>(&self, path: &str) -> Result<Vec<T>> {
        let mut out = Vec::new();
        for page in 1..=20 {
            let sep = if path.contains('?') { '&' } else { '?' };
            let items: Vec<T> = self.get(&format!("{path}{sep}per_page=100&page={page}"))?;
            let n = items.len();
            out.extend(items);
            if n < 100 {
                break;
            }
        }
        Ok(out)
    }
}

fn s(v: &Value, k: &str) -> String {
    v.get(k)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}
fn os(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::to_string)
}

fn parse_pull(v: &Value) -> PullRequest {
    let merged = v.get("merged_at").map(|m| !m.is_null()).unwrap_or(false);
    let state = match (s(v, "state").as_str(), merged) {
        (_, true) => PullState::Merged,
        ("open", _) => PullState::Open,
        _ => PullState::Closed,
    };
    PullRequest {
        number: v.get("number").and_then(Value::as_u64).unwrap_or(0),
        title: s(v, "title"),
        body: s(v, "body"),
        author: v.get("user").map(|u| s(u, "login")).unwrap_or_default(),
        head_branch: v.get("head").map(|h| s(h, "ref")).unwrap_or_default(),
        base_branch: v.get("base").map(|h| s(h, "ref")).unwrap_or_default(),
        state,
        draft: v.get("draft").and_then(Value::as_bool).unwrap_or(false),
        url: s(v, "html_url"),
        updated_at: s(v, "updated_at"),
        files: vec![],
        reviewers: v
            .get("requested_reviewers")
            .and_then(Value::as_array)
            .map(|a| a.iter().map(|u| s(u, "login")).collect())
            .unwrap_or_default(),
    }
}

fn parse_comment(v: &Value) -> Comment {
    Comment {
        id: v.get("id").and_then(Value::as_u64).unwrap_or(0),
        author: v.get("user").map(|u| s(u, "login")).unwrap_or_default(),
        body: s(v, "body"),
        created_at: s(v, "created_at"),
        url: s(v, "html_url"),
        path: os(v, "path"),
        line: v.get("line").and_then(Value::as_u64).map(|n| n as u32),
        side: match v.get("side").and_then(Value::as_str) {
            Some("LEFT") => Some(Side::Left),
            Some("RIGHT") => Some(Side::Right),
            _ => None,
        },
    }
}

fn parse_repo(v: &Value) -> RepoSummary {
    RepoSummary {
        owner: v.get("owner").map(|o| s(o, "login")).unwrap_or_default(),
        name: s(v, "name"),
        full_name: s(v, "full_name"),
        private: v.get("private").and_then(Value::as_bool).unwrap_or(false),
        default_branch: s(v, "default_branch"),
        https_url: s(v, "clone_url"),
        description: os(v, "description"),
    }
}

fn parse_issue(v: &Value) -> Issue {
    Issue {
        number: v.get("number").and_then(Value::as_u64).unwrap_or(0),
        title: s(v, "title"),
        body: s(v, "body"),
        url: s(v, "html_url"),
        open: s(v, "state") == "open",
    }
}

impl GitHub {
    fn list_open_pulls_graphql(&self, repo: &RepoRef) -> Result<Vec<PullRequest>> {
        const QUERY: &str = r#"query($owner: String!, $name: String!, $after: String) {
  repository(owner: $owner, name: $name) {
    pullRequests(states: OPEN, first: 100, after: $after, orderBy: {field: UPDATED_AT, direction: DESC}) {
      pageInfo { hasNextPage endCursor }
      nodes {
        number title body isDraft url updatedAt headRefName baseRefName
        author { login }
        files(first: 100) { nodes { path } }
        reviewRequests(first: 20) { nodes { requestedReviewer { ... on User { login } } } }
      }
    }
  }
}"#;
        let mut out = Vec::new();
        let mut after: Option<String> = None;
        for _ in 0..20 {
            let data = self.graphql(
                QUERY,
                json!({ "owner": repo.owner, "name": repo.name, "after": after }),
            )?;
            let prs = data
                .pointer("/repository/pullRequests")
                .ok_or_else(|| ProviderError::Decode("graphql: no pullRequests".into()))?;
            for n in prs
                .get("nodes")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
            {
                out.push(PullRequest {
                    number: n.get("number").and_then(Value::as_u64).unwrap_or(0),
                    title: s(&n, "title"),
                    body: s(&n, "body"),
                    author: n.get("author").map(|a| s(a, "login")).unwrap_or_default(),
                    head_branch: s(&n, "headRefName"),
                    base_branch: s(&n, "baseRefName"),
                    state: PullState::Open,
                    draft: n.get("isDraft").and_then(Value::as_bool).unwrap_or(false),
                    url: s(&n, "url"),
                    updated_at: s(&n, "updatedAt"),
                    files: n
                        .pointer("/files/nodes")
                        .and_then(Value::as_array)
                        .map(|a| a.iter().map(|f| s(f, "path")).collect())
                        .unwrap_or_default(),
                    reviewers: n
                        .pointer("/reviewRequests/nodes")
                        .and_then(Value::as_array)
                        .map(|a| {
                            a.iter()
                                .filter_map(|r| {
                                    r.pointer("/requestedReviewer/login")
                                        .and_then(Value::as_str)
                                })
                                .map(str::to_string)
                                .collect()
                        })
                        .unwrap_or_default(),
                });
            }
            let page = prs.get("pageInfo").cloned().unwrap_or(Value::Null);
            if page.get("hasNextPage").and_then(Value::as_bool) == Some(true) {
                after = page
                    .get("endCursor")
                    .and_then(Value::as_str)
                    .map(str::to_string);
            } else {
                break;
            }
        }
        Ok(out)
    }
}

impl Provider for GitHub {
    fn current_user(&self) -> Result<User> {
        let v: Value = self.get("/user")?;
        Ok(User {
            login: s(&v, "login"),
            name: os(&v, "name"),
            email: os(&v, "email"),
            avatar_url: os(&v, "avatar_url"),
        })
    }

    fn list_repos(&self) -> Result<Vec<RepoSummary>> {
        let items: Vec<Value> = self.all_pages(
            "/user/repos?sort=updated&affiliation=owner,collaborator,organization_member",
        )?;
        Ok(items.iter().map(parse_repo).collect())
    }

    fn create_repo(
        &self,
        name: &str,
        description: &str,
        private: bool,
        org: Option<&str>,
    ) -> Result<RepoSummary> {
        let path = match org {
            Some(o) => format!("/orgs/{o}/repos"),
            None => "/user/repos".to_string(),
        };
        let v: Value = self.post(&path, &json!({ "name": name, "description": description, "private": private, "auto_init": false }))?;
        Ok(parse_repo(&v))
    }

    fn default_branch_protected(&self, repo: &RepoRef, branch: &str) -> Result<Option<bool>> {
        let v: Value = self.get(&format!(
            "/repos/{}/{}/branches/{branch}",
            repo.owner, repo.name
        ))?;
        Ok(v.get("protected").and_then(Value::as_bool))
    }

    fn list_open_pulls(&self, repo: &RepoRef) -> Result<Vec<PullRequest>> {
        // One GraphQL request for every open PR with its files (D31) instead of 1 + N REST calls.
        // Falls back to REST when GraphQL is unavailable (older GHES, restricted tokens).
        match self.list_open_pulls_graphql(repo) {
            Ok(prs) => return Ok(prs),
            Err(e) => {
                tracing::debug!("graphql pulls failed, using REST: {e}");
            }
        }
        let items: Vec<Value> = self.all_pages(&format!(
            "/repos/{}/{}/pulls?state=open",
            repo.owner, repo.name
        ))?;
        let mut out = Vec::new();
        for v in &items {
            let mut pr = parse_pull(v);
            // One failing files call must not hide every review (D22): keep the PR and let the
            // markdown filter see an empty list, which keeps it visible.
            pr.files = self.pull_files(repo, pr.number).unwrap_or_default();
            out.push(pr);
        }
        Ok(out)
    }

    fn get_pull(&self, repo: &RepoRef, number: u64) -> Result<PullRequest> {
        let v: Value = self.get(&format!(
            "/repos/{}/{}/pulls/{number}",
            repo.owner, repo.name
        ))?;
        Ok(parse_pull(&v))
    }

    fn pull_files(&self, repo: &RepoRef, number: u64) -> Result<Vec<String>> {
        let items: Vec<Value> = self.all_pages(&format!(
            "/repos/{}/{}/pulls/{number}/files",
            repo.owner, repo.name
        ))?;
        Ok(items.iter().map(|f| s(f, "filename")).collect())
    }

    fn create_pull(&self, repo: &RepoRef, pull: &NewPull) -> Result<PullRequest> {
        let v: Value = self.post(
            &format!("/repos/{}/{}/pulls", repo.owner, repo.name),
            &json!({ "title": pull.title, "body": pull.body, "head": pull.head, "base": pull.base, "draft": pull.draft }),
        )?;
        Ok(parse_pull(&v))
    }

    fn update_pull(
        &self,
        repo: &RepoRef,
        number: u64,
        title: &str,
        body: &str,
    ) -> Result<PullRequest> {
        let v: Value = self.patch(
            &format!("/repos/{}/{}/pulls/{number}", repo.owner, repo.name),
            &json!({ "title": title, "body": body }),
        )?;
        Ok(parse_pull(&v))
    }

    fn mergeability(&self, repo: &RepoRef, number: u64) -> Result<Mergeability> {
        let v: Value = self.get(&format!(
            "/repos/{}/{}/pulls/{number}",
            repo.owner, repo.name
        ))?;
        let reviews: Vec<Value> = self.all_pages(&format!(
            "/repos/{}/{}/pulls/{number}/reviews",
            repo.owner, repo.name
        ))?;
        // Latest review per user decides.
        let mut latest: std::collections::HashMap<String, String> = Default::default();
        for r in &reviews {
            let user = r.get("user").map(|u| s(u, "login")).unwrap_or_default();
            let state = s(r, "state");
            if state == "APPROVED" || state == "CHANGES_REQUESTED" || state == "DISMISSED" {
                latest.insert(user, state);
            }
        }
        let approvals = latest.values().filter(|st| *st == "APPROVED").count() as u32;
        let changes_requested = latest.values().any(|st| st == "CHANGES_REQUESTED");
        let sha = v.get("head").map(|h| s(h, "sha")).unwrap_or_default();
        let checks_passing = if sha.is_empty() {
            None
        } else {
            let st: Value = self.get(&format!(
                "/repos/{}/{}/commits/{sha}/status",
                repo.owner, repo.name
            ))?;
            match (
                s(&st, "state").as_str(),
                st.get("total_count").and_then(Value::as_u64).unwrap_or(0),
            ) {
                (_, 0) => None,
                ("success", _) => Some(true),
                ("pending", _) => None,
                _ => Some(false),
            }
        };
        Ok(Mergeability {
            mergeable: v.get("mergeable").and_then(Value::as_bool),
            state: s(&v, "mergeable_state"),
            approvals,
            changes_requested,
            checks_passing,
        })
    }

    fn merge_pull(&self, repo: &RepoRef, number: u64, method: MergeMethod) -> Result<()> {
        let m = match method {
            MergeMethod::Merge => "merge",
            MergeMethod::Squash => "squash",
            MergeMethod::Rebase => "rebase",
        };
        self.put_no_body(
            &format!("/repos/{}/{}/pulls/{number}/merge", repo.owner, repo.name),
            &json!({ "merge_method": m }),
        )
    }

    fn list_comments(&self, repo: &RepoRef, number: u64) -> Result<Vec<Comment>> {
        let issue: Vec<Value> = self.all_pages(&format!(
            "/repos/{}/{}/issues/{number}/comments",
            repo.owner, repo.name
        ))?;
        let review: Vec<Value> = self.all_pages(&format!(
            "/repos/{}/{}/pulls/{number}/comments",
            repo.owner, repo.name
        ))?;
        let mut out: Vec<Comment> = issue
            .iter()
            .chain(review.iter())
            .map(parse_comment)
            .collect();
        out.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        Ok(out)
    }

    fn create_comment(&self, repo: &RepoRef, number: u64, body: &str) -> Result<Comment> {
        let v: Value = self.post(
            &format!(
                "/repos/{}/{}/issues/{number}/comments",
                repo.owner, repo.name
            ),
            &json!({ "body": body }),
        )?;
        Ok(parse_comment(&v))
    }

    fn update_comment(&self, repo: &RepoRef, comment_id: u64, body: &str) -> Result<Comment> {
        let v: Value = self.patch(
            &format!(
                "/repos/{}/{}/issues/comments/{comment_id}",
                repo.owner, repo.name
            ),
            &json!({ "body": body }),
        )?;
        Ok(parse_comment(&v))
    }

    fn create_review_comment(
        &self,
        repo: &RepoRef,
        number: u64,
        body: &str,
        path: &str,
        line: u32,
        side: Side,
    ) -> Result<Comment> {
        let pr: Value = self.get(&format!(
            "/repos/{}/{}/pulls/{number}",
            repo.owner, repo.name
        ))?;
        let sha = pr.get("head").map(|h| s(h, "sha")).unwrap_or_default();
        let v: Value = self.post(
            &format!("/repos/{}/{}/pulls/{number}/comments", repo.owner, repo.name),
            &json!({ "body": body, "commit_id": sha, "path": path, "line": line, "side": if side == Side::Left { "LEFT" } else { "RIGHT" } }),
        )?;
        Ok(parse_comment(&v))
    }

    fn submit_review(
        &self,
        repo: &RepoRef,
        number: u64,
        event: ReviewEvent,
        body: &str,
    ) -> Result<()> {
        let ev = match event {
            ReviewEvent::Approve => "APPROVE",
            ReviewEvent::RequestChanges => "REQUEST_CHANGES",
            ReviewEvent::Comment => "COMMENT",
        };
        let _: Value = self.post(
            &format!("/repos/{}/{}/pulls/{number}/reviews", repo.owner, repo.name),
            &json!({ "event": ev, "body": body }),
        )?;
        Ok(())
    }

    fn find_issue(&self, repo: &RepoRef, label: &str, title: &str) -> Result<Option<Issue>> {
        let items: Vec<Value> = self.all_pages(&format!(
            "/repos/{}/{}/issues?state=open&labels={}",
            repo.owner,
            repo.name,
            urlencode(label)
        ))?;
        Ok(items
            .iter()
            .filter(|v| v.get("pull_request").is_none())
            .find(|v| s(v, "title") == title)
            .map(parse_issue))
    }

    fn create_issue(
        &self,
        repo: &RepoRef,
        title: &str,
        body: &str,
        labels: &[&str],
    ) -> Result<Issue> {
        let v: Value = self.post(
            &format!("/repos/{}/{}/issues", repo.owner, repo.name),
            &json!({ "title": title, "body": body, "labels": labels }),
        )?;
        Ok(parse_issue(&v))
    }

    fn list_issue_comments(&self, repo: &RepoRef, number: u64) -> Result<Vec<Comment>> {
        let items: Vec<Value> = self.all_pages(&format!(
            "/repos/{}/{}/issues/{number}/comments",
            repo.owner, repo.name
        ))?;
        Ok(items.iter().map(parse_comment).collect())
    }

    fn comment_issue(&self, repo: &RepoRef, number: u64, body: &str) -> Result<Comment> {
        self.create_comment(repo, number, body)
    }

    fn create_issue_labels(&self, repo: &RepoRef, number: u64, labels: &[String]) -> Result<()> {
        let _: Value = self.post(
            &format!("/repos/{}/{}/issues/{number}/labels", repo.owner, repo.name),
            &json!({ "labels": labels }),
        )?;
        Ok(())
    }
}

fn urlencode(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

/// Device flow step 1. `client_id` is the public GitHub App client id (D30).
pub fn start_device_flow(client_id: &str, scope: &str, url: &str) -> Result<DeviceCode> {
    let resp = Client::new()
        .post(url)
        .header(USER_AGENT, UA)
        .header(ACCEPT, "application/json")
        .form(&[("client_id", client_id), ("scope", scope)])
        .send()?;
    let resp = GitHub::check(resp)?;
    resp.json::<DeviceCode>()
        .map_err(|e| ProviderError::Decode(e.to_string()))
}

/// Device flow step 2, call every `interval` seconds until Token, Denied, or Expired.
pub fn poll_device_flow(client_id: &str, device_code: &str, url: &str) -> Result<DevicePoll> {
    let resp = Client::new()
        .post(url)
        .header(USER_AGENT, UA)
        .header(ACCEPT, "application/json")
        .form(&[
            ("client_id", client_id),
            ("device_code", device_code),
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ])
        .send()?;
    let v: Value = GitHub::check(resp)?
        .json()
        .map_err(|e| ProviderError::Decode(e.to_string()))?;
    if let Some(t) = v.get("access_token").and_then(Value::as_str) {
        return Ok(DevicePoll::Token(t.to_string()));
    }
    Ok(match v.get("error").and_then(Value::as_str) {
        Some("authorization_pending") => DevicePoll::Pending,
        Some("slow_down") => DevicePoll::SlowDown,
        Some("access_denied") => DevicePoll::Denied,
        Some("expired_token") => DevicePoll::Expired,
        other => {
            return Err(ProviderError::Decode(format!(
                "unexpected device flow response: {other:?}"
            )))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::{Matcher, Server};

    fn repo() -> RepoRef {
        RepoRef {
            owner: "acme".into(),
            name: "kb".into(),
        }
    }

    #[test]
    fn lists_open_pulls_with_files_and_filters_markdown() {
        let mut server = Server::new();
        let _pulls = server
            .mock("GET", Matcher::Regex(r"^/repos/acme/kb/pulls\?state=open.*".into()))
            .match_header("authorization", "Bearer t0k")
            .with_body(r#"[{"number":7,"title":"Update deploy","body":"","state":"open","draft":false,"html_url":"u7","updated_at":"2026-09-19T00:00:00Z","user":{"login":"alice"},"head":{"ref":"kmdn/alice/deploy","sha":"abc"},"base":{"ref":"main"}},
                          {"number":8,"title":"Bump deps","body":"","state":"open","draft":false,"html_url":"u8","updated_at":"2026-09-19T00:00:00Z","user":{"login":"bot"},"head":{"ref":"deps","sha":"def"},"base":{"ref":"main"}}]"#)
            .create();
        let _f7 = server
            .mock(
                "GET",
                Matcher::Regex(r"^/repos/acme/kb/pulls/7/files.*".into()),
            )
            .with_body(r#"[{"filename":"ops/deploy.md"},{"filename":"ops/assets/deploy/a.png"}]"#)
            .create();
        let _f8 = server
            .mock(
                "GET",
                Matcher::Regex(r"^/repos/acme/kb/pulls/8/files.*".into()),
            )
            .with_body(r#"[{"filename":"package.json"}]"#)
            .create();

        let _no_graphql = server
            .mock("POST", "/graphql")
            .with_status(404)
            .with_body("{}")
            .create();
        let gh = GitHub::with_base_url(&server.url(), "t0k");
        let prs = gh.list_open_pulls(&repo()).unwrap();
        assert_eq!(prs.len(), 2);
        assert_eq!(prs[0].head_branch, "kmdn/alice/deploy");
        let md: Vec<&PullRequest> = prs.iter().filter(|p| touches_markdown(&p.files)).collect();
        assert_eq!(md.len(), 1);
        assert_eq!(md[0].number, 7);
    }

    #[test]
    fn creates_pull_and_reviews_and_merges() {
        let mut server = Server::new();
        let create = server
            .mock("POST", "/repos/acme/kb/pulls")
            .match_body(Matcher::PartialJsonString(r#"{"head":"kmdn/alice/deploy","base":"main","title":"Update deploy"}"#.into()))
            .with_status(201)
            .with_body(r#"{"number":9,"title":"Update deploy","body":"b","state":"open","draft":false,"html_url":"u9","updated_at":"x","user":{"login":"alice"},"head":{"ref":"kmdn/alice/deploy","sha":"abc"},"base":{"ref":"main"}}"#)
            .create();
        let review = server
            .mock("POST", "/repos/acme/kb/pulls/9/reviews")
            .match_body(Matcher::PartialJsonString(r#"{"event":"APPROVE"}"#.into()))
            .with_body("{}")
            .create();
        let merge = server
            .mock("PUT", "/repos/acme/kb/pulls/9/merge")
            .match_body(Matcher::PartialJsonString(
                r#"{"merge_method":"squash"}"#.into(),
            ))
            .with_body(r#"{"merged":true}"#)
            .create();

        let gh = GitHub::with_base_url(&server.url(), "t0k");
        let pr = gh
            .create_pull(
                &repo(),
                &NewPull {
                    title: "Update deploy".into(),
                    body: "b".into(),
                    head: "kmdn/alice/deploy".into(),
                    base: "main".into(),
                    draft: false,
                },
            )
            .unwrap();
        assert_eq!(pr.number, 9);
        gh.submit_review(&repo(), 9, ReviewEvent::Approve, "LGTM")
            .unwrap();
        gh.merge_pull(&repo(), 9, MergeMethod::Squash).unwrap();
        create.assert();
        review.assert();
        merge.assert();
    }

    #[test]
    fn mergeability_counts_latest_review_per_user() {
        let mut server = Server::new();
        let _pr = server.mock("GET", "/repos/acme/kb/pulls/9").with_body(r#"{"number":9,"state":"open","mergeable":true,"mergeable_state":"clean","head":{"ref":"h","sha":"abc"},"base":{"ref":"main"},"user":{"login":"a"}}"#).create();
        let _rv = server.mock("GET", Matcher::Regex(r"^/repos/acme/kb/pulls/9/reviews.*".into()))
            .with_body(r#"[{"user":{"login":"bob"},"state":"CHANGES_REQUESTED"},{"user":{"login":"bob"},"state":"APPROVED"},{"user":{"login":"carol"},"state":"COMMENTED"}]"#).create();
        let _st = server
            .mock("GET", "/repos/acme/kb/commits/abc/status")
            .with_body(r#"{"state":"success","total_count":2}"#)
            .create();
        let gh = GitHub::with_base_url(&server.url(), "t0k");
        let m = gh.mergeability(&repo(), 9).unwrap();
        assert_eq!(
            m,
            Mergeability {
                mergeable: Some(true),
                state: "clean".into(),
                approvals: 1,
                changes_requested: false,
                checks_passing: Some(true)
            }
        );
    }

    #[test]
    fn surfaces_http_errors_with_body() {
        let mut server = Server::new();
        let _m = server
            .mock("GET", "/user")
            .with_status(401)
            .with_body(r#"{"message":"Bad credentials"}"#)
            .create();
        let gh = GitHub::with_base_url(&server.url(), "bad");
        let err = gh.current_user().unwrap_err();
        assert!(
            matches!(err, ProviderError::Status { status: 401, .. }),
            "{err}"
        );
        assert!(err.to_string().contains("Bad credentials"));
    }

    #[test]
    fn device_flow_start_and_poll() {
        let mut server = Server::new();
        let _start = server.mock("POST", "/login/device/code").with_body(r#"{"device_code":"dc","user_code":"ABCD-1234","verification_uri":"https://github.com/login/device","expires_in":900,"interval":5}"#).create();
        let _pending = server
            .mock("POST", "/login/oauth/access_token")
            .with_body(r#"{"error":"authorization_pending"}"#)
            .expect(1)
            .create();
        let code = start_device_flow(
            "cid",
            "repo",
            &format!("{}/login/device/code", server.url()),
        )
        .unwrap();
        assert_eq!(code.user_code, "ABCD-1234");
        assert_eq!(
            poll_device_flow(
                "cid",
                &code.device_code,
                &format!("{}/login/oauth/access_token", server.url())
            )
            .unwrap(),
            DevicePoll::Pending
        );
        drop(_pending);
        let _ok = server
            .mock("POST", "/login/oauth/access_token")
            .with_body(r#"{"access_token":"gho_x","token_type":"bearer"}"#)
            .create();
        assert_eq!(
            poll_device_flow(
                "cid",
                &code.device_code,
                &format!("{}/login/oauth/access_token", server.url())
            )
            .unwrap(),
            DevicePoll::Token("gho_x".into())
        );
    }

    #[test]
    fn find_issue_by_label_and_title_skips_pull_requests() {
        let mut server = Server::new();
        let _m = server.mock("GET", Matcher::Regex(r"^/repos/acme/kb/issues\?state=open&labels=kmdn.*".into()))
            .with_body(r#"[{"number":1,"title":"ops/deploy.md","body":"","html_url":"u","state":"open","pull_request":{}},{"number":2,"title":"ops/deploy.md","body":"","html_url":"u2","state":"open"}]"#).create();
        let gh = GitHub::with_base_url(&server.url(), "t0k");
        let issue = gh
            .find_issue(&repo(), "kmdn", "ops/deploy.md")
            .unwrap()
            .unwrap();
        assert_eq!(issue.number, 2);
        assert!(gh
            .find_issue(&repo(), "kmdn", "other.md")
            .unwrap()
            .is_none());
    }

    #[test]
    fn lists_open_pulls_with_one_graphql_request() {
        let mut server = Server::new();
        let gql = server
            .mock("POST", "/graphql")
            .match_body(Matcher::Regex("pullRequests".into()))
            .with_body(r#"{"data":{"repository":{"pullRequests":{"pageInfo":{"hasNextPage":false,"endCursor":null},"nodes":[
                {"number":7,"title":"Update deploy","body":"","isDraft":false,"url":"u7","updatedAt":"2026-09-19T00:00:00Z","headRefName":"kmdn/alice/deploy","baseRefName":"main","author":{"login":"alice"},"files":{"nodes":[{"path":"ops/deploy.md"}]}},
                {"number":8,"title":"Bump deps","body":"","isDraft":false,"url":"u8","updatedAt":"2026-09-19T00:00:00Z","headRefName":"deps","baseRefName":"main","author":{"login":"bot"},"files":{"nodes":[{"path":"package.json"}]}}
            ]}}}}"#)
            .expect(1)
            .create();
        let rest = server
            .mock("GET", Matcher::Regex(r"^/repos/.*".into()))
            .expect(0)
            .create();
        let gh = GitHub::with_base_url(&server.url(), "t0k");
        let prs = gh.list_open_pulls(&repo()).unwrap();
        assert_eq!(prs.iter().map(|p| p.number).collect::<Vec<_>>(), vec![7, 8]);
        assert_eq!(prs[0].files, vec!["ops/deploy.md"]);
        gql.assert();
        rest.assert();
    }

    #[test]
    fn conditional_requests_reuse_the_cached_body_on_304() {
        let mut server = Server::new();
        let first = server
            .mock("GET", "/user")
            .match_header("if-none-match", Matcher::Missing)
            .with_header("etag", "\"abc\"")
            .with_body(r#"{"login":"alice"}"#)
            .expect(1)
            .create();
        let second = server
            .mock("GET", "/user")
            .match_header("if-none-match", "\"abc\"")
            .with_status(304)
            .expect(1)
            .create();
        let gh = GitHub::with_base_url(&server.url(), "t0k");
        assert_eq!(gh.current_user().unwrap().login, "alice");
        assert_eq!(gh.current_user().unwrap().login, "alice");
        first.assert();
        second.assert();
    }
}
