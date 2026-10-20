import type { QueryOutcome } from "@/services/client";
import { formatCount } from "@/lib/format";

/** "100 rows · more available · 238 ms" strip under a result grid. */
export function ResultFooter({ outcome }: { outcome: Extract<QueryOutcome, { ok: true }> }) {
  const { result } = outcome;
  return (
    <div
      role="status"
      className="flex h-7 shrink-0 items-center gap-3 border-t border-border bg-surface px-4 text-[11.5px] text-muted-foreground"
    >
      <span className="tabular font-medium text-foreground">
        {formatCount(result.rows.length)} {result.rows.length === 1 ? "row" : "rows"}
      </span>
      {result.truncated && (
        <span className="text-warning">Limited to {formatCount(result.rowLimit)}. More rows are available.</span>
      )}
      <span className="tabular">{formatCount(outcome.durationMs)} ms</span>
      <span>{result.columns.length} columns</span>
      {outcome.simulated && <span className="ml-auto text-subtle-foreground">Simulated data</span>}
    </div>
  );
}
