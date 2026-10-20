import { useCallback, useEffect, useRef, useState } from "react";
import { Group, Panel, Separator } from "react-resizable-panels";
import type { EditorView } from "@uiw/react-codemirror";
import { AlertCircle, CircleSlash, Loader2, Lock, Play, Square, SquareTerminal } from "lucide-react";
import { client, type QueryOutcome } from "@/services/client";
import { DataGrid } from "@/components/data-grid/DataGrid";
import { SqlEditor, sqlToRun } from "@/components/sql-editor/SqlEditor";
import { Button } from "@/components/ui/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Tooltip } from "@/components/ui/tooltip";
import { formatCount } from "@/lib/format";
import { ERROR_TITLES } from "@/lib/error-titles";
import type { QueryTab as QueryTabModel } from "./tabs-state";
import { useWorkspace } from "./workspace-context";
import { SEPARATOR_CLASS } from "./styles";
import { cn } from "@/lib/utils";
import { ResultFooter } from "./ResultFooter";

const ROW_LIMITS = [100, 500, 1000, 5000, 10000];

type Run =
  | { status: "idle" }
  | { status: "running"; startedAt: number; sql: string }
  | { status: "done"; outcome: QueryOutcome; sql: string; runId: number }
  | { status: "cancelled"; sql: string };

type Props = {
  tab: QueryTabModel;
  active: boolean;
  onSqlChange: (sql: string) => void;
};

export function QueryTab({ tab, active, onSqlChange }: Props) {
  const { profile, catalog } = useWorkspace();
  const [rowLimit, setRowLimit] = useState(1000);
  const [run, setRun] = useState<Run>({ status: "idle" });
  const [now, setNow] = useState(() => performance.now());
  const viewRef = useRef<EditorView | null>(null);
  const abortRef = useRef<AbortController | null>(null);
  const runCounter = useRef(0);

  /** Stops the in-flight query without reporting it as "cancelled" (used on unmount). */
  const abortSilently = () => {
    const controller = abortRef.current;
    abortRef.current = null;
    controller?.abort();
  };
  useEffect(() => abortSilently, []);

  // Live elapsed time while a query runs.
  useEffect(() => {
    if (run.status !== "running") return;
    const id = setInterval(() => setNow(performance.now()), 100);
    return () => clearInterval(id);
  }, [run.status]);

  const execute = useCallback(
    (sql: string) => {
      if (!sql.trim()) return;
      // Starting a new run supersedes the old one; its late result must not overwrite this one.
      abortSilently();
      const controller = new AbortController();
      abortRef.current = controller;
      const isCurrent = () => abortRef.current === controller;
      setRun({ status: "running", startedAt: performance.now(), sql });
      setNow(performance.now());
      client.query
        .execute(profile.id, sql, { rowLimit }, controller.signal)
        .then((outcome) => {
          if (isCurrent()) setRun({ status: "done", outcome, sql, runId: ++runCounter.current });
        })
        .catch((e) => {
          if (!isCurrent()) return;
          if (e instanceof DOMException && e.name === "AbortError") setRun({ status: "cancelled", sql });
          else
            setRun({
              status: "done",
              sql,
              runId: ++runCounter.current,
              outcome: {
                ok: false,
                category: "connection_lost",
                message: "The query could not be completed.",
                hint: "Check the connection and try again.",
                position: null,
                durationMs: 0,
                simulated: client.simulated,
              },
            });
        })
        .finally(() => {
          if (isCurrent()) abortRef.current = null;
        });
    },
    [profile.id, rowLimit],
  );

  // "View data" style tabs run their SQL once when they open. The cleanup lets React's
  // development double-mount re-run it instead of leaving a cancelled query behind.
  const executeRef = useRef(execute);
  executeRef.current = execute;
  const openingSql = useRef(tab.sql);
  const runOnOpen = tab.runOnOpen;
  useEffect(() => {
    if (!runOnOpen) return;
    executeRef.current(openingSql.current);
    return abortSilently;
  }, [runOnOpen]);

  const runFromToolbar = () => {
    const view = viewRef.current;
    execute(view ? sqlToRun(view) : tab.sql);
  };

  const running = run.status === "running";

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex h-10 shrink-0 items-center gap-2 border-b border-border bg-surface px-3">
        {running ? (
          <Button size="sm" variant="danger" onClick={() => abortRef.current?.abort()}>
            <Square className="fill-current" /> Cancel
          </Button>
        ) : (
          <Tooltip content="Run the selection, or the whole editor" shortcut="Ctrl+Enter">
            <Button size="sm" variant="primary" onClick={runFromToolbar} disabled={!tab.sql.trim()}>
              <Play className="fill-current" /> Run
            </Button>
          </Tooltip>
        )}
        <div className="mx-1 h-4 w-px bg-border" />
        <span className="text-xs text-muted-foreground">Row limit</span>
        <Select value={String(rowLimit)} onValueChange={(v) => setRowLimit(Number(v))}>
          <SelectTrigger className="h-7 w-[96px] text-xs" aria-label="Row limit">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {ROW_LIMITS.map((n) => (
              <SelectItem key={n} value={String(n)}>
                {formatCount(n)}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Tooltip content="Queries run in a read-only transaction. Statements that change data are rejected.">
          <span className="ml-auto inline-flex h-6 items-center gap-1.5 border border-border px-2 text-[11px] text-muted-foreground">
            <Lock className="size-3" /> Read-only
          </span>
        </Tooltip>
      </div>

      <Group orientation="vertical" className="min-h-0 flex-1">
        <Panel defaultSize="45%" minSize={90}>
          <SqlEditor
            value={tab.sql}
            onChange={onSqlChange}
            onRun={execute}
            onViewReady={(v) => (viewRef.current = v)}
            catalog={catalog}
            autoFocus={active}
            placeholder="Write a query. Press Ctrl+Enter to run it."
            ariaLabel={`SQL editor, ${tab.title}`}
          />
        </Panel>
        <Separator className={cn("h-px", SEPARATOR_CLASS)} />
        <Panel minSize={110}>
          <Results run={run} now={now} rowLimit={rowLimit} />
        </Panel>
      </Group>
    </div>
  );
}

function Results({ run, now, rowLimit }: { run: Run; now: number; rowLimit: number }) {
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

function Kbd({ children }: { children: string }) {
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
