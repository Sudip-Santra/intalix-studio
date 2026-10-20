import { useCallback, useEffect, useState, type ReactNode } from "react";
import { AlertCircle, Check, Copy, KeyRound, Link2, Loader2, RotateCw, SquareTerminal } from "lucide-react";
import { client, type QueryOutcome, type RelationDetail } from "@/services/client";
import { DataGrid } from "@/components/data-grid/DataGrid";
import { SqlEditor } from "@/components/sql-editor/SqlEditor";
import { Button } from "@/components/ui/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { formatBytes, formatCount } from "@/lib/format";
import { cn } from "@/lib/utils";
import { RELATION_META } from "./object-icons";
import type { RelationTab as RelationTabModel, RelationView } from "./tabs-state";
import { qualified, useWorkspace } from "./workspace-context";
import { ResultFooter } from "./ResultFooter";

type Props = { tab: RelationTabModel; onViewChange: (view: RelationView) => void };

export function RelationTab({ tab, onViewChange }: Props) {
  const { loadRelation, openQuery } = useWorkspace();
  const [state, setState] = useState<
    { status: "loading" } | { status: "ready"; detail: RelationDetail } | { status: "error"; message: string }
  >({ status: "loading" });

  const load = useCallback(
    (refresh = false) => {
      setState({ status: "loading" });
      loadRelation(tab.schema, tab.name, { refresh })
        .then((detail) => setState({ status: "ready", detail }))
        .catch((e) => setState({ status: "error", message: e instanceof Error ? e.message : "Something went wrong." }));
    },
    [loadRelation, tab.schema, tab.name],
  );

  useEffect(() => load(), [load]);

  const meta = RELATION_META[tab.relationKind];
  const isView = tab.relationKind === "view" || tab.relationKind === "materialized_view";
  const views: { value: RelationView; label: string; count?: number }[] = [
    { value: "columns", label: "Columns", count: state.status === "ready" ? state.detail.columns.length : undefined },
    { value: "data", label: "Data" },
    ...(!isView
      ? [
          {
            value: "indexes" as const,
            label: "Indexes",
            count: state.status === "ready" ? state.detail.indexes.length : undefined,
          },
          {
            value: "constraints" as const,
            label: "Constraints",
            count: state.status === "ready" ? state.detail.constraints.length : undefined,
          },
        ]
      : []),
    { value: "ddl", label: isView ? "Definition" : "DDL" },
  ];

  const detail = state.status === "ready" ? state.detail : null;

  return (
    <div className="flex h-full min-h-0 flex-col">
      <header className="shrink-0 border-b border-border bg-surface px-5 pt-4">
        <div className="flex items-center justify-between gap-6">
          <div className="min-w-0">
            {/* One line: name, then the key facts, all at the same text size. */}
            <div className="flex min-w-0 flex-wrap items-center gap-x-5 gap-y-1 text-[13px]">
              <h1
                className="flex min-w-0 items-center gap-2 font-semibold"
                title={`${meta.label} ${tab.schema}.${tab.name}`}
              >
                <meta.icon className="size-4 shrink-0" style={{ color: meta.color }} />
                <span className="truncate">
                  {tab.schema}.{tab.name}
                </span>
              </h1>
              <dl className="flex flex-wrap items-center gap-x-5 gap-y-1">
                {detail?.summary.estimatedRows != null && (
                  <Fact
                    label="Rows"
                    value={`≈ ${formatCount(detail.summary.estimatedRows)}`}
                    title="Planner estimate, not an exact count"
                  />
                )}
                {detail?.summary.sizeBytes != null && (
                  <Fact label="Size" value={formatBytes(detail.summary.sizeBytes)} />
                )}
                {detail && <Fact label="Owner" value={detail.owner} />}
              </dl>
            </div>
            {detail?.summary.comment && (
              <p className="mt-1 max-w-3xl text-xs text-subtle-foreground">{detail.summary.comment}</p>
            )}
          </div>
          <div className="flex shrink-0 items-center gap-1.5">
            <Button size="sm" variant="ghost" onClick={() => load(true)} aria-label="Refresh">
              <RotateCw /> Refresh
            </Button>
            <Button
              size="sm"
              variant="primary"
              onClick={() => openQuery(`SELECT *\nFROM ${qualified(tab.schema, tab.name)}\nLIMIT 100;`, tab.name)}
            >
              <SquareTerminal /> Query
            </Button>
          </div>
        </div>

        <Tabs value={tab.view} onValueChange={(v) => onViewChange(v as RelationView)} className="mt-3">
          <TabsList className="border-b-0">
            {views.map((v) => (
              <TabsTrigger key={v.value} value={v.value}>
                {v.label}
                {v.count != null && (
                  <span className="tabular text-[11px] font-normal text-subtle-foreground">{v.count}</span>
                )}
              </TabsTrigger>
            ))}
          </TabsList>
        </Tabs>
      </header>

      <div className="min-h-0 flex-1 bg-background">
        {state.status === "loading" && (
          <div className="flex items-center gap-2 p-5 text-xs text-muted-foreground">
            <Loader2 className="size-3.5 animate-spin" /> Loading {tab.schema}.{tab.name}…
          </div>
        )}
        {state.status === "error" && (
          <div className="m-5 flex max-w-xl items-start gap-2.5 border border-border bg-danger-soft p-3 text-xs">
            <AlertCircle className="mt-px size-4 shrink-0 text-danger" />
            <div className="flex flex-col items-start gap-2">
              <p className="text-[13px] font-medium">Couldn't load this {meta.label.toLowerCase()}</p>
              <p className="text-muted-foreground">{state.message}</p>
              <Button size="sm" onClick={() => load(true)}>
                <RotateCw /> Try again
              </Button>
            </div>
          </div>
        )}
        {detail && tab.view === "columns" && <ColumnsView detail={detail} />}
        {/* Data stays mounted once opened so switching views does not re-run the preview. */}
        <DataView tab={tab} hidden={tab.view !== "data"} />
        {detail && tab.view === "indexes" && <IndexesView detail={detail} />}
        {detail && tab.view === "constraints" && <ConstraintsView detail={detail} />}
        {detail && tab.view === "ddl" && <DdlView ddl={detail.ddl} />}
      </div>
    </div>
  );
}

function Fact({ label, value, title }: { label: string; value: string; title?: string }) {
  return (
    <div className="flex items-center gap-1.5" title={title}>
      <dt className="text-subtle-foreground">{label}</dt>
      <dd className="tabular font-medium text-foreground">{value}</dd>
    </div>
  );
}

/* ---------------------------------------------------------------------------------------- */

function Table({ head, children }: { head: ReactNode[]; children: ReactNode }) {
  return (
    <div className="h-full overflow-auto">
      <table className="w-full border-collapse text-[12.5px]">
        <thead className="sticky top-0 z-10 bg-surface">
          <tr>
            {head.map((h, i) => (
              <th
                key={i}
                scope="col"
                className="h-8 border-b border-border-strong px-3 text-left text-[11px] font-semibold tracking-wide whitespace-nowrap text-muted-foreground uppercase"
              >
                {h}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>{children}</tbody>
      </table>
    </div>
  );
}

const td = "h-9 border-b border-border px-3 align-middle";

function ForeignKeyLink({ schema, table, column }: { schema: string; table: string; column?: string }) {
  const { openRelation } = useWorkspace();
  return (
    <button
      type="button"
      onClick={() => openRelation(schema, table, "table")}
      className="inline-flex items-center gap-1 font-mono text-[11.5px] hover:underline"
      style={{ color: "var(--syntax-type)" }}
      title={`Open ${schema}.${table}`}
    >
      <Link2 className="size-3" />
      {schema}.{table}
      {column && `(${column})`}
    </button>
  );
}

function ColumnsView({ detail }: { detail: RelationDetail }) {
  return (
    <Table head={["#", "Column", "Type", "Nullable", "Default", "Key", "Comment"]}>
      {detail.columns.map((c) => (
        <tr key={c.name} className="hover:bg-surface-2">
          <td className={cn(td, "tabular w-10 font-mono text-[11px] text-subtle-foreground")}>{c.position}</td>
          <td className={cn(td, "font-medium whitespace-nowrap")}>
            <span className="inline-flex items-center gap-1.5">
              {c.isPrimaryKey && <KeyRound className="size-3.5 text-primary-text" aria-label="Primary key" />}
              {c.name}
            </span>
          </td>
          <td className={cn(td, "font-mono text-[12px] whitespace-nowrap")} style={{ color: "var(--syntax-type)" }}>
            {c.dataType}
          </td>
          <td className={cn(td, "text-xs whitespace-nowrap", c.nullable ? "text-muted-foreground" : "text-foreground")}>
            {c.nullable ? "Yes" : "No"}
          </td>
          <td
            className={cn(td, "max-w-[240px] truncate font-mono text-[11.5px] text-muted-foreground")}
            title={c.defaultValue ?? undefined}
          >
            {c.defaultValue ?? <span className="text-subtle-foreground">—</span>}
          </td>
          <td className={cn(td, "whitespace-nowrap")}>
            <span className="flex items-center gap-2">
              {c.isPrimaryKey && <Badge tone="primary">PK</Badge>}
              {c.foreignKey && (
                <>
                  <Badge tone="info">FK</Badge>
                  <ForeignKeyLink
                    schema={c.foreignKey.schema}
                    table={c.foreignKey.table}
                    column={c.foreignKey.column}
                  />
                </>
              )}
            </span>
          </td>
          <td className={cn(td, "max-w-[360px] truncate text-xs text-muted-foreground")} title={c.comment ?? undefined}>
            {c.comment}
          </td>
        </tr>
      ))}
    </Table>
  );
}

function Badge({
  tone,
  children,
}: {
  tone: "primary" | "info" | "muted" | "success" | "warning";
  children: ReactNode;
}) {
  const color = {
    primary: "var(--primary-text)",
    info: "var(--syntax-type)",
    muted: "var(--muted-foreground)",
    success: "var(--success)",
    warning: "var(--warning)",
  }[tone];
  return (
    <span
      className="inline-flex h-[18px] items-center border px-1.5 text-[10px] font-semibold tracking-wide"
      style={{
        color,
        borderColor: `color-mix(in srgb, ${color} 45%, transparent)`,
        background: `color-mix(in srgb, ${color} 8%, transparent)`,
      }}
    >
      {children}
    </span>
  );
}

function IndexesView({ detail }: { detail: RelationDetail }) {
  if (detail.indexes.length === 0) return <Empty>This table has no indexes.</Empty>;
  return (
    <Table head={["Name", "Columns", "Method", "Properties", "Size", "Definition"]}>
      {detail.indexes.map((ix) => (
        <tr key={ix.name} className="hover:bg-surface-2">
          <td className={cn(td, "font-medium whitespace-nowrap")}>{ix.name}</td>
          <td className={cn(td, "font-mono text-[12px] whitespace-nowrap")}>{ix.columns.join(", ")}</td>
          <td className={cn(td, "font-mono text-[11.5px] text-muted-foreground")}>{ix.method}</td>
          <td className={cn(td, "whitespace-nowrap")}>
            <span className="flex gap-1.5">
              {ix.isPrimary && <Badge tone="primary">PRIMARY</Badge>}
              {ix.isUnique && <Badge tone="success">UNIQUE</Badge>}
              {!ix.isPrimary && !ix.isUnique && <span className="text-xs text-subtle-foreground">—</span>}
            </span>
          </td>
          <td className={cn(td, "tabular text-xs whitespace-nowrap text-muted-foreground")}>
            {ix.sizeBytes != null ? formatBytes(ix.sizeBytes) : "—"}
          </td>
          <td
            className={cn(td, "max-w-[520px] truncate font-mono text-[11.5px] text-muted-foreground")}
            title={ix.definition}
          >
            {ix.definition}
          </td>
        </tr>
      ))}
    </Table>
  );
}

const CONSTRAINT_LABEL = {
  primary_key: ["PRIMARY KEY", "primary"],
  foreign_key: ["FOREIGN KEY", "info"],
  unique: ["UNIQUE", "success"],
  check: ["CHECK", "warning"],
  exclusion: ["EXCLUDE", "muted"],
} as const;

function ConstraintsView({ detail }: { detail: RelationDetail }) {
  if (detail.constraints.length === 0) return <Empty>This table has no constraints.</Empty>;
  return (
    <Table head={["Name", "Type", "Columns", "Definition"]}>
      {detail.constraints.map((k) => {
        const [label, tone] = CONSTRAINT_LABEL[k.kind];
        return (
          <tr key={k.name} className="hover:bg-surface-2">
            <td className={cn(td, "font-medium whitespace-nowrap")}>{k.name}</td>
            <td className={cn(td, "whitespace-nowrap")}>
              <Badge tone={tone}>{label}</Badge>
            </td>
            <td className={cn(td, "font-mono text-[12px] whitespace-nowrap")}>{k.columns.join(", ")}</td>
            <td
              className={cn(td, "max-w-[600px] truncate font-mono text-[11.5px] text-muted-foreground")}
              title={k.definition}
            >
              {k.references ? (
                <span className="flex items-center gap-2">
                  <span className="text-subtle-foreground">references</span>
                  <ForeignKeyLink
                    schema={k.references.schema}
                    table={k.references.table}
                    column={k.references.columns.join(", ")}
                  />
                </span>
              ) : (
                k.definition
              )}
            </td>
          </tr>
        );
      })}
    </Table>
  );
}

function DdlView({ ddl }: { ddl: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <div className="relative h-full">
      <SqlEditor value={ddl} readOnly ariaLabel="DDL" />
      <Button
        size="sm"
        className="absolute top-3 right-4"
        onClick={() =>
          navigator.clipboard
            .writeText(ddl)
            .then(() => {
              setCopied(true);
              setTimeout(() => setCopied(false), 1500);
            })
            .catch(() => undefined)
        }
      >
        {copied ? <Check /> : <Copy />} {copied ? "Copied" : "Copy"}
      </Button>
    </div>
  );
}

const PREVIEW_LIMITS = [100, 500, 1000];

function DataView({ tab, hidden }: { tab: RelationTabModel; hidden: boolean }) {
  const { profile, openQuery } = useWorkspace();
  const [limit, setLimit] = useState(100);
  const [outcome, setOutcome] = useState<QueryOutcome | null>(null);
  const [loading, setLoading] = useState(false);
  const [runId, setRunId] = useState(0);
  // Lazy: nothing runs until the Data view is first shown; after that it stays loaded.
  const [opened, setOpened] = useState(!hidden);
  useEffect(() => {
    if (!hidden) setOpened(true);
  }, [hidden]);

  useEffect(() => {
    if (!opened) return;
    const controller = new AbortController();
    setLoading(true);
    client.query
      .preview(profile.id, tab.schema, tab.name, { rowLimit: limit }, controller.signal)
      .then((o) => setOutcome(o))
      .catch((e) => {
        if (!(e instanceof DOMException && e.name === "AbortError")) {
          setOutcome({
            ok: false,
            category: "connection_lost",
            message: "Couldn't load rows.",
            hint: null,
            position: null,
            durationMs: 0,
            simulated: client.simulated,
          });
        }
      })
      .finally(() => !controller.signal.aborted && setLoading(false));
    return () => controller.abort();
  }, [opened, profile.id, tab.schema, tab.name, limit, runId]);

  if (hidden) return null;

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex h-10 shrink-0 items-center gap-2 border-b border-border bg-surface px-4">
        <span className="text-xs text-muted-foreground">Show first</span>
        <Select value={String(limit)} onValueChange={(v) => setLimit(Number(v))}>
          <SelectTrigger className="h-7 w-[92px] text-xs" aria-label="Rows to preview">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {PREVIEW_LIMITS.map((n) => (
              <SelectItem key={n} value={String(n)}>
                {formatCount(n)} rows
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Button size="sm" variant="ghost" onClick={() => setRunId((n) => n + 1)} disabled={loading}>
          {loading ? <Loader2 className="animate-spin" /> : <RotateCw />} Refresh
        </Button>
        <div className="ml-auto">
          <Button
            size="sm"
            onClick={() => openQuery(`SELECT *\nFROM ${qualified(tab.schema, tab.name)}\nLIMIT ${limit};`, tab.name)}
          >
            <SquareTerminal /> Open as query
          </Button>
        </div>
      </div>
      <div className="relative min-h-0 flex-1">
        {outcome?.ok && (
          <DataGrid
            key={`${runId}-${limit}`}
            columns={outcome.result.columns}
            rows={outcome.result.rows}
            className="h-full"
            aria-label={`Rows of ${tab.schema}.${tab.name}`}
          />
        )}
        {outcome && !outcome.ok && (
          <div className="m-4 flex max-w-xl items-start gap-2.5 border border-border bg-danger-soft p-3 text-xs">
            <AlertCircle className="mt-px size-4 shrink-0 text-danger" />
            <div>
              <p className="text-[13px] font-medium">{outcome.message}</p>
              {outcome.hint && <p className="mt-0.5 text-muted-foreground">{outcome.hint}</p>}
            </div>
          </div>
        )}
        {loading && (
          <div className="absolute inset-x-0 top-0 h-0.5 overflow-hidden bg-primary-soft">
            <div className="h-full w-1/3 animate-[progress_1.1s_ease-in-out_infinite] bg-primary" />
          </div>
        )}
      </div>
      {outcome?.ok && <ResultFooter outcome={outcome} />}
    </div>
  );
}

function Empty({ children }: { children: ReactNode }) {
  return <p className="p-5 text-xs text-subtle-foreground">{children}</p>;
}
