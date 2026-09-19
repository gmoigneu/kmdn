// Thin typed wrappers over Tauri commands. Types mirror kmdn-core until ts-rs generation lands (#20).
import { invoke } from "@tauri-apps/api/core";

export type ProviderKind =
  | { kind: "git_hub" }
  | { kind: "git_lab"; host: string }
  | { kind: "unknown"; host: string };

export interface RemoteInfo {
  provider: ProviderKind;
  host: string;
  owner: string;
  name: string;
  https_url: string;
  original_url: string;
}

export interface KbConfig {
  name?: string | null;
  description?: string | null;
}

export interface Author {
  name: string;
  email: string;
}

export interface KbInfo {
  root: string;
  remote: RemoteInfo | null;
  default_branch: string;
  head_branch: string | null;
  dirty_paths: string[];
  config: KbConfig;
  user: Author;
  authenticated: boolean;
}

export interface StoredHost { host: string; login: string; kind: string }
export interface AuthStatus { hosts: StoredHost[]; github_device_flow_available: boolean }
export interface DeviceCode { device_code: string; user_code: string; verification_uri: string; expires_in: number; interval: number }
export interface User { login: string; name: string | null; email: string | null; avatar_url: string | null }
export interface RepoSummary { owner: string; name: string; full_name: string; private: boolean; default_branch: string; https_url: string; description: string | null }

export type PullState = "open" | "closed" | "merged";
export interface PullRequest {
  number: number; title: string; body: string; author: string; head_branch: string; base_branch: string;
  state: PullState; draft: boolean; url: string; updated_at: string; files: string[];
}
export interface Comment { id: number; author: string; body: string; created_at: string; url: string; path: string | null; line: number | null; side: "left" | "right" | null }
export interface Submission { pull: PullRequest; created: boolean; pushed_head: string; log_comment: Comment | null }
export interface SubmitOutcome { submission: Submission | null; findings: Finding[]; error: string | null }
export interface Mergeability { mergeable: boolean | null; state: string; approvals: number; changes_requested: boolean; checks_passing: boolean | null }
export interface ReviewDetail { pull: PullRequest; changes: FileChange[]; comments: Comment[]; mergeability: Mergeability; head_sha: string }
export type ReviewEvent = "approve" | "request_changes" | "comment";
export type MergeMethod = "merge" | "squash" | "rebase";

export type Status = "draft" | "review" | "published" | "deprecated";

export interface Document {
  path: string;
  title: string;
  description: string | null;
  status: Status | null;
  order: number | null;
  tags: string[];
  owner: string | null;
  frontmatter_error: string | null;
}

export interface ThreadWorktree {
  slug: string;
  branch: string;
  path: string;
}

export interface Finding {
  level: "error" | "warning";
  kind: string;
  path: string;
  message: string;
}

export type ChangeStatus = "added" | "modified" | "deleted" | "renamed";

export interface FileChange {
  path: string;
  old_path: string | null;
  status: ChangeStatus;
  old: string | null;
  new: string | null;
  binary: boolean;
}

export type FastForward = "UpToDate" | { Forwarded: { from: string; to: string } } | { Skipped: string };
export type RebaseOutcome = "UpToDate" | { Rebased: { new_head: string } } | { Conflicts: ConflictFile[] };

export interface ConflictFile {
  path: string;
  main: string | null;
  thread: string | null;
  base: string | null;
}

export interface SyncReport {
  main: FastForward;
  threads: [string, { Ok: RebaseOutcome } | { Err: string }][];
}

export interface LocalChanges {
  head_branch: string | null;
  on_default_branch: boolean;
  dirty_paths: string[];
  foreign_branch: string | null;
  operation_in_progress: string | null;
}

export const api = {
  openKb: (path: string) => invoke<KbInfo>("open_kb", { path }),
  listDocuments: (root: string) => invoke<Document[]>("list_documents", { root }),
  readDocument: (root: string, path: string) => invoke<string>("read_document", { root, path }),
  listThreads: (root: string) => invoke<ThreadWorktree[]>("list_threads", { root }),
  createThread: (root: string, slug: string) => invoke<ThreadWorktree>("create_thread", { root, slug }),
  abandonThread: (root: string, slug: string) => invoke<void>("abandon_thread", { root, slug }),
  saveDocument: (root: string, slug: string, path: string, content: string) =>
    invoke<string | null>("save_document", { root, slug, path, content }),
  threadChanges: (root: string, slug: string) => invoke<FileChange[]>("thread_changes", { root, slug }),
  runChecks: (root: string) => invoke<Finding[]>("run_checks", { root }),
  syncNow: (root: string) => invoke<SyncReport>("sync_now", { root }),
  localChanges: (root: string) => invoke<LocalChanges>("local_changes", { root }),
  moveLocalChangesToThread: (root: string, slug: string) =>
    invoke<ThreadWorktree>("move_local_changes_to_thread", { root, slug }),
  submitThread: (root: string, slug: string, title: string, summary: string | null) =>
    invoke<SubmitOutcome>("submit_thread", { root, slug, title, summary }),
  listReviews: (root: string) => invoke<PullRequest[]>("list_reviews", { root }),
  reviewDetail: (root: string, number: number) => invoke<ReviewDetail>("review_detail", { root, number }),
  reviewComment: (root: string, number: number, body: string, path?: string, line?: number, side?: "left" | "right") =>
    invoke<Comment>("review_comment", { root, number, body, path: path ?? null, line: line ?? null, side: side ?? null }),
  reviewSubmit: (root: string, number: number, event: ReviewEvent, body: string) => invoke<void>("review_submit", { root, number, event, body }),
  reviewMerge: (root: string, number: number, method: MergeMethod = "squash") => invoke<void>("review_merge", { root, number, method }),
  cloneKb: (url: string, dest: string) => invoke<KbInfo>("clone_kb", { url, dest }),
  createKb: (host: string, name: string, description: string, org: string | null, dest: string) =>
    invoke<KbInfo>("create_kb", { host, name, description, org, dest }),
  authStatus: () => invoke<AuthStatus>("auth_status"),
  authStartDeviceFlow: () => invoke<DeviceCode>("auth_start_device_flow"),
  authPollDeviceFlow: (deviceCode: string) => invoke<string>("auth_poll_device_flow", { deviceCode }),
  authSavePat: (host: string, token: string) => invoke<User>("auth_save_pat", { host, token }),
  authSignOut: (host: string) => invoke<void>("auth_sign_out", { host }),
  listRemoteRepos: (host: string) => invoke<RepoSummary[]>("list_remote_repos", { host }),
  defaultCloneDir: (name: string) => invoke<string>("default_clone_dir", { name }),
};
