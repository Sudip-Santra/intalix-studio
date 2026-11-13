import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Group, Panel, Separator } from "react-resizable-panels";
import type { EditorView } from "@uiw/react-codemirror";
import { Lock, Play, Square } from "lucide-react";
import { client } from "@/services/client";
import { SqlEditor, sqlToRun } from "@/components/sql-editor/SqlEditor";
import { sqlLanguageService } from "@/components/sql-editor/language";
import { Button } from "@/components/ui/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Tooltip } from "@/components/ui/tooltip";
import { formatCount } from "@/lib/format";
import type { QueryTab as QueryTabModel } from "./tabs-state";
import { useWorkspace } from "./workspace-context";
import { SEPARATOR_CLASS } from "./styles";
import { cn } from "@/lib/utils";
import { QueryResults, type Run } from "./QueryResults";

const ROW_LIMITS = [100, 500, 1000, 5000, 10000];

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
  // PostgreSQL hints and checks from the backend. Explorer tabs have no {{variables}}.
  const language = useMemo(
    () => sqlLanguageService({ connectionId: profile.id, getVariables: () => null }),
    [profile.id],
  );

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
            language={language}
            autoFocus={active}
            placeholder="Write a query. Press Ctrl+Enter to run it."
            ariaLabel={`SQL editor, ${tab.title}`}
          />
        </Panel>
        <Separator className={cn("h-px", SEPARATOR_CLASS)} />
        <Panel minSize={110}>
          <QueryResults run={run} now={now} rowLimit={rowLimit} />
        </Panel>
      </Group>
    </div>
  );
}
