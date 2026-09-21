// Thin typed wrappers over Tauri commands. Types come from the Rust structs via ts-rs
// (scripts/gen-types.sh, #20); CI fails when apps/desktop/src/lib/generated drifts.
import { invoke } from "@tauri-apps/api/core";
import type {
  AgentKind,
  AgentMode,
  AuthStatus,
  Comment,
  DetectedAgent,
  DeviceCode,
  Discussion,
  Document,
  Draft,
  FileChange,
  Finding,
  KbInfo,
  LocalChanges,
  MergeMethod,
  PullRequest,
  RebaseOutcome,
  RepoSummary,
  ReviewDetail,
  ReviewEvent,
  SearchHit,
  SessionInfo,
  SubmitOutcome,
  SubmitPreview,
  Suggestion,
  SyncReport,
  ThreadWorktree,
  User
} from "./generated";

export type * from "./generated";

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
  adoptBranch: (root: string, branch: string) => invoke<ThreadWorktree>("adopt_branch", { root, branch }),
  threadConflicts: (root: string, slug: string) => invoke<RebaseOutcome>("thread_conflicts", { root, slug }),
  resolveThreadConflicts: (root: string, slug: string, resolutions: Record<string, string>) =>
    invoke<RebaseOutcome>("resolve_thread_conflicts", { root, slug, resolutions }),
  submitPreview: (root: string, slug: string) => invoke<SubmitPreview>("submit_preview", { root, slug }),
  submitSuggest: (root: string, slug: string, kind: AgentKind) => invoke<Suggestion>("submit_suggest", { root, slug, kind }),
  submitThread: (root: string, slug: string, title: string, summary: string | null, agentLog: string | null) =>
    invoke<SubmitOutcome>("submit_thread", { root, slug, title, summary, agentLog }),
  listReviews: (root: string) => invoke<PullRequest[]>("list_reviews", { root }),
  docDiscussion: (root: string, path: string) => invoke<Discussion>("doc_discussion", { root, path }),
  docDiscussionComment: (root: string, path: string, body: string) => invoke<Discussion>("doc_discussion_comment", { root, path, body }),
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
  agentDetect: () => invoke<DetectedAgent[]>("agent_detect"),
  diagnostics: () => invoke<string>("diagnostics"),
  addCiCheck: (root: string) => invoke<ThreadWorktree>("add_ci_check", { root }),
  reindexKb: (root: string) => invoke<number>("reindex_kb", { root }),
  searchDocs: (root: string, query: string) => invoke<SearchHit[]>("search_docs", { root, query }),
  agentStart: (root: string, slug: string, kind: AgentKind, mode: AgentMode, resume: string | null = null) =>
    invoke<SessionInfo>("agent_start", { root, slug, kind, mode, resume }),
  agentSend: (slug: string, text: string) => invoke<void>("agent_send", { slug, text }),
  agentReplyPermission: (slug: string, id: string, allow: boolean, reason: string | null = null) =>
    invoke<void>("agent_reply_permission", { slug, id, allow, reason }),
  agentCancel: (slug: string) => invoke<void>("agent_cancel", { slug }),
  agentStop: (slug: string) => invoke<void>("agent_stop", { slug }),
  agentSession: (slug: string) => invoke<SessionInfo | null>("agent_session", { slug }),
  saveAsset: (root: string, slug: string, docPath: string, name: string, dataBase64: string) =>
    invoke<string>("save_asset", { root, slug, docPath, name, dataBase64 }),
  draftSave: (root: string, slug: string, path: string, text: string) => invoke<void>("draft_save", { root, slug, path, text }),
  draftGet: (root: string, slug: string, path: string) => invoke<Draft | null>("draft_get", { root, slug, path }),
  draftClear: (root: string, slug: string, path: string) => invoke<void>("draft_clear", { root, slug, path }),
  draftList: (root: string, slug: string) => invoke<Draft[]>("draft_list", { root, slug }),
  agentPending: (root: string, slug: string) => invoke<string[]>("agent_pending", { root, slug }),
  agentAccept: (root: string, slug: string, paths: string[]) => invoke<string | null>("agent_accept", { root, slug, paths }),
  agentRevert: (root: string, slug: string, paths: string[]) => invoke<void>("agent_revert", { root, slug, paths }),
};
