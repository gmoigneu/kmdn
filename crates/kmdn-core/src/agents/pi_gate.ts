// kmdn write gate for pi (06-agents.md). Loaded with `pi --mode rpc -e kmdn-gate.ts`.
// MODE is rewritten by kmdn at launch: "suggest" | "edit" | "developer".
import * as path from "node:path";
import * as fs from "node:fs";

const MODE = "edit";
const ALLOWED = [/\.md$/i, /(^|\/)assets\//];
const PROTECTED_FIRST = new Set(["agents.md", ".kmdn", ".git"]);

/** Worktree-relative normalized path, or null when it escapes the worktree or crosses a symlink. */
function relative(p: string): string | null {
  if (!p || p.startsWith("~")) return null;
  const root = path.resolve(process.cwd());
  const abs = path.resolve(root, p);
  const rel = path.relative(root, abs);
  if (!rel || rel.startsWith("..") || path.isAbsolute(rel)) return null;
  let cursor = root;
  for (const seg of rel.split(path.sep)) {
    cursor = path.join(cursor, seg);
    try {
      if (fs.lstatSync(cursor).isSymbolicLink()) return null;
    } catch {
      break; // not created yet
    }
  }
  return rel.split(path.sep).join("/");
}

function protectedPath(rel: string): boolean {
  const first = rel.split("/")[0].toLowerCase();
  return rel.toLowerCase() === "agents.md" || PROTECTED_FIRST.has(first);
}

export default function (pi: any) {
  pi.on("tool_call", async (event: any, ctx: any) => {
    const name: string = event.toolName || event.name || "";
    const input = event.input || event.args || {};

    if (name === "bash") {
      if (MODE !== "developer") return { block: true, reason: "kmdn: shell is disabled outside Developer mode" };
      if (ctx.hasUI) {
        const ok = await ctx.ui.confirm("kmdn command", `Allow command: ${String(input.command ?? "").slice(0, 200)}`);
        if (!ok) return { block: true, reason: "kmdn: command denied by user" };
      }
      return undefined;
    }

    if (name === "write" || name === "edit") {
      if (MODE === "suggest") return { block: true, reason: "kmdn: Suggest mode, propose the change in your reply instead of writing files" };
      const raw = String(input.path || input.file_path || "");
      const p = relative(raw);
      if (p === null) return { block: true, reason: `kmdn: path outside the knowledge base (${raw})` };
      if (protectedPath(p)) return { block: true, reason: `kmdn: ${p} is generated or configuration, not editable by agents` };
      if (!ALLOWED.some((r) => r.test(p))) return { block: true, reason: `kmdn: writes outside markdown documents and assets are not allowed (${p})` };
      if (ctx.hasUI) {
        const ok = await ctx.ui.confirm("kmdn write", `Allow write to ${p}?`);
        if (!ok) return { block: true, reason: "kmdn: write denied by user" };
      }
      return undefined;
    }

    // Any other tool that names a path is treated as a write attempt and gated the same way.
    const maybePath = input.path || input.file_path || input.target;
    if (maybePath && name !== "read" && name !== "ls" && name !== "grep" && name !== "find" && name !== "glob") {
      const p = relative(String(maybePath));
      if (p === null || protectedPath(p) || !ALLOWED.some((r) => r.test(p))) {
        return { block: true, reason: `kmdn: ${name} on ${String(maybePath)} is not allowed` };
      }
    }
    return undefined;
  });
}
