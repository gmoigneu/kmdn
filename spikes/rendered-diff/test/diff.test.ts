import { splitBlocks } from "../src/blocks";
import { align } from "../src/align";
import { wordDiff } from "../src/wordDiff";
import { renderDiff, commentAnchor, toHtml } from "../src/render";

const OLD = `---
title: Deploy
---
# Deploy runbook

Run the rollout and watch the dashboard for five minutes.

## Steps

- Check the dashboard
- Announce in #ops

\`\`\`bash
kubectl rollout status deploy/api
\`\`\`

| Env | URL |
|---|---|
| prod | https://example.com |

Final paragraph stays the same.
`;

const NEW = `---
title: Deploy
---
# Deploy runbook

Run the rollout and watch the dashboard for ten minutes, then page on-call if errors rise.

## Before you start

Make sure you have kubectl access.

## Steps

- Check the dashboard
- Announce in #ops
- Update the status page

\`\`\`bash
kubectl rollout status deploy/api --timeout=5m
\`\`\`

| Env | URL |
|---|---|
| prod | https://example.com |

Final paragraph stays the same.
`;

test("splits into typed blocks with line numbers", () => {
  const b = splitBlocks(OLD).filter((x) => x.kind !== "blank");
  expect(b.map((x) => x.kind)).toEqual(["frontmatter", "heading", "paragraph", "heading", "list_item", "list_item", "fence", "table", "paragraph"]);
  expect(b[6]).toMatchObject({ kind: "fence", startLine: 13, endLine: 15 });
  expect(b[7]).toMatchObject({ kind: "table", startLine: 17, endLine: 19 });
});

test("aligns blocks: equal, modified, added", () => {
  const ops = align(splitBlocks(OLD), splitBlocks(NEW));
  const types = ops.map((o) => o.type);
  expect(types.filter((t) => t === "equal").length).toBe(7);
  expect(types.filter((t) => t === "modified").length).toBe(2);
  expect(types).toContain("modified");
  expect(types.filter((t) => t === "added").length).toBe(3);
  expect(types).not.toContain("removed");
});

test("word diff highlights only changed words", () => {
  const segs = wordDiff("watch the dashboard for five minutes.", "watch the dashboard for ten minutes, then page on-call.");
  expect(segs.find((s) => s.type === "del")?.text).toBe("five");
  expect(segs.filter((s) => s.type === "ins").map((s) => s.text).join("|")).toBe("ten|, then page on-call");
});

test("renders modified prose with ins/del, fences line-level, anchors map to new-file lines", () => {
  const rows = renderDiff(OLD, NEW);
  const para = rows.find((r) => r.type === "modified" && r.new?.includes("<p>"));
  expect(para?.new).toContain("<ins>ten</ins>");
  expect(para?.old).toContain("<del>five</del>");
  expect(para?.new?.match(/<ins>/g)?.length).toBe(2);
  const fence = rows.find((r) => r.type === "modified" && r.new?.includes('class="raw"'));
  expect(fence?.new).toContain("<ins>kubectl rollout status deploy/api --timeout=5m");
  expect(commentAnchor(para!)).toEqual({ side: "RIGHT", line: 6 });
  const added = rows.filter((r) => r.type === "added");
  expect(commentAnchor(added[0])).toEqual({ side: "RIGHT", line: 8 });
  expect(toHtml(rows)).toContain('data-line-new="6"');
});

test("50-block document with insert, delete, edit renders quickly", () => {
  const blocks = Array.from({ length: 50 }, (_, i) => `## Section ${i}\n\nParagraph ${i} with some words that describe step ${i} of the process in detail.\n`);
  const oldDoc = blocks.join("\n");
  const newBlocks = [...blocks];
  newBlocks.splice(10, 1);
  newBlocks[20] = newBlocks[20].replace("some words", "several carefully chosen words");
  newBlocks.splice(30, 0, "## Inserted\n\nBrand new paragraph.\n");
  const t0 = performance.now();
  const rows = renderDiff(oldDoc, newBlocks.join("\n"));
  const ms = performance.now() - t0;
  const c = (t: string) => rows.filter((r) => r.type === t).length;
  expect(c("removed")).toBe(2);
  expect(c("added")).toBe(2);
  expect(c("modified")).toBe(1);
  expect(ms).toBeLessThan(500);
  console.log(`50-block diff: ${ms.toFixed(1)} ms`);
});
