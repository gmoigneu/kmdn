import { useMemo } from "react";
import type { FileChange } from "@/lib/api";
import { renderDiff, type RenderedRow } from "@/lib/rdiff/render";
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

export function RenderedDiff({ change }: { change: FileChange }) {
  const rows = useMemo(() => renderDiff(change.old ?? "", change.new ?? ""), [change.old, change.new]);
  if (change.binary) return <p className="text-fg-muted text-xs px-3">Binary file, {change.status}.</p>;
  return <div className="rdiff prose-pane text-[14px]">{rows.map((r, i) => <Row key={i} row={r} />)}</div>;
}
