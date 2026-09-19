export function sample(lines: number): string {
  const blocks = [
    "# Deploy runbook",
    "Some **bold** text with *emphasis*, `code`, ~~gone~~ and a [link](rollback.md#step-2).",
    "",
    "## Steps",
    "- [ ] Check the dashboard",
    "- [x] Announce in #ops",
    "- Plain bullet",
    "",
    "> A quoted warning that spans one line.",
    "",
    "```bash\nkubectl rollout status deploy/api\n```",
    "",
    "| Env | URL |\n|---|---|\n| prod | https://example.com |",
    "",
    "![diagram](assets/deploy/flow.png)",
    "",
    "---",
    "",
  ];
  const out: string[] = ["---", "title: Deploy runbook", "status: published", "tags: [ops]", "---"];
  let i = 0;
  while (out.join("\n").split("\n").length < lines) { out.push(blocks[i % blocks.length]); i++; }
  return out.join("\n");
}
