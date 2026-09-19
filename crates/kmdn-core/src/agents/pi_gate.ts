// kmdn write gate for pi (06-agents.md). Loaded with `pi --mode rpc -e kmdn-gate.ts`.
// MODE is rewritten by kmdn at launch: "suggest" | "edit" | "developer".
const MODE = "edit";
const ALLOWED = [/\.md$/i, /(^|\/)assets\//];
const PROTECTED = [/^AGENTS\.md$/, /^\.kmdn(\/|$)/];

function relative(p: string): string {
  const cwd = process.cwd().replace(/\/$/, "") + "/";
  return p.startsWith(cwd) ? p.slice(cwd.length) : p.replace(/^\.\//, "");
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
      const p = relative(String(input.path || input.file_path || ""));
      if (p.startsWith("..") || p.startsWith("/")) return { block: true, reason: `kmdn: path outside the knowledge base (${p})` };
      if (PROTECTED.some((r) => r.test(p))) return { block: true, reason: `kmdn: ${p} is generated or configuration, not editable by agents` };
      if (!ALLOWED.some((r) => r.test(p))) return { block: true, reason: `kmdn: writes outside markdown documents and assets are not allowed (${p})` };
      if (ctx.hasUI) {
        const ok = await ctx.ui.confirm("kmdn write", `Allow write to ${p}?`);
        if (!ok) return { block: true, reason: "kmdn: write denied by user" };
      }
    }
    return undefined;
  });
}
