// Text-level frontmatter edits for the properties strip (04-content-model.md: kmdn writes only
// the fields the user changed and never reorders or reformats the rest).

export interface FrontmatterView { present: boolean; fields: Record<string, string>; }

export function readFrontmatter(text: string): FrontmatterView {
  if (!text.startsWith("---\n")) return { present: false, fields: {} };
  const end = text.indexOf("\n---", 4);
  if (end < 0) return { present: false, fields: {} };
  const fields: Record<string, string> = {};
  for (const line of text.slice(4, end).split("\n")) {
    const m = /^([A-Za-z_][\w-]*):\s*(.*)$/.exec(line);
    if (m) fields[m[1]] = m[2].trim();
  }
  return { present: true, fields };
}

function yamlScalar(value: string): string {
  const v = value.trim();
  if (v === "") return '""';
  if (/^\[.*\]$/.test(v)) return v; // list literal such as [ops, deploy]
  if (/^[A-Za-z][\w ./-]*$/.test(v) && !/^(true|false|null|yes|no)$/i.test(v)) return v;
  return JSON.stringify(v);
}

/** Sets or replaces one top-level key, adding a frontmatter block when the document has none. */
export function setFrontmatterScalar(text: string, key: string, value: string): string {
  const line = `${key}: ${yamlScalar(value)}`;
  if (!text.startsWith("---\n")) return `---\n${line}\n---\n${text}`;
  const end = text.indexOf("\n---", 4);
  if (end < 0) return text;
  const lines = text.slice(4, end).split("\n");
  const idx = lines.findIndex((l) => l.startsWith(`${key}:`));
  if (value.trim() === "" && idx >= 0) lines.splice(idx, 1);
  else if (idx >= 0) lines[idx] = line;
  else if (value.trim() !== "") lines.push(line);
  return `---\n${lines.join("\n")}${text.slice(end)}`;
}

/** Relative link from one document path to another, e.g. ops/a.md -> guides/b.md gives ../guides/b.md */
export function relativeLink(fromDoc: string, toDoc: string): string {
  const from = fromDoc.split("/").slice(0, -1);
  const to = toDoc.split("/");
  let i = 0;
  while (i < from.length && i < to.length - 1 && from[i] === to[i]) i++;
  const up = from.length - i;
  return [...Array(up).fill(".."), ...to.slice(i)].join("/") || toDoc;
}
