// kmdn write gate for pi: block writes outside allowed globs, ask host for the rest.
export default function (pi: any) {
  const allowed = [/\.md$/i, /(^|\/)assets\//];
  pi.on("tool_call", async (event: any, ctx: any) => {
    const name = event.toolName || event.name || "";
    const input = event.input || event.args || {};
    if (name === "bash") return { block: true, reason: "kmdn: shell disabled in Edit mode" };
    if (name === "write" || name === "edit") {
      const p: string = input.path || input.file_path || "";
      const ok = allowed.some((r) => r.test(p));
      if (!ok) return { block: true, reason: `kmdn: writes outside markdown and assets are not allowed (${p})` };
      // demonstrate host round-trip: ask via confirm (RPC emits extension_ui_request)
      if (ctx.hasUI) {
        const yes = await ctx.ui.confirm("kmdn write", `Allow write to ${p}?`);
        if (!yes) return { block: true, reason: "kmdn: denied by user" };
      }
    }
    return undefined;
  });
}
