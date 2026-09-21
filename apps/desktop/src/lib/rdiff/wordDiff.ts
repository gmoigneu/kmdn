// Word-level diff for prose blocks; line-level for fences and table rows.
import { lcs } from "./align";
export type Seg = { type: "equal" | "ins" | "del"; text: string };

export function tokenize(s: string): string[] { return s.match(/\s+|[\w'-]+|[^\s\w]/g) ?? []; }

export function diffTokens(a: string[], b: string[]): Seg[] {
  const pairs = lcs(a, b, (x, y) => x === y);
  const out: Seg[] = [];
  let i = 0, j = 0;
  const emit = (type: Seg["type"], text: string) => {
    if (!text) return;
    const last = out[out.length - 1];
    if (last && last.type === type) last.text += text; else out.push({ type, text });
  };
  for (const [ai, bj] of pairs) {
    emit("del", a.slice(i, ai).join("")); emit("ins", b.slice(j, bj).join("")); emit("equal", a[ai]); i = ai + 1; j = bj + 1;
  }
  emit("del", a.slice(i).join("")); emit("ins", b.slice(j).join(""));
  return out;
}
/** Word diff for prose. Blocks too large for the token table fall back to a line diff (review P1). */
export const WORD_CELL_LIMIT = 2_000_000;
export const wordDiff = (a: string, b: string) => {
  const ta = tokenize(a), tb = tokenize(b);
  if (ta.length * tb.length > WORD_CELL_LIMIT) return lineDiff(a, b);
  return diffTokens(ta, tb);
};
export const lineDiff = (a: string, b: string) => diffTokens(a.split(/(?<=\n)/), b.split(/(?<=\n)/));
