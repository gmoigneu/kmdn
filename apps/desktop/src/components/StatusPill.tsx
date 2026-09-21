import type { Status } from "@/lib/status";
import { toneClass } from "@/lib/status";
import { cn } from "@/lib/utils";

export function StatusPill({ status, className }: { status: Status; className?: string }) {
  return <span className={cn("text-[10px] px-1.5 rounded-full border", toneClass[status.tone], className)}>{status.label}</span>;
}
