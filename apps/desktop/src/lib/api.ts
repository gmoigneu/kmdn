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

export interface KbInfo {
  root: string;
  remote: RemoteInfo | null;
  default_branch: string;
  head_branch: string | null;
  dirty_paths: string[];
  config: KbConfig;
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

export const api = {
  openKb: (path: string) => invoke<KbInfo>("open_kb", { path }),
  listDocuments: (root: string) => invoke<Document[]>("list_documents", { root }),
  readDocument: (root: string, path: string) => invoke<string>("read_document", { root, path }),
  listThreads: (root: string) => invoke<ThreadWorktree[]>("list_threads", { root }),
  createThread: (root: string, user: string, slug: string) =>
    invoke<ThreadWorktree>("create_thread", { root, user, slug }),
  runChecks: (root: string) => invoke<Finding[]>("run_checks", { root }),
};
