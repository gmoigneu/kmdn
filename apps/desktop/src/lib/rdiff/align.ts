// Align two block lists. LCS on normalized text gives equal pairs; unmatched runs pair up
// positionally as "modified" when kinds match; leftovers are added or removed.
import { Block } from "./blocks";

export type Op =
  | { type: "equal"; a: Block; b: Block }
  | { type: "modified"; a: Block; b: Block }
  | { type: "added"; b: Block }
  | { type: "removed"; a: Block };

const norm = (b: Block) => b.kind + "|" + b.text.replace(/\s+/g, " ").trim();

export function lcs<T>(a: T[], b: T[], eq: (x: T, y: T) => boolean): [number, number][] {
  const n = a.length, m = b.length;
  const dp: Uint32Array[] = Array.from({ length: n + 1 }, () => new Uint32Array(m + 1));
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      dp[i][j] = eq(a[i], b[j]) ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1]);
    }
  }
  const pairs: [number, number][] = [];
  let i = 0, j = 0;
  while (i < n && j < m) {
    if (eq(a[i], b[j])) { pairs.push([i, j]); i++; j++; }
    else if (dp[i + 1][j] >= dp[i][j + 1]) i++;
    else j++;
  }
  return pairs;
}

export function align(a: Block[], b: Block[]): Op[] {
  const A = a.filter((x) => x.kind !== "blank"), B = b.filter((x) => x.kind !== "blank");
  const pairs = lcs(A, B, (x, y) => norm(x) === norm(y));
  const ops: Op[] = [];
  let i = 0, j = 0;
  const flush = (ai: number, bj: number) => {
    const ra = A.slice(i, ai), rb = B.slice(j, bj);
    // Pair each new block with the first remaining old block of the same kind, in order.
    let x = 0;
    for (const nb of rb) {
      const k = ra.findIndex((ob, idx) => idx >= x && ob.kind === nb.kind);
      if (k === -1) { ops.push({ type: "added", b: nb }); continue; }
      for (; x < k; x++) ops.push({ type: "removed", a: ra[x] });
      ops.push({ type: "modified", a: ra[x], b: nb }); x++;
    }
    for (; x < ra.length; x++) ops.push({ type: "removed", a: ra[x] });
  };
  for (const [ai, bj] of pairs) { flush(ai, bj); ops.push({ type: "equal", a: A[ai], b: B[bj] }); i = ai + 1; j = bj + 1; }
  flush(A.length, B.length);
  return ops;
}
