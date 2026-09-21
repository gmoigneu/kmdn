// Align two block lists. LCS on normalized text gives equal pairs; unmatched runs pair up
// positionally as "modified" when kinds match; leftovers are added or removed.
import { Block } from "./blocks";

export type Op =
  | { type: "equal"; a: Block; b: Block }
  | { type: "modified"; a: Block; b: Block }
  | { type: "added"; b: Block }
  | { type: "removed"; a: Block };

const norm = (b: Block) => b.kind + "|" + b.text.replace(/\s+/g, " ").trim();

/** Fallback for huge inputs: equal keys at the same offset pair up, nothing else does. */
function positionalPairs(a: string[], b: string[]): [number, number][] {
  const out: [number, number][] = [];
  const n = Math.min(a.length, b.length);
  for (let i = 0; i < n; i++) if (a[i] === b[i]) out.push([i, i]);
  return out;
}

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

/** Above this many cell comparisons the LCS table is too costly; pair positionally instead. */
export const LCS_CELL_LIMIT = 4_000_000;

export function align(a: Block[], b: Block[]): Op[] {
  const A = a.filter((x) => x.kind !== "blank"), B = b.filter((x) => x.kind !== "blank");
  // Normalize once per block; the comparator then compares precomputed keys (review P1).
  const ka = A.map(norm), kb = B.map(norm);
  // Identical prefix and suffix never need the table.
  let pre = 0;
  while (pre < ka.length && pre < kb.length && ka[pre] === kb[pre]) pre++;
  let suf = 0;
  while (suf < ka.length - pre && suf < kb.length - pre && ka[ka.length - 1 - suf] === kb[kb.length - 1 - suf]) suf++;
  const midA = ka.slice(pre, ka.length - suf), midB = kb.slice(pre, kb.length - suf);
  const midPairs: [number, number][] = midA.length * midB.length > LCS_CELL_LIMIT
    ? positionalPairs(midA, midB)
    : lcs(midA, midB, (x, y) => x === y);
  const pairs: [number, number][] = [];
  for (let i = 0; i < pre; i++) pairs.push([i, i]);
  for (const [x, y] of midPairs) pairs.push([x + pre, y + pre]);
  for (let i = 0; i < suf; i++) pairs.push([ka.length - suf + i, kb.length - suf + i]);
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
