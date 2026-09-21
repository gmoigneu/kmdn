import { describe, expect, test } from "vitest";
import { splitBlocks } from "./blocks";
import { align } from "./align";
import { wordDiff } from "./wordDiff";
import { groupRows, renderDiff } from "./render";

function doc(lines: number, marker = "", at = 10) {
  const out: string[] = [];
  for (let i = 0; out.length < lines; i++) {
    out.push(`## Section ${i}`, "", `Paragraph ${i} with some words that describe step ${i}${i === at ? marker : ""}.`, "");
  }
  return out.join("\n");
}

describe("rdiff performance guards", () => {
  test("2,000-line document with one changed word aligns in well under 100 ms", () => {
    const a = doc(2000), b = doc(2000, " changed", 300);
    const t0 = performance.now();
    const ops = align(splitBlocks(a), splitBlocks(b));
    const ms = performance.now() - t0;
    expect(ops.filter((o) => o.type === "modified").length).toBe(1);
    expect(ops.filter((o) => o.type === "equal").length).toBeGreaterThan(900);
    expect(ms).toBeLessThan(100);
  });

  test("huge single paragraph falls back to a line diff instead of a quadratic table", () => {
    const line = "word ".repeat(40).trim();
    const a = Array.from({ length: 400 }, () => line).join("\n");
    const b = a.replace("word word", "word changed word");
    const t0 = performance.now();
    const segs = wordDiff(a, b);
    expect(performance.now() - t0).toBeLessThan(200);
    expect(segs.some((s) => s.type === "ins")).toBe(true);
  });

  test("unchanged runs collapse with one row of context on each side", () => {
    const rows = renderDiff(doc(120), doc(120, " changed"));
    const groups = groupRows(rows);
    const collapsed = groups.filter((g) => g.kind === "unchanged");
    expect(collapsed.length).toBeGreaterThanOrEqual(1);
    expect(groups.filter((g) => g.kind === "row" && g.row.type === "modified").length).toBe(1);
    const total = groups.reduce((n, g) => n + (g.kind === "row" ? 1 : g.rows.length), 0);
    expect(total).toBe(rows.length);
  });
});
