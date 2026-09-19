//! In-memory provider for tests and for running the app without a network.

use std::collections::BTreeMap;
use std::sync::Mutex;

use super::*;

#[derive(Default)]
pub struct MockProvider {
    pub pulls: Mutex<BTreeMap<u64, PullRequest>>,
    pub comments: Mutex<BTreeMap<u64, Vec<Comment>>>,
    pub reviews: Mutex<Vec<(u64, ReviewEvent, String)>>,
    pub issues: Mutex<Vec<Issue>>,
    pub labels: Mutex<Vec<(u64, Vec<String>)>>,
    next_id: Mutex<u64>,
}

impl MockProvider {
    fn id(&self) -> u64 {
        let mut n = self.next_id.lock().unwrap();
        *n += 1;
        *n
    }
}

impl Provider for MockProvider {
    fn current_user(&self) -> Result<User> {
        Ok(User {
            login: "mock".into(),
            name: Some("Mock User".into()),
            email: Some("mock@example.com".into()),
            avatar_url: None,
        })
    }
    fn list_repos(&self) -> Result<Vec<RepoSummary>> {
        Ok(vec![])
    }
    fn create_repo(
        &self,
        name: &str,
        description: &str,
        private: bool,
        org: Option<&str>,
    ) -> Result<RepoSummary> {
        let owner = org.unwrap_or("mock").to_string();
        Ok(RepoSummary {
            full_name: format!("{owner}/{name}"),
            https_url: format!("https://mock.local/{owner}/{name}.git"),
            owner,
            name: name.into(),
            private,
            default_branch: "main".into(),
            description: Some(description.into()),
        })
    }
    fn default_branch_protected(&self, _: &RepoRef, _: &str) -> Result<Option<bool>> {
        Ok(None)
    }
    fn list_open_pulls(&self, _: &RepoRef) -> Result<Vec<PullRequest>> {
        Ok(self
            .pulls
            .lock()
            .unwrap()
            .values()
            .filter(|p| p.state == PullState::Open)
            .cloned()
            .collect())
    }
    fn get_pull(&self, _: &RepoRef, number: u64) -> Result<PullRequest> {
        self.pulls
            .lock()
            .unwrap()
            .get(&number)
            .cloned()
            .ok_or_else(|| ProviderError::Status {
                status: 404,
                url: format!("pull {number}"),
                body: String::new(),
            })
    }
    fn pull_files(&self, _: &RepoRef, number: u64) -> Result<Vec<String>> {
        Ok(self
            .pulls
            .lock()
            .unwrap()
            .get(&number)
            .map(|p| p.files.clone())
            .unwrap_or_default())
    }
    fn create_pull(&self, _: &RepoRef, pull: &NewPull) -> Result<PullRequest> {
        let number = self.id();
        let pr = PullRequest {
            number,
            title: pull.title.clone(),
            body: pull.body.clone(),
            author: "mock".into(),
            head_branch: pull.head.clone(),
            base_branch: pull.base.clone(),
            state: PullState::Open,
            draft: pull.draft,
            url: format!("https://mock.local/pull/{number}"),
            updated_at: "2026-09-19T00:00:00Z".into(),
            files: vec![],
        };
        self.pulls.lock().unwrap().insert(number, pr.clone());
        Ok(pr)
    }
    fn update_pull(
        &self,
        r: &RepoRef,
        number: u64,
        title: &str,
        body: &str,
    ) -> Result<PullRequest> {
        let mut pulls = self.pulls.lock().unwrap();
        let p = pulls
            .get_mut(&number)
            .ok_or_else(|| ProviderError::Status {
                status: 404,
                url: format!("{}/{} pull {number}", r.owner, r.name),
                body: String::new(),
            })?;
        p.title = title.into();
        p.body = body.into();
        Ok(p.clone())
    }
    fn mergeability(&self, _: &RepoRef, number: u64) -> Result<Mergeability> {
        let approvals = self
            .reviews
            .lock()
            .unwrap()
            .iter()
            .filter(|(n, e, _)| *n == number && *e == ReviewEvent::Approve)
            .count() as u32;
        Ok(Mergeability {
            mergeable: Some(true),
            state: "clean".into(),
            approvals,
            changes_requested: false,
            checks_passing: None,
        })
    }
    fn merge_pull(&self, _: &RepoRef, number: u64, _: MergeMethod) -> Result<()> {
        if let Some(p) = self.pulls.lock().unwrap().get_mut(&number) {
            p.state = PullState::Merged;
        }
        Ok(())
    }
    fn list_comments(&self, _: &RepoRef, number: u64) -> Result<Vec<Comment>> {
        Ok(self
            .comments
            .lock()
            .unwrap()
            .get(&number)
            .cloned()
            .unwrap_or_default())
    }
    fn create_comment(&self, _: &RepoRef, number: u64, body: &str) -> Result<Comment> {
        let c = Comment {
            id: self.id(),
            author: "mock".into(),
            body: body.into(),
            created_at: "2026-09-19T00:00:00Z".into(),
            url: String::new(),
            path: None,
            line: None,
            side: None,
        };
        self.comments
            .lock()
            .unwrap()
            .entry(number)
            .or_default()
            .push(c.clone());
        Ok(c)
    }
    fn update_comment(&self, _: &RepoRef, comment_id: u64, body: &str) -> Result<Comment> {
        for list in self.comments.lock().unwrap().values_mut() {
            if let Some(c) = list.iter_mut().find(|c| c.id == comment_id) {
                c.body = body.into();
                return Ok(c.clone());
            }
        }
        Err(ProviderError::Status {
            status: 404,
            url: format!("comment {comment_id}"),
            body: String::new(),
        })
    }
    fn create_review_comment(
        &self,
        _: &RepoRef,
        number: u64,
        body: &str,
        path: &str,
        line: u32,
        side: Side,
    ) -> Result<Comment> {
        let c = Comment {
            id: self.id(),
            author: "mock".into(),
            body: body.into(),
            created_at: "2026-09-19T00:00:00Z".into(),
            url: String::new(),
            path: Some(path.into()),
            line: Some(line),
            side: Some(side),
        };
        self.comments
            .lock()
            .unwrap()
            .entry(number)
            .or_default()
            .push(c.clone());
        Ok(c)
    }
    fn submit_review(
        &self,
        _: &RepoRef,
        number: u64,
        event: ReviewEvent,
        body: &str,
    ) -> Result<()> {
        self.reviews
            .lock()
            .unwrap()
            .push((number, event, body.into()));
        Ok(())
    }
    fn find_issue(&self, _: &RepoRef, _label: &str, title: &str) -> Result<Option<Issue>> {
        Ok(self
            .issues
            .lock()
            .unwrap()
            .iter()
            .find(|i| i.title == title && i.open)
            .cloned())
    }
    fn create_issue(
        &self,
        _: &RepoRef,
        title: &str,
        body: &str,
        _labels: &[&str],
    ) -> Result<Issue> {
        let i = Issue {
            number: self.id(),
            title: title.into(),
            body: body.into(),
            url: String::new(),
            open: true,
        };
        self.issues.lock().unwrap().push(i.clone());
        Ok(i)
    }
    fn list_issue_comments(&self, r: &RepoRef, number: u64) -> Result<Vec<Comment>> {
        self.list_comments(r, number)
    }
    fn comment_issue(&self, r: &RepoRef, number: u64, body: &str) -> Result<Comment> {
        self.create_comment(r, number, body)
    }
    fn create_issue_labels(&self, _: &RepoRef, number: u64, labels: &[String]) -> Result<()> {
        self.labels.lock().unwrap().push((number, labels.to_vec()));
        Ok(())
    }
}
