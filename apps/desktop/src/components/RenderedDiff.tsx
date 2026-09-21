import { useMemo, useState } from "react";
import type { FileChange } from "@/lib/api";
import { groupRows, renderDiff, type RenderedRow, type RowGroup } from "@/lib/rdiff/render";
import { cn } from "@/lib/utils";

function Row({ row }: { row: RenderedRow }) {
  const tone = row.type === "added" ? "border-l-ok" : row.type === "removed" ? "border-l-danger" : row.type === "modified" ? "border-l-warn" : "border-l-transparent";
  if (row.type === "equal") {
    return <div className={cn("rdiff-row px-3 py-1 border-l-2 opacity-60", tone)} data-line-new={row.lineNew} dangerouslySetInnerHTML={{ __html: row.new ?? "" }} />;
  }
  if (row.type === "added" || row.type === "removed") {
    return <div className={cn("rdiff-row px-3 py-1 border-l-2", tone)} data-line-new={row.lineNew} data-line-old={row.lineOld} dangerouslySetInnerHTML={{ __html: row.new ?? row.old ?? "" }} />;
  }
  return (
    <div className={cn("rdiff-row grid grid-cols-2 gap-3 px-3 py-1 border-l-2", tone)} data-line-new={row.lineNew} data-line-old={row.lineOld}>
      <div className="opacity-70" dangerouslySetInnerHTML={{ __html: row.old ?? "" }} />
      <div dangerouslySetInnerHTML={{ __html: row.new ?? "" }} />
    </div>
  );
}

/** A folded run of unchanged blocks; one click renders them (review P1, D44). */
export function UnchangedRun({ group, render }: { group: Extract<RowGroup, { kind: "unchanged" }>; render: (row: RenderedRow, index: number) => React.ReactNode }) {
  const [open, setOpen] = useState(false);
  if (open) return <>{group.rows.map((r, k) => render(r, group.start + k))}</>;
  return (
    <button onClick={() => setOpen(true)} className="w-full text-left px-3 py-1 text-[11px] text-fg-muted border-l-2 border-l-transparent hover:bg-bg-muted">
      {group.rows.length} unchanged block{group.rows.length === 1 ? "" : "s"} hidden. Show
    </button>
  );
}

export function RenderedDiff({ change }: { change: FileChange }) {
  const groups = useMemo(() => groupRows(renderDiff(change.old ?? "", change.new ?? "")), [change.old, change.new]);
  if (change.binary) return <p className="text-fg-muted text-xs px-3">Binary file, {change.status}.</p>;
  return (
    <div className="rdiff prose-pane text-[14px]">
      {groups.map((g, i) => g.kind === "row" ? <Row key={g.index} row={g.row} /> : <UnchangedRun key={`u${i}`} group={g} render={(r, idx) => <Row key={idx} row={r} />} />)}
    </div>
  );
}
