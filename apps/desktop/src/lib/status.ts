// One place for the thread / review lifecycle labels shown in the sidebar, thread header,
// and review header (D44 vocabulary: draft, in review, done, published).
import type { PullRequest } from "@/lib/api";

export type StatusTone = "muted" | "accent" | "ok";
export interface Status { label: string; tone: StatusTone }

/** Status of a local thread given its matching PR, if any. */
export function threadStatus(thread: { merged_at?: number | null }, pr?: PullRequest | null): Status {
  if (thread.merged_at) return { label: "done", tone: "ok" };
  if (pr && !pr.draft) return { label: "in review", tone: "accent" };
  return { label: "draft", tone: "muted" };
}

/** Status of a PR viewed on its own. */
export function reviewStatus(pr: PullRequest): Status {
  if (pr.state === "merged") return { label: "published", tone: "ok" };
  if (pr.draft) return { label: "draft", tone: "muted" };
  return { label: "in review", tone: "accent" };
}

export const toneClass: Record<StatusTone, string> = {
  muted: "border-border text-fg-muted",
  accent: "border-accent text-accent",
  ok: "border-ok text-ok",
};
