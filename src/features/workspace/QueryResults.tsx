import { AlertCircle, CircleSlash, Loader2, SquareTerminal } from "lucide-react";
import type { QueryOutcome } from "@/services/client";
import { DataGrid } from "@/components/data-grid/DataGrid";
import { formatCount } from "@/lib/format";
import { ERROR_TITLES } from "@/lib/error-titles";
import { ResultFooter } from "./ResultFooter";

/** The state of one query run, shown by `QueryResults`. */
export type Run =
  | { status: "idle" }
  | { status: "running"; startedAt: number; sql: string }
  | { status: "done"; outcome: QueryOutcome; sql: string; runId: number }
  | { status: "cancelled"; sql: string };

/** Result area under a SQL editor: empty state, progress, error, or the result grid. */
export function QueryResults({ run, now, rowLimit }: { run: Run; now: number; rowLimit: number }) {
  if (run.status === "idle") {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 bg-background text-center">
        <SquareTerminal className="size-6 text-subtle-foreground" />
        <div>
          <p className="text-[13px] font-medium">Run a query to see results</p>
          <p className="mt-1 text-xs text-subtle-foreground">
            Press <Kbd>Ctrl</Kbd> + <Kbd>Enter</Kbd> to run. Up to {formatCount(rowLimit)} rows are shown.
          </p>
        </div>
      </div>
    );
  }

  if (run.status === "running") {
    return (
      <div className="flex h-full items-center justify-center gap-2.5 bg-background text-[13px] text-muted-foreground">
        <Loader2 className="size-4 animate-spin text-primary-text" />
        Running query…
        <span className="tabular font-mono text-xs text-subtle-foreground">
          {((now - run.startedAt) / 1000).toFixed(1)}s
        </span>
      </div>
    );
  }

  if (run.status === "cancelled") {
    return (
      <div className="flex h-full items-center justify-center gap-2 bg-background text-[13px] text-muted-foreground">
        <CircleSlash className="size-4" /> Query cancelled. No results were kept.
      </div>
    );
  }

  const { outcome } = run;
  if (!outcome.ok) {
    const where = outcome.position != null ? lineAndColumn(run.sql, outcome.position) : null;
    return (
      <div className="h-full overflow-auto bg-background p-4">
        <div className="flex max-w-3xl items-start gap-3 border border-border bg-danger-soft p-3.5 shadow-[inset_2px_0_0_var(--danger)]">
          <AlertCircle className="mt-0.5 size-4 shrink-0 text-danger" />
          <div className="min-w-0 text-xs">
            <p className="text-[13px] font-medium">{ERROR_TITLES[outcome.category]}</p>
            <p className="mt-1 font-mono text-[12px] break-words text-foreground">{outcome.message}</p>
            {where && (
              <p className="mt-1 text-muted-foreground">
                At line {where.line}, column {where.column}
              </p>
            )}
            {outcome.hint && <p className="mt-1.5 text-muted-foreground">{outcome.hint}</p>}
            {outcome.simulated && <p className="mt-1.5 text-[11px] text-subtle-foreground">Simulated result.</p>}
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-full min-h-0 flex-col bg-background">
      {outcome.result.columns.length === 0 ? (
        <p className="p-4 text-xs text-muted-foreground">The statement ran and returned no columns.</p>
      ) : outcome.result.rows.length === 0 ? (
        <>
          <DataGrid key={run.runId} columns={outcome.result.columns} rows={[]} className="h-9 flex-none" />
          <p className="p-4 text-xs text-muted-foreground">No rows matched.</p>
        </>
      ) : (
        <DataGrid
          key={run.runId}
          columns={outcome.result.columns}
          rows={outcome.result.rows}
          className="flex-1"
          aria-label="Query results"
        />
      )}
      <ResultFooter outcome={outcome} />
    </div>
  );
}

export function Kbd({ children }: { children: string }) {
  return (
    <kbd className="border border-border bg-surface-2 px-1 py-px font-mono text-[10.5px] text-muted-foreground">
      {children}
    </kbd>
  );
}

/** Converts PostgreSQL's 1-based character position into a line and column for display. */
function lineAndColumn(sql: string, position: number) {
  const before = sql.slice(0, Math.max(0, position - 1));
  const lines = before.split("\n");
  return { line: lines.length, column: (lines.at(-1)?.length ?? 0) + 1 };
}
