/**
 * SQL Editor: write PostgreSQL with hints from the backend language service, define
 * `{{variables}}` with their values in the Variables panel, run, and save the query.
 *
 * Left: saved queries. Centre: editor, filter bar and results. Right: Variables panel.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Group, Panel, Separator } from "react-resizable-panels";
import type { EditorView } from "@uiw/react-codemirror";
import {
  AlertCircle,
  Check,
  Copy,
  FileCode2,
  Loader2,
  MoreVertical,
  Play,
  Plus,
  RefreshCw,
  Save,
  Search,
  Square,
  Trash2,
} from "lucide-react";
import {
  client,
  type SavedQuery,
  type SqlVariableReference,
  type VariableDef,
  type VariableValue,
} from "@/services/client";
import { SqlEditor, sqlToRun } from "@/components/sql-editor/SqlEditor";
import { refreshSqlDiagnostics, sqlLanguageService, type CheckState } from "@/components/sql-editor/language";
import { Button } from "@/components/ui/button";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Tooltip } from "@/components/ui/tooltip";
import { formatCount } from "@/lib/format";
import { cn } from "@/lib/utils";
import type { ConnectionProfile } from "@/services/client";
import { QueryResults, type Run } from "@/features/workspace/QueryResults";
import { SEPARATOR_CLASS } from "@/features/workspace/styles";
import { blankDraft, sameDraft, syncVariables, toDraft, uniqueName, valueProblem, type QueryDraft } from "./model";
import { VariablesPanel } from "./VariablesPanel";

const ROW_LIMITS = [100, 500, 1000, 5000, 10000];

type Props = {
  profile: ConnectionProfile;
  /** True while this section is showing (keyboard shortcuts only apply then). */
  active: boolean;
};

export function SqlEditorPage({ profile, active }: Props) {
  const [saved, setSaved] = useState<SavedQuery[]>([]);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [baseline, setBaseline] = useState<QueryDraft>(() => blankDraft("Untitled Query"));
  const [draft, setDraft] = useState<QueryDraft>(baseline);
  /** Bumped when another query opens, to give the editor a fresh undo history. */
  const [editorKey, setEditorKey] = useState(0);
  const [showProblems, setShowProblems] = useState(false);
  const [references, setReferences] = useState<SqlVariableReference[]>([]);
  const [problemCount, setProblemCount] = useState(0);
  const [check, setCheck] = useState<CheckState>({ status: "idle" });
  const [rowLimit, setRowLimit] = useState(1000);
  const [run, setRun] = useState<Run>({ status: "idle" });
  const [now, setNow] = useState(() => performance.now());
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [justSaved, setJustSaved] = useState(false);
  const [pendingSwitch, setPendingSwitch] = useState<(() => void) | null>(null);
  const [pendingDelete, setPendingDelete] = useState<SavedQuery | null>(null);
  /** Save was pressed while the query still had its placeholder name. */
  const [nameHint, setNameHint] = useState(false);
  const nameRef = useRef<HTMLInputElement>(null);

  const viewRef = useRef<EditorView | null>(null);
  const abortRef = useRef<AbortController | null>(null);
  const runCounter = useRef(0);
  const draftRef = useRef(draft);
  draftRef.current = draft;
  /** Variables created automatically from the SQL and not edited since. */
  const untouched = useRef(new Set<string>());

  const changed = !sameDraft(draft, baseline);

  useEffect(() => {
    let live = true;
    client.queries
      .list()
      .then((list) => live && setSaved(list))
      .catch(
        (e: unknown) => live && setLoadError(e instanceof Error ? e.message : "Saved queries could not be loaded."),
      );
    return () => {
      live = false;
    };
  }, []);

  /* ------------------------------- language service ------------------------------- */

  const onAnalysis = useCallback((analysis: { references: SqlVariableReference[]; diagnostics: unknown[] }) => {
    setReferences(analysis.references);
    setProblemCount(analysis.diagnostics.length);
    const current = draftRef.current;
    const pending = new Set(untouched.current);
    const next = syncVariables(current.variables, analysis.references, pending, current.sql);
    if (!next) return;
    untouched.current = pending;
    // Applied only if nothing else changed the draft in the meantime; the next pause re-syncs.
    setDraft((d) => (d === current ? { ...d, variables: next } : d));
  }, []);

  const language = useMemo(
    () =>
      sqlLanguageService({
        connectionId: profile.id,
        getVariables: () => draftRef.current.variables,
        onAnalysis,
        onCheck: setCheck,
      }),
    [profile.id, onAnalysis],
  );

  // New, renamed or retyped variables change the diagnostics and the server check.
  const variableShape = JSON.stringify(draft.variables.map((v) => [v.name, v.type, v.options?.source ?? null]));
  useEffect(() => {
    const view = viewRef.current;
    if (view) refreshSqlDiagnostics(view);
  }, [variableShape]);

  /* ------------------------------------- running ------------------------------------- */

  const abortSilently = () => {
    const controller = abortRef.current;
    abortRef.current = null;
    controller?.abort();
  };
  useEffect(() => abortSilently, []);

  useEffect(() => {
    if (run.status !== "running") return;
    const id = setInterval(() => setNow(performance.now()), 100);
    return () => clearInterval(id);
  }, [run.status]);

  const execute = (sql: string) => {
    if (!sql.trim()) return;
    const current = draftRef.current;
    // Each variable's value is stored with it, so a saved query reruns with the same values.
    // Only required variables are applied, so only they need a valid value.
    if (current.variables.some((v) => v.required && valueProblem(v, v.default) != null)) {
      setShowProblems(true);
      return;
    }
    setShowProblems(false);
    abortSilently();
    const controller = new AbortController();
    abortRef.current = controller;
    const isCurrent = () => abortRef.current === controller;
    setRun({ status: "running", startedAt: performance.now(), sql });
    setNow(performance.now());
    const values: Record<string, VariableValue> = Object.fromEntries(current.variables.map((v) => [v.name, v.default]));
    client.sql
      .run(profile.id, sql, current.variables, values, { rowLimit }, controller.signal)
      .then((outcome) => {
        if (isCurrent()) setRun({ status: "done", outcome, sql, runId: ++runCounter.current });
      })
      .catch((e: unknown) => {
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
  };
  const executeRef = useRef(execute);
  executeRef.current = execute;
  const runFromToolbar = () => {
    const view = viewRef.current;
    execute(view ? sqlToRun(view) : draft.sql);
  };

  /* ------------------------------ opening and saving ------------------------------ */

  function guard(action: () => void) {
    if (changed) setPendingSwitch(() => action);
    else action();
  }

  function openDraft(next: QueryDraft) {
    abortSilently();
    untouched.current = new Set();
    setNameHint(false);
    setDraft(next);
    setBaseline(next);
    setShowProblems(false);
    setReferences([]);
    setCheck({ status: "idle" });
    setRun({ status: "idle" });
    setSaveError(null);
    setEditorKey((k) => k + 1);
  }

  const allNames = saved.map((q) => q.name);
  const startNew = () => openDraft(blankDraft(uniqueName("Untitled Query", allNames)));
  const duplicate = (q: SavedQuery) =>
    openDraft({ ...toDraft(q), id: "", name: uniqueName(`${q.name} Copy`, allNames) });

  async function save() {
    if (saving) return;
    const current = draftRef.current;
    // An unnamed query is named first: highlight the name and put the cursor there.
    if (!current.name.trim() || isPlaceholderName(current.name)) {
      setNameHint(true);
      setSaveError(null);
      requestAnimationFrame(() => {
        nameRef.current?.focus();
        nameRef.current?.select();
      });
      return;
    }
    setNameHint(false);
    setSaving(true);
    setSaveError(null);
    try {
      const stored = await client.queries.save(
        { name: current.name, description: current.description, sql: current.sql, variables: current.variables },
        current.id || undefined,
      );
      setSaved((list) => [stored, ...list.filter((q) => q.id !== stored.id)]);
      const next = toDraft(stored);
      // Keep the editor and results; only the identity and baseline change.
      untouched.current = new Set();
      setDraft(next);
      setBaseline(next);
      setJustSaved(true);
    } catch (e) {
      setSaveError(e instanceof Error ? e.message : "The query could not be saved.");
    } finally {
      setSaving(false);
    }
  }

  async function remove(q: SavedQuery) {
    setPendingDelete(null);
    try {
      await client.queries.remove(q.id);
      setSaved((list) => list.filter((x) => x.id !== q.id));
      if (draft.id === q.id) startNew();
    } catch (e) {
      setSaveError(e instanceof Error ? e.message : "The query could not be deleted.");
    }
  }

  useEffect(() => {
    if (!justSaved) return;
    const t = setTimeout(() => setJustSaved(false), 1800);
    return () => clearTimeout(t);
  }, [justSaved]);

  // Ctrl+S saves while this page is showing.
  const saveRef = useRef(save);
  saveRef.current = save;
  useEffect(() => {
    if (!active) return;
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
        e.preventDefault();
        void saveRef.current();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [active]);

  /* ----------------------------------- variables ----------------------------------- */

  const used = useMemo(() => new Set(references.map((r) => r.name)), [references]);

  const updateVariable = (name: string, next: VariableDef) => {
    untouched.current.delete(name);
    setDraft((d) => ({ ...d, variables: d.variables.map((v) => (v.name === name ? next : v)) }));
  };
  const removeVariable = (name: string) => {
    untouched.current.delete(name);
    setDraft((d) => ({ ...d, variables: d.variables.filter((v) => v.name !== name) }));
  };

  const running = run.status === "running";
  const query = search.trim().toLowerCase();
  const shown = query ? saved.filter((q) => q.name.toLowerCase().includes(query)) : saved;

  return (
    <div className="flex h-full min-h-0">
      {/* --------------------------------- saved queries --------------------------------- */}
      <aside className="flex w-[240px] shrink-0 flex-col border-r border-border bg-surface">
        <div className="flex h-10 shrink-0 items-center justify-between border-b border-border pr-1.5 pl-4">
          <h2 className="text-[13px] font-semibold">SQL Editor</h2>
          <Tooltip content="New query">
            <Button size="icon-sm" variant="ghost" aria-label="New query" onClick={() => guard(startNew)}>
              <Plus />
            </Button>
          </Tooltip>
        </div>
        {saved.length > 6 && (
          <div className="relative border-b border-border p-2">
            <Search className="pointer-events-none absolute top-1/2 left-4 size-3.5 -translate-y-1/2 text-subtle-foreground" />
            <Input
              aria-label="Search saved queries"
              placeholder="Search"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              className="h-7 pl-7"
            />
          </div>
        )}
        <div className="min-h-0 flex-1 overflow-y-auto py-1.5">
          {draft.id === "" && <QueryItem name={draft.name || "Untitled Query"} detail="Not saved yet" selected />}
          {shown.map((q) => (
            <QueryItem
              key={q.id}
              name={q.name}
              detail={
                q.variables.length
                  ? `${q.variables.length} ${q.variables.length === 1 ? "variable" : "variables"}`
                  : "No variables"
              }
              selected={draft.id === q.id}
              onOpen={() => draft.id !== q.id && guard(() => openDraft(toDraft(q)))}
              onDuplicate={() => guard(() => duplicate(q))}
              onDelete={() => setPendingDelete(q)}
            />
          ))}
          {loadError && (
            <p role="alert" className="mx-4 mt-1 text-xs text-danger">
              {loadError}
            </p>
          )}
          {!loadError && saved.length === 0 && (
            <p className="mx-4 mt-2 text-xs leading-relaxed text-subtle-foreground">
              Queries you save appear here. Saved queries work with any connection.
            </p>
          )}
          {query && shown.length === 0 && saved.length > 0 && (
            <p className="mx-4 mt-2 text-xs text-subtle-foreground">No query matches “{search.trim()}”.</p>
          )}
        </div>
      </aside>

      {/* ------------------------------------- editor ------------------------------------- */}
      <section className="flex min-w-0 flex-1 flex-col bg-background">
        <div className="flex h-10 shrink-0 items-center gap-2 border-b border-border bg-surface px-3">
          <Input
            ref={nameRef}
            aria-label="Query name"
            aria-invalid={nameHint || undefined}
            value={draft.name}
            maxLength={120}
            onChange={(e) => {
              setNameHint(false);
              setDraft({ ...draft, name: e.target.value });
            }}
            onKeyDown={(e) => {
              // Enter saves straight away once the query has a name.
              if (e.key === "Enter" && nameHint) {
                e.preventDefault();
                void save();
              }
            }}
            className={cn(
              "h-7 max-w-[260px] min-w-[120px] border-transparent bg-transparent px-1.5 text-[13px] font-semibold hover:border-border focus-visible:bg-surface-2",
              nameHint && "border-danger bg-surface-2 shadow-[0_0_0_1px_var(--danger)]",
            )}
          />
          {nameHint && (
            <span role="alert" className="shrink-0 text-[11px] font-medium text-danger">
              Name the query, then save
            </span>
          )}
          {(changed || draft.id === "") && !saveError && !nameHint && (
            <span className="shrink-0 text-[11px] text-subtle-foreground">
              {draft.id === "" ? "Not saved yet" : "Unsaved changes"}
            </span>
          )}
          {justSaved && !changed && <span className="shrink-0 text-[11px] text-success">Saved</span>}
          {saveError && (
            <span role="alert" className="min-w-0 truncate text-[11px] text-danger" title={saveError}>
              {saveError}
            </span>
          )}

          <div className="ml-auto flex shrink-0 items-center gap-2">
            <CheckStatus check={check} problems={problemCount} />
            <Tooltip content="Reload table and column names for hints">
              <Button
                size="icon-sm"
                variant="ghost"
                aria-label="Reload table and column names"
                onClick={() => {
                  void client.sql.refreshCatalog(profile.id).catch(() => undefined);
                }}
              >
                <RefreshCw />
              </Button>
            </Tooltip>
            <div className="mx-0.5 h-4 w-px bg-border" />
            <span className="hidden text-xs text-muted-foreground xl:inline">Row limit</span>
            <Select value={String(rowLimit)} onValueChange={(v) => setRowLimit(Number(v))}>
              <SelectTrigger className="h-7 w-[90px] text-xs" aria-label="Row limit">
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
            <Tooltip content="Save" shortcut="Ctrl+S">
              <Button size="sm" onClick={() => void save()} disabled={saving}>
                {saving ? <Loader2 className="animate-spin" /> : <Save />} Save
              </Button>
            </Tooltip>
            {running ? (
              <Button size="sm" variant="danger" onClick={() => abortRef.current?.abort()}>
                <Square className="fill-current" /> Cancel
              </Button>
            ) : (
              <Tooltip content="Run the selection, or the whole query" shortcut="Ctrl+Enter">
                <Button size="sm" variant="primary" onClick={runFromToolbar} disabled={!draft.sql.trim()}>
                  <Play className="fill-current" /> Run
                </Button>
              </Tooltip>
            )}
          </div>
        </div>

        <Group orientation="horizontal" className="min-h-0 flex-1">
          <Panel minSize={360}>
            <Group orientation="vertical" className="h-full">
              <Panel defaultSize="50%" minSize={90}>
                <SqlEditor
                  key={editorKey}
                  value={draft.sql}
                  onChange={(sql) => setDraft((d) => ({ ...d, sql }))}
                  onRun={(sql) => executeRef.current(sql)}
                  onViewReady={(v) => (viewRef.current = v)}
                  language={language}
                  autoFocus={active}
                  placeholder="Write a query. Press Ctrl+Space for suggestions, Ctrl+Enter to run."
                  ariaLabel={`SQL editor, ${draft.name}`}
                />
              </Panel>
              <Separator className={cn("h-px", SEPARATOR_CLASS)} />
              <Panel minSize={120}>
                <div className="flex h-full min-h-0 flex-col">
                  <div className="min-h-0 flex-1">
                    <QueryResults run={run} now={now} rowLimit={rowLimit} />
                  </div>
                </div>
              </Panel>
            </Group>
          </Panel>
          <Separator className={cn("w-px", SEPARATOR_CLASS)} />
          <Panel defaultSize={320} minSize={260} maxSize="45%">
            <VariablesPanel
              connectionId={profile.id}
              queryKey={`${draft.id}:${editorKey}`}
              sql={draft.sql}
              variables={draft.variables}
              used={used}
              showProblems={showProblems}
              onChange={updateVariable}
              onRemove={removeVariable}
            />
          </Panel>
        </Group>
      </section>

      <ConfirmDialog
        open={pendingSwitch !== null}
        title="Discard unsaved changes?"
        description={`"${draft.name}" has changes that haven't been saved. Continuing discards them.`}
        confirmLabel="Discard changes"
        tone="danger"
        onConfirm={() => {
          pendingSwitch?.();
          setPendingSwitch(null);
        }}
        onCancel={() => setPendingSwitch(null)}
      />
      <ConfirmDialog
        open={pendingDelete !== null}
        title={`Delete ${pendingDelete?.name ?? "query"}?`}
        description="The saved query and its variables are deleted. This can't be undone."
        confirmLabel="Delete Query"
        tone="danger"
        onConfirm={() => pendingDelete && void remove(pendingDelete)}
        onCancel={() => setPendingDelete(null)}
      />
    </div>
  );
}

/** "Untitled Query" or "Untitled Query 2": the name a new query starts with. */
const isPlaceholderName = (name: string) => /^untitled query( \d+)?$/i.test(name.trim());

/** Live verdict next to the toolbar: problems found while typing, or PostgreSQL's check. */
function CheckStatus({ check, problems }: { check: CheckState; problems: number }) {
  if (problems > 0)
    return (
      <span className="flex items-center gap-1.5 text-xs text-warning">
        <AlertCircle className="size-3.5" />
        {problems === 1 ? "1 problem" : `${problems} problems`}
      </span>
    );
  if (check.status === "checking")
    return (
      <span className="flex items-center gap-1.5 text-xs text-muted-foreground">
        <Loader2 className="size-3.5 animate-spin" /> Checking
      </span>
    );
  if (check.status !== "done") return null;
  const { outcome } = check;
  if (!outcome.ok)
    return (
      <Tooltip content={outcome.message}>
        <span className="flex items-center gap-1.5 text-xs text-danger" tabIndex={0}>
          <AlertCircle className="size-3.5" /> PostgreSQL error
        </span>
      </Tooltip>
    );
  const detail = outcome.parameters.length
    ? outcome.parameters.map((p) => `{{${p.reference}}} as ${p.pgType}`).join(", ")
    : "Parsed by PostgreSQL without running it.";
  return (
    <Tooltip content={detail}>
      <span className="flex items-center gap-1.5 text-xs text-success" tabIndex={0}>
        <Check className="size-3.5" strokeWidth={2.5} />
        Valid
        {outcome.columns.length > 0 && (
          <span className="text-muted-foreground">
            · {outcome.columns.length} {outcome.columns.length === 1 ? "column" : "columns"}
          </span>
        )}
      </span>
    </Tooltip>
  );
}

function QueryItem({
  name,
  detail,
  selected,
  onOpen,
  onDuplicate,
  onDelete,
}: {
  name: string;
  detail: string;
  selected: boolean;
  onOpen?: () => void;
  onDuplicate?: () => void;
  onDelete?: () => void;
}) {
  return (
    <div className={cn("group relative flex", selected && "bg-surface-2")}>
      {selected && <span aria-hidden className="absolute inset-y-0 left-0 w-0.5 bg-primary" />}
      <button
        type="button"
        onClick={onOpen}
        aria-current={selected ? "true" : undefined}
        className="flex min-w-0 flex-1 items-center gap-2.5 py-2 pr-1 pl-4 text-left outline-none hover:bg-surface-2 focus-visible:bg-surface-2"
      >
        <FileCode2 className="size-4 shrink-0 text-muted-foreground" strokeWidth={1.75} />
        <span className="min-w-0 flex-1">
          <span className="block truncate text-[13px] font-medium">{name}</span>
          <span className="block truncate text-[11px] text-subtle-foreground">{detail}</span>
        </span>
      </button>
      {onDuplicate && (
        <DropdownMenu>
          <Tooltip content="More actions">
            <DropdownMenuTrigger asChild>
              <button
                type="button"
                aria-label={`Actions for ${name}`}
                className="flex w-8 shrink-0 items-center justify-center text-muted-foreground opacity-0 outline-none group-hover:opacity-100 hover:text-foreground focus-visible:opacity-100 data-[state=open]:opacity-100"
              >
                <MoreVertical className="size-4" />
              </button>
            </DropdownMenuTrigger>
          </Tooltip>
          <DropdownMenuContent align="end" className="min-w-[170px]">
            <DropdownMenuItem onSelect={onDuplicate}>
              <Copy /> Duplicate
            </DropdownMenuItem>
            {onDelete && (
              <>
                <DropdownMenuSeparator />
                <DropdownMenuItem
                  onSelect={onDelete}
                  className="text-danger data-[highlighted]:bg-danger-soft data-[highlighted]:text-danger [&_svg]:text-danger!"
                >
                  <Trash2 /> Delete Query
                </DropdownMenuItem>
              </>
            )}
          </DropdownMenuContent>
        </DropdownMenu>
      )}
    </div>
  );
}
