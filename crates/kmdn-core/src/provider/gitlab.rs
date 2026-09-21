//! GitLab REST v4 implementation for gitlab.com and self-hosted instances (D4, D13, D29).
//! MR maps to PR, notes to comments, discussions with a position to inline comments.
//! Approval uses the native API when the instance allows it, otherwise a marker note.

use reqwest::blocking::{Client, RequestBuilder, Response};
use reqwest::header::{ACCEPT, USER_AGENT};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use super::*;

const UA: &str = "kmdn";

pub struct GitLab {
    client: Client,
    /// e.g. https://gitlab.com/api/v4
    api: String,
    token: String,
}

impl GitLab {
    /// `host` is a bare hostname such as `gitlab.com` or `gitlab.example.org`.
    pub fn new(host: &str, token: &str) -> Self {
        Self::with_api_url(&format!("https://{host}/api/v4"), token)
    }

    pub fn with_api_url(api: &str, token: &str) -> Self {
        Self {
            client: Client::new(),
            api: api.trim_end_matches('/').to_string(),
            token: token.to_string(),
        }
    }

    fn req(&self, rb: RequestBuilder) -> RequestBuilder {
        rb.header(USER_AGENT, UA)
            .header(ACCEPT, "application/json")
            .header("PRIVATE-TOKEN", &self.token)
    }

    fn project(repo: &RepoRef) -> String {
        urlencode(&format!("{}/{}", repo.owner, repo.name))
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.api, path)
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
        let resp = Self::check(self.req(self.client.get(self.url(path))).send()?)?;
        resp.json::<T>()
            .map_err(|e| ProviderError::Decode(e.to_string()))
    }

    fn send<T: DeserializeOwned>(&self, rb: RequestBuilder, body: &Value) -> Result<T> {
        let resp = Self::check(self.req(rb).json(body).send()?)?;
        resp.json::<T>()
            .map_err(|e| ProviderError::Decode(e.to_string()))
    }

    fn post<T: DeserializeOwned>(&self, path: &str, body: &Value) -> Result<T> {
        self.send(self.client.post(self.url(path)), body)
    }

    fn put<T: DeserializeOwned>(&self, path: &str, body: &Value) -> Result<T> {
        self.send(self.client.put(self.url(path)), body)
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

    fn mr_path(repo: &RepoRef, iid: u64) -> String {
        format!("/projects/{}/merge_requests/{iid}", Self::project(repo))
    }
}

fn urlencode(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes())
        .collect::<String>()
        .replace('+', "%20")
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

fn parse_mr(v: &Value) -> PullRequest {
    let state = match s(v, "state").as_str() {
        "opened" | "locked" => PullState::Open,
        "merged" => PullState::Merged,
        _ => PullState::Closed,
    };
    PullRequest {
        number: v.get("iid").and_then(Value::as_u64).unwrap_or(0),
        title: s(v, "title"),
        body: s(v, "description"),
        author: v
            .get("author")
            .map(|u| s(u, "username"))
            .unwrap_or_default(),
        head_branch: s(v, "source_branch"),
        base_branch: s(v, "target_branch"),
        state,
        draft: v.get("draft").and_then(Value::as_bool).unwrap_or(false),
        url: s(v, "web_url"),
        updated_at: s(v, "updated_at"),
        files: vec![],
    }
}

fn parse_note(v: &Value) -> Comment {
    let pos = v.get("position");
    Comment {
        id: v.get("id").and_then(Value::as_u64).unwrap_or(0),
        author: v
            .get("author")
            .map(|u| s(u, "username"))
            .unwrap_or_default(),
        body: s(v, "body"),
        created_at: s(v, "created_at"),
        url: String::new(),
        path: pos.and_then(|p| os(p, "new_path").or_else(|| os(p, "old_path"))),
        line: pos
            .and_then(|p| p.get("new_line").or_else(|| p.get("old_line")))
            .and_then(Value::as_u64)
            .map(|n| n as u32),
        side: pos.map(|p| {
            if p.get("new_line").map(|n| !n.is_null()).unwrap_or(false) {
                Side::Right
            } else {
                Side::Left
            }
        }),
    }
}

fn parse_project(v: &Value) -> RepoSummary {
    let full = s(v, "path_with_namespace");
    let (owner, name) = full
        .rsplit_once('/')
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .unwrap_or((String::new(), full.clone()));
    RepoSummary {
        owner,
        name,
        full_name: full,
        private: s(v, "visibility") == "private",
        default_branch: s(v, "default_branch"),
        https_url: s(v, "http_url_to_repo"),
        description: os(v, "description"),
    }
}

fn parse_issue(v: &Value) -> Issue {
    Issue {
        number: v.get("iid").and_then(Value::as_u64).unwrap_or(0),
        title: s(v, "title"),
        body: s(v, "description"),
        url: s(v, "web_url"),
        open: s(v, "state") == "opened",
    }
}

impl Provider for GitLab {
    fn current_user(&self) -> Result<User> {
        let v: Value = self.get("/user")?;
        Ok(User {
            login: s(&v, "username"),
            name: os(&v, "name"),
            email: os(&v, "email").or_else(|| os(&v, "public_email")),
            avatar_url: os(&v, "avatar_url"),
        })
    }

    fn list_repos(&self) -> Result<Vec<RepoSummary>> {
        let items: Vec<Value> =
            self.all_pages("/projects?membership=true&order_by=last_activity_at&simple=true")?;
        Ok(items.iter().map(parse_project).collect())
    }

    fn create_repo(
        &self,
        name: &str,
        description: &str,
        private: bool,
        org: Option<&str>,
    ) -> Result<RepoSummary> {
        let mut body = json!({ "name": name, "description": description, "visibility": if private { "private" } else { "public" }, "initialize_with_readme": false });
        if let Some(group) = org {
            let groups: Vec<Value> = self.get(&format!("/groups?search={}", urlencode(group)))?;
            if let Some(g) = groups
                .iter()
                .find(|g| s(g, "full_path") == group || s(g, "path") == group)
            {
                body["namespace_id"] = g.get("id").cloned().unwrap_or(Value::Null);
            }
        }
        let v: Value = self.post("/projects", &body)?;
        Ok(parse_project(&v))
    }

    fn default_branch_protected(&self, repo: &RepoRef, branch: &str) -> Result<Option<bool>> {
        match self.get::<Value>(&format!(
            "/projects/{}/protected_branches/{}",
            Self::project(repo),
            urlencode(branch)
        )) {
            Ok(_) => Ok(Some(true)),
            Err(ProviderError::Status { status: 404, .. }) => Ok(Some(false)),
            Err(e) => Err(e),
        }
    }

    fn list_open_pulls(&self, repo: &RepoRef) -> Result<Vec<PullRequest>> {
        let items: Vec<Value> = self.all_pages(&format!(
            "/projects/{}/merge_requests?state=opened",
            Self::project(repo)
        ))?;
        let mut out = Vec::new();
        for v in &items {
            let mut pr = parse_mr(v);
            pr.files = self.pull_files(repo, pr.number)?;
            out.push(pr);
        }
        Ok(out)
    }

    fn get_pull(&self, repo: &RepoRef, number: u64) -> Result<PullRequest> {
        let v: Value = self.get(&Self::mr_path(repo, number))?;
        Ok(parse_mr(&v))
    }

    fn pull_files(&self, repo: &RepoRef, number: u64) -> Result<Vec<String>> {
        let items: Vec<Value> =
            self.all_pages(&format!("{}/diffs", Self::mr_path(repo, number)))?;
        Ok(items.iter().map(|d| s(d, "new_path")).collect())
    }

    fn create_pull(&self, repo: &RepoRef, pull: &NewPull) -> Result<PullRequest> {
        let title = if pull.draft {
            format!("Draft: {}", pull.title)
        } else {
            pull.title.clone()
        };
        let v: Value = self.post(
            &format!("/projects/{}/merge_requests", Self::project(repo)),
            &json!({ "source_branch": pull.head, "target_branch": pull.base, "title": title, "description": pull.body, "remove_source_branch": true }),
        )?;
        Ok(parse_mr(&v))
    }

    fn update_pull(
        &self,
        repo: &RepoRef,
        number: u64,
        title: &str,
        body: &str,
    ) -> Result<PullRequest> {
        let v: Value = self.put(
            &Self::mr_path(repo, number),
            &json!({ "title": title, "description": body }),
        )?;
        Ok(parse_mr(&v))
    }

    fn mergeability(&self, repo: &RepoRef, number: u64) -> Result<Mergeability> {
        let v: Value = self.get(&Self::mr_path(repo, number))?;
        let status = s(&v, "detailed_merge_status");
        let mergeable = match status.as_str() {
            "mergeable" => Some(true),
            "checking" | "unchecked" | "" => None,
            _ => Some(false),
        };
        let (mut approvals, mut changes_requested) = (0u32, false);
        match self.get::<Value>(&format!("{}/approvals", Self::mr_path(repo, number))) {
            Ok(a) => {
                approvals = a
                    .get("approved_by")
                    .and_then(Value::as_array)
                    .map(|x| x.len() as u32)
                    .unwrap_or(0)
            }
            Err(ProviderError::Status { .. }) => {}
            Err(e) => return Err(e),
        }
        // Marker notes cover instances without approvals and the request-changes event (D29).
        // Anyone who can comment can post a marker, so the MR author's own notes are ignored
        // and every other marker author must hold Developer access or above (review S6).
        let mr_author = v
            .get("author")
            .map(|u| s(u, "username"))
            .unwrap_or_default();
        let notes: Vec<Value> =
            self.all_pages(&format!("{}/notes?sort=asc", Self::mr_path(repo, number)))?;
        let mut marker_state: std::collections::HashMap<String, &str> = Default::default();
        let mut access_cache: std::collections::HashMap<u64, bool> = Default::default();
        for n in &notes {
            let body = s(n, "body");
            let is_marker =
                body.starts_with(MARKER_APPROVED) || body.starts_with(MARKER_CHANGES_REQUESTED);
            if !is_marker {
                continue;
            }
            let author = n.get("author").cloned().unwrap_or(Value::Null);
            let user = s(&author, "username");
            if user.is_empty() || user == mr_author {
                continue;
            }
            let uid = author.get("id").and_then(Value::as_u64).unwrap_or(0);
            let trusted = match access_cache.get(&uid) {
                Some(t) => *t,
                None => {
                    let level = self
                        .get::<Value>(&format!(
                            "/projects/{}/members/all/{uid}",
                            Self::project(repo)
                        ))
                        .ok()
                        .and_then(|m| m.get("access_level").and_then(Value::as_u64))
                        .unwrap_or(0);
                    let t = level >= 30; // Developer
                    access_cache.insert(uid, t);
                    t
                }
            };
            if !trusted {
                continue;
            }
            if body.starts_with(MARKER_APPROVED) {
                marker_state.insert(user, "approved");
            } else {
                marker_state.insert(user, "changes");
            }
        }
        if approvals == 0 {
            approvals = marker_state.values().filter(|v| **v == "approved").count() as u32;
        }
        changes_requested |= marker_state.values().any(|v| *v == "changes");
        let checks_passing = v
            .get("head_pipeline")
            .and_then(|p| p.get("status"))
            .and_then(Value::as_str)
            .map(|st| match st {
                "success" => Some(true),
                "running"
                | "pending"
                | "created"
                | "waiting_for_resource"
                | "preparing"
                | "scheduled" => None,
                _ => Some(false),
            });
        Ok(Mergeability {
            mergeable,
            state: status,
            approvals,
            changes_requested,
            checks_passing: checks_passing.flatten(),
        })
    }

    fn merge_pull(&self, repo: &RepoRef, number: u64, method: MergeMethod) -> Result<()> {
        let _: Value = self.put(&format!("{}/merge", Self::mr_path(repo, number)), &json!({ "squash": method == MergeMethod::Squash, "should_remove_source_branch": true }))?;
        Ok(())
    }

    fn list_comments(&self, repo: &RepoRef, number: u64) -> Result<Vec<Comment>> {
        let notes: Vec<Value> =
            self.all_pages(&format!("{}/notes?sort=asc", Self::mr_path(repo, number)))?;
        Ok(notes
            .iter()
            .filter(|n| !n.get("system").and_then(Value::as_bool).unwrap_or(false))
            .map(parse_note)
            .collect())
    }

    fn create_comment(&self, repo: &RepoRef, number: u64, body: &str) -> Result<Comment> {
        let v: Value = self.post(
            &format!("{}/notes", Self::mr_path(repo, number)),
            &json!({ "body": body }),
        )?;
        Ok(parse_note(&v))
    }

    fn update_comment(&self, repo: &RepoRef, comment_id: u64, body: &str) -> Result<Comment> {
        // GitLab note updates need the MR iid; callers pass the note id only, so search open MRs.
        for mr in self.list_open_pulls(repo)? {
            let notes: Vec<Value> =
                self.all_pages(&format!("{}/notes", Self::mr_path(repo, mr.number)))?;
            if notes
                .iter()
                .any(|n| n.get("id").and_then(Value::as_u64) == Some(comment_id))
            {
                let v: Value = self.put(
                    &format!("{}/notes/{comment_id}", Self::mr_path(repo, mr.number)),
                    &json!({ "body": body }),
                )?;
                return Ok(parse_note(&v));
            }
        }
        Err(ProviderError::Status {
            status: 404,
            url: format!("note {comment_id}"),
            body: "note not found on any open merge request".into(),
        })
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
        let mr: Value = self.get(&Self::mr_path(repo, number))?;
        let refs = mr.get("diff_refs").cloned().unwrap_or(Value::Null);
        let mut position = json!({
            "position_type": "text",
            "base_sha": refs.get("base_sha"), "head_sha": refs.get("head_sha"), "start_sha": refs.get("start_sha"),
            "new_path": path, "old_path": path,
        });
        if side == Side::Right {
            position["new_line"] = json!(line);
        } else {
            position["old_line"] = json!(line);
        }
        let v: Value = self.post(
            &format!("{}/discussions", Self::mr_path(repo, number)),
            &json!({ "body": body, "position": position }),
        )?;
        let note = v
            .get("notes")
            .and_then(Value::as_array)
            .and_then(|n| n.first())
            .cloned()
            .unwrap_or(Value::Null);
        Ok(parse_note(&note))
    }

    fn submit_review(
        &self,
        repo: &RepoRef,
        number: u64,
        event: ReviewEvent,
        body: &str,
    ) -> Result<()> {
        match event {
            ReviewEvent::Approve => {
                match self.post::<Value>(
                    &format!("{}/approve", Self::mr_path(repo, number)),
                    &json!({}),
                ) {
                    Ok(_) => {}
                    // Instances or tiers without approvals: fall back to the marker note.
                    Err(ProviderError::Status { status, .. })
                        if status == 401 || status == 403 || status == 404 || status == 405 =>
                    {
                        self.create_comment(
                            repo,
                            number,
                            &format!("{MARKER_APPROVED}\n**Approved** via kmdn.\n\n{body}"),
                        )?;
                        return Ok(());
                    }
                    Err(e) => return Err(e),
                }
                if !body.trim().is_empty() {
                    self.create_comment(repo, number, body)?;
                }
                Ok(())
            }
            ReviewEvent::RequestChanges => {
                let _ = self.post::<Value>(
                    &format!("{}/unapprove", Self::mr_path(repo, number)),
                    &json!({}),
                );
                self.create_comment(
                    repo,
                    number,
                    &format!(
                        "{MARKER_CHANGES_REQUESTED}\n**Changes requested** via kmdn.\n\n{body}"
                    ),
                )?;
                Ok(())
            }
            ReviewEvent::Comment => {
                self.create_comment(repo, number, body)?;
                Ok(())
            }
        }
    }

    fn find_issue(&self, repo: &RepoRef, label: &str, title: &str) -> Result<Option<Issue>> {
        let items: Vec<Value> = self.all_pages(&format!(
            "/projects/{}/issues?state=opened&labels={}&search={}&in=title",
            Self::project(repo),
            urlencode(label),
            urlencode(title)
        ))?;
        Ok(items
            .iter()
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
            &format!("/projects/{}/issues", Self::project(repo)),
            &json!({ "title": title, "description": body, "labels": labels.join(",") }),
        )?;
        Ok(parse_issue(&v))
    }

    fn list_issue_comments(&self, repo: &RepoRef, number: u64) -> Result<Vec<Comment>> {
        let notes: Vec<Value> = self.all_pages(&format!(
            "/projects/{}/issues/{number}/notes?sort=asc",
            Self::project(repo)
        ))?;
        Ok(notes
            .iter()
            .filter(|n| !n.get("system").and_then(Value::as_bool).unwrap_or(false))
            .map(parse_note)
            .collect())
    }

    fn comment_issue(&self, repo: &RepoRef, number: u64, body: &str) -> Result<Comment> {
        let v: Value = self.post(
            &format!("/projects/{}/issues/{number}/notes", Self::project(repo)),
            &json!({ "body": body }),
        )?;
        Ok(parse_note(&v))
    }

    fn create_issue_labels(&self, repo: &RepoRef, number: u64, labels: &[String]) -> Result<()> {
        let _: Value = self.put(
            &Self::mr_path(repo, number),
            &json!({ "add_labels": labels.join(",") }),
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::{Matcher, Server};

    fn repo() -> RepoRef {
        RepoRef {
            owner: "team/sub".into(),
            name: "kb".into(),
        }
    }
    const P: &str = "/projects/team%2Fsub%2Fkb";

    #[test]
    fn lists_open_mrs_with_files() {
        let mut server = Server::new();
        let _mrs = server.mock("GET", Matcher::Regex(format!(r"^{P}/merge_requests\?state=opened.*")))
            .match_header("private-token", "glpat")
            .with_body(r#"[{"iid":3,"title":"Update deploy","description":"d","state":"opened","draft":false,"web_url":"u3","updated_at":"x","author":{"username":"alice"},"source_branch":"kmdn/alice/deploy","target_branch":"main"}]"#).create();
        let _diffs = server
            .mock(
                "GET",
                Matcher::Regex(format!(r"^{P}/merge_requests/3/diffs.*")),
            )
            .with_body(r#"[{"new_path":"ops/deploy.md"}]"#)
            .create();
        let gl = GitLab::with_api_url(&server.url(), "glpat");
        let prs = gl.list_open_pulls(&repo()).unwrap();
        assert_eq!(prs.len(), 1);
        assert_eq!(prs[0].files, vec!["ops/deploy.md"]);
        assert_eq!(prs[0].head_branch, "kmdn/alice/deploy");
        assert!(touches_markdown(&prs[0].files));
    }

    #[test]
    fn creates_mr_and_approves_natively_or_with_marker() {
        let mut server = Server::new();
        let create = server.mock("POST", format!("{P}/merge_requests").as_str())
            .match_body(Matcher::PartialJsonString(r#"{"source_branch":"kmdn/a/x","target_branch":"main","title":"T"}"#.into()))
            .with_status(201)
            .with_body(r#"{"iid":4,"title":"T","description":"b","state":"opened","web_url":"u4","updated_at":"x","author":{"username":"a"},"source_branch":"kmdn/a/x","target_branch":"main"}"#).create();
        let approve = server
            .mock("POST", format!("{P}/merge_requests/4/approve").as_str())
            .with_status(201)
            .with_body("{}")
            .create();
        let gl = GitLab::with_api_url(&server.url(), "glpat");
        let pr = gl
            .create_pull(
                &repo(),
                &NewPull {
                    title: "T".into(),
                    body: "b".into(),
                    head: "kmdn/a/x".into(),
                    base: "main".into(),
                    draft: false,
                },
            )
            .unwrap();
        assert_eq!(pr.number, 4);
        gl.submit_review(&repo(), 4, ReviewEvent::Approve, "")
            .unwrap();
        create.assert();
        approve.assert();

        // request changes always uses the marker note; approve falls back when the endpoint is missing
        let note = server
            .mock("POST", format!("{P}/merge_requests/4/notes").as_str())
            .match_body(Matcher::Regex("kmdn:changes-requested".into()))
            .with_status(201)
            .with_body(r#"{"id":9,"body":"x","author":{"username":"bob"},"created_at":"x"}"#)
            .create();
        let _unapprove = server
            .mock("POST", format!("{P}/merge_requests/4/unapprove").as_str())
            .with_status(201)
            .with_body("{}")
            .create();
        gl.submit_review(&repo(), 4, ReviewEvent::RequestChanges, "please fix")
            .unwrap();
        note.assert();

        let mut server2 = Server::new();
        let _no_approve = server2
            .mock("POST", format!("{P}/merge_requests/4/approve").as_str())
            .with_status(404)
            .with_body(r#"{"message":"404 Not Found"}"#)
            .create();
        let marker = server2
            .mock("POST", format!("{P}/merge_requests/4/notes").as_str())
            .match_body(Matcher::Regex("kmdn:approved".into()))
            .with_status(201)
            .with_body(r#"{"id":10,"body":"x","author":{"username":"bob"},"created_at":"x"}"#)
            .create();
        let gl2 = GitLab::with_api_url(&server2.url(), "glpat");
        gl2.submit_review(&repo(), 4, ReviewEvent::Approve, "lgtm")
            .unwrap();
        marker.assert();
    }

    #[test]
    fn mergeability_reads_native_approvals_then_markers() {
        let mut server = Server::new();
        let _mr = server.mock("GET", format!("{P}/merge_requests/4").as_str())
            .with_body(r#"{"iid":4,"state":"opened","detailed_merge_status":"mergeable","head_pipeline":{"status":"success"},"source_branch":"s","target_branch":"main","author":{"username":"a"}}"#).create();
        let _ap = server
            .mock("GET", format!("{P}/merge_requests/4/approvals").as_str())
            .with_status(404)
            .with_body("{}")
            .create();
        let _notes = server.mock("GET", Matcher::Regex(format!(r"^{P}/merge_requests/4/notes.*")))
            .with_body(r#"[{"id":1,"body":"<!-- kmdn:approved -->\nok","author":{"username":"bob","id":2},"created_at":"a"},{"id":2,"body":"<!-- kmdn:changes-requested -->\nno","author":{"username":"carol","id":3},"created_at":"b"},{"id":3,"body":"<!-- kmdn:approved -->\nself","author":{"username":"a","id":1},"created_at":"c"},{"id":4,"body":"<!-- kmdn:approved -->\nguest","author":{"username":"dave","id":4},"created_at":"d"}]"#).create();
        let _bob = server
            .mock("GET", format!("{P}/members/all/2").as_str())
            .with_body(r#"{"id":2,"access_level":30}"#)
            .create();
        let _carol = server
            .mock("GET", format!("{P}/members/all/3").as_str())
            .with_body(r#"{"id":3,"access_level":40}"#)
            .create();
        let _dave = server
            .mock("GET", format!("{P}/members/all/4").as_str())
            .with_body(r#"{"id":4,"access_level":10}"#)
            .create();
        let gl = GitLab::with_api_url(&server.url(), "glpat");
        let m = gl.mergeability(&repo(), 4).unwrap();
        assert_eq!(
            m,
            Mergeability {
                mergeable: Some(true),
                state: "mergeable".into(),
                approvals: 1,
                changes_requested: true,
                checks_passing: Some(true)
            }
        );
    }

    #[test]
    fn inline_comment_uses_diff_refs_and_position() {
        let mut server = Server::new();
        let _mr = server.mock("GET", format!("{P}/merge_requests/4").as_str())
            .with_body(r#"{"iid":4,"state":"opened","diff_refs":{"base_sha":"b","head_sha":"h","start_sha":"s"},"source_branch":"s","target_branch":"main","author":{"username":"a"}}"#).create();
        let disc = server.mock("POST", format!("{P}/merge_requests/4/discussions").as_str())
            .match_body(Matcher::PartialJsonString(r#"{"position":{"position_type":"text","head_sha":"h","new_path":"ops/deploy.md","new_line":12}}"#.into()))
            .with_status(201)
            .with_body(r#"{"id":"d1","notes":[{"id":77,"body":"hm","author":{"username":"bob"},"created_at":"x","position":{"new_path":"ops/deploy.md","new_line":12}}]}"#).create();
        let gl = GitLab::with_api_url(&server.url(), "glpat");
        let c = gl
            .create_review_comment(&repo(), 4, "hm", "ops/deploy.md", 12, Side::Right)
            .unwrap();
        assert_eq!(c.id, 77);
        assert_eq!(c.line, Some(12));
        assert_eq!(c.side, Some(Side::Right));
        disc.assert();
    }
}
