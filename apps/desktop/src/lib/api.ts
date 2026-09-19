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
}

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
};
