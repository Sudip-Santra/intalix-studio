import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent,
  type ReactNode,
} from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import {
  AlertCircle,
  ChevronRight,
  Database,
  KeyRound,
  Link2,
  Loader2,
  MoreHorizontal,
  RotateCw,
  Search,
  SquareTerminal,
  X,
  type LucideIcon,
} from "lucide-react";
import { client, type ColumnInfo, type SchemaObjects, type SchemaSummary, type DatabaseInfo } from "@/services/client";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Tooltip } from "@/components/ui/tooltip";
import { formatBytes, formatCompact, formatCount } from "@/lib/format";
import { cn } from "@/lib/utils";
import { RELATION_META, ROUTINE_ICON, SchemaIcon, SchemaOpenIcon, SequenceIcon } from "./object-icons";
import { qualified, useWorkspace } from "./workspace-context";
import { ExplorerMenu, type MenuTarget } from "./ExplorerMenu";
import { ContextMenu, ContextMenuTrigger } from "@/components/ui/context-menu";

type Load<T> = { status: "loading" } | { status: "ready"; data: T } | { status: "error"; message: string };

const ROW_HEIGHT = 26;
const INDENT = 14;
/** Schemas expanded automatically on first load when there are this many or fewer. */
const AUTO_EXPAND_LIMIT = 5;

type GroupKey = "tables" | "views" | "matviews" | "functions" | "sequences";
const GROUPS: { key: GroupKey; label: string; defaultOpen: boolean }[] = [
  { key: "tables", label: "Tables", defaultOpen: true },
  { key: "views", label: "Views", defaultOpen: true },
  { key: "matviews", label: "Materialized views", defaultOpen: true },
  { key: "functions", label: "Functions", defaultOpen: false },
  { key: "sequences", label: "Sequences", defaultOpen: false },
];

interface Row {
  id: string;
  parentId: string | null;
  depth: number;
  label: string;
  icon?: LucideIcon;
  iconColor?: string;
  detail?: ReactNode;
  title?: string;
  expandable: boolean;
  expanded: boolean;
  tone?: "muted" | "error" | "loading";
  onActivate?: () => void;
  onToggle?: () => void;
  action?: { label: string; icon: LucideIcon; run: () => void };
  /** What the right-click menu acts on; rows without one get the background menu. */
  menu?: MenuTarget;
}

const errorMessage = (e: unknown) => (e instanceof Error ? e.message : "Something went wrong.");

export function SchemaExplorer({ database }: { database: DatabaseInfo | null }) {
  const { profile, openRelation, openQuery, loadRelation } = useWorkspace();
  const connectionId = profile.id;

  const [schemas, setSchemas] = useState<Load<SchemaSummary[]>>({ status: "loading" });
  const [objects, setObjects] = useState<Record<string, Load<SchemaObjects>>>({});
  const [columns, setColumns] = useState<Record<string, Load<ColumnInfo[]>>>({});
  /** Node ids whose open/closed state differs from their default. */
  const [toggled, setToggled] = useState<Set<string>>(new Set());
  const [filter, setFilter] = useState("");
  const [showSystem, setShowSystem] = useState(false);
  const [menuTarget, setMenuTarget] = useState<MenuTarget | null>(null);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [reloadKey, setReloadKey] = useState(0);
  const inflight = useRef(new Set<string>());

  const ensureObjects = useCallback(
    (schema: string, force = false) => {
      if (!force && (objects[schema] || inflight.current.has(`o:${schema}`))) return;
      inflight.current.add(`o:${schema}`);
      setObjects((m) => ({ ...m, [schema]: { status: "loading" } }));
      client.schema
        .listObjects(connectionId, schema)
        .then((data) => setObjects((m) => ({ ...m, [schema]: { status: "ready", data } })))
        .catch((e) => setObjects((m) => ({ ...m, [schema]: { status: "error", message: errorMessage(e) } })))
        .finally(() => inflight.current.delete(`o:${schema}`));
    },
    [connectionId, objects],
  );

  const ensureColumns = useCallback(
    (schema: string, name: string) => {
      const key = `${schema}.${name}`;
      if (columns[key] || inflight.current.has(`c:${key}`)) return;
      inflight.current.add(`c:${key}`);
      setColumns((m) => ({ ...m, [key]: { status: "loading" } }));
      loadRelation(schema, name)
        .then((d) => setColumns((m) => ({ ...m, [key]: { status: "ready", data: d.columns } })))
        .catch((e) => setColumns((m) => ({ ...m, [key]: { status: "error", message: errorMessage(e) } })))
        .finally(() => inflight.current.delete(`c:${key}`));
    },
    [columns, loadRelation],
  );

  /** Re-reads a relation's details (bypassing the cache) and updates its column rows. */
  const refreshRelation = useCallback(
    (schema: string, name: string) => {
      const key = `${schema}.${name}`;
      setColumns((m) => ({ ...m, [key]: { status: "loading" } }));
      loadRelation(schema, name, { refresh: true })
        .then((d) => setColumns((m) => ({ ...m, [key]: { status: "ready", data: d.columns } })))
        .catch((e) => setColumns((m) => ({ ...m, [key]: { status: "error", message: errorMessage(e) } })));
    },
    [loadRelation],
  );

  // Load (or reload) the schema list; small databases open their schemas straight away.
  useEffect(() => {
    let active = true;
    setSchemas({ status: "loading" });
    setObjects({});
    setColumns({});
    client.schema
      .listSchemas(connectionId)
      .then((list) => {
        if (!active) return;
        setSchemas({ status: "ready", data: list });
        const user = list.filter((s) => !s.isSystem);
        if (user.length <= AUTO_EXPAND_LIMIT) setToggled(new Set(user.map((s) => `s:${s.name}`)));
      })
      .catch((e) => active && setSchemas({ status: "error", message: errorMessage(e) }));
    return () => {
      active = false;
    };
  }, [connectionId, reloadKey]);

  const visibleSchemas = useMemo(
    () => (schemas.status === "ready" ? schemas.data.filter((s) => showSystem || !s.isSystem) : []),
    [schemas, showSystem],
  );

  const query = filter.trim().toLowerCase();

  // Expanded schemas need their objects; while filtering, every visible schema does.
  useEffect(() => {
    for (const s of visibleSchemas) {
      if (query || toggled.has(`s:${s.name}`)) ensureObjects(s.name);
    }
  }, [visibleSchemas, toggled, query, ensureObjects]);

  const isOpen = useCallback((id: string, byDefault: boolean) => toggled.has(id) !== byDefault, [toggled]);
  const toggle = useCallback(
    (id: string) =>
      setToggled((s) => {
        const next = new Set(s);
        if (next.has(id)) next.delete(id);
        else next.add(id);
        return next;
      }),
    [],
  );

  const rows = useMemo(() => {
    const out: Row[] = [];
    const matches = (name: string) => !query || name.toLowerCase().includes(query);

    for (const schema of visibleSchemas) {
      const sid = `s:${schema.name}`;
      const loaded = objects[schema.name];
      const data = loaded?.status === "ready" ? loaded.data : null;

      if (query && data) {
        const any =
          data.relations.some((r) => matches(r.name)) ||
          data.routines.some((r) => matches(r.name)) ||
          data.sequences.some((r) => matches(r.name));
        if (!any && !matches(schema.name)) continue;
      }

      const open = query ? true : isOpen(sid, false);
      out.push({
        id: sid,
        parentId: null,
        depth: 0,
        label: schema.name,
        icon: open ? SchemaOpenIcon : SchemaIcon,
        iconColor: schema.isSystem ? "var(--subtle-foreground)" : "var(--primary-text)",
        detail: schema.isSystem ? "system" : data ? String(data.relations.length) : undefined,
        title: schema.comment ?? undefined,
        expandable: true,
        expanded: open,
        onActivate: () => toggle(sid),
        onToggle: () => toggle(sid),
        menu: { kind: "schema", schema, nodeId: sid, expanded: open },
      });
      if (!open) continue;

      if (!loaded || loaded.status === "loading") {
        out.push({
          id: `${sid}:loading`,
          parentId: sid,
          depth: 1,
          label: "Loading…",
          tone: "loading",
          expandable: false,
          expanded: false,
        });
        continue;
      }
      if (loaded.status === "error") {
        out.push({
          id: `${sid}:error`,
          parentId: sid,
          depth: 1,
          label: `Couldn't load: ${loaded.message}`,
          tone: "error",
          expandable: false,
          expanded: false,
          onActivate: () => ensureObjects(schema.name, true),
          action: { label: "Retry", icon: RotateCw, run: () => ensureObjects(schema.name, true) },
        });
        continue;
      }

      const d = loaded.data;
      const byGroup: Record<GroupKey, number> = {
        tables: d.relations.filter(
          (r) => r.kind === "table" || r.kind === "partitioned_table" || r.kind === "foreign_table",
        ).length,
        views: d.relations.filter((r) => r.kind === "view").length,
        matviews: d.relations.filter((r) => r.kind === "materialized_view").length,
        functions: d.routines.length,
        sequences: d.sequences.length,
      };

      if (GROUPS.every((g) => byGroup[g.key] === 0)) {
        out.push({
          id: `${sid}:empty`,
          parentId: sid,
          depth: 1,
          label: "No objects you can see",
          tone: "muted",
          expandable: false,
          expanded: false,
        });
        continue;
      }

      for (const group of GROUPS) {
        if (byGroup[group.key] === 0) continue;
        const gid = `g:${schema.name}:${group.key}`;

        const children: Row[] = [];
        if (group.key === "functions") {
          for (const r of d.routines.filter((r) => matches(r.name))) {
            children.push({
              id: `f:${schema.name}.${r.name}(${r.arguments})`,
              parentId: gid,
              depth: 2,
              label: r.name,
              icon: ROUTINE_ICON[r.kind],
              iconColor: "var(--syntax-function)",
              detail: r.returns ? <span className="font-mono">{shorten(r.returns, 18)}</span> : r.kind,
              title: `${schema.name}.${r.name}(${r.arguments})${r.returns ? ` → ${r.returns}` : ""}`,
              expandable: false,
              expanded: false,
              menu: { kind: "routine", routine: r },
            });
          }
        } else if (group.key === "sequences") {
          for (const s of d.sequences.filter((s) => matches(s.name))) {
            children.push({
              id: `q:${schema.name}.${s.name}`,
              parentId: gid,
              depth: 2,
              label: s.name,
              icon: SequenceIcon,
              iconColor: "var(--muted-foreground)",
              detail: <span className="font-mono">{s.dataType}</span>,
              title: s.ownedBy ? `Owned by ${s.ownedBy}` : undefined,
              expandable: false,
              expanded: false,
              menu: { kind: "sequence", sequence: s },
            });
          }
        } else {
          const kinds =
            group.key === "tables"
              ? ["table", "partitioned_table", "foreign_table"]
              : group.key === "views"
                ? ["view"]
                : ["materialized_view"];
          for (const r of d.relations.filter((r) => kinds.includes(r.kind) && matches(r.name))) {
            const rid = `r:${schema.name}.${r.name}`;
            const key = `${schema.name}.${r.name}`;
            const ropen = isOpen(rid, false);
            const meta = RELATION_META[r.kind];
            children.push({
              id: rid,
              parentId: gid,
              depth: 2,
              label: r.name,
              icon: meta.icon,
              iconColor: meta.color,
              detail: r.estimatedRows != null ? formatCompact(r.estimatedRows) : undefined,
              title: [
                `${meta.label} ${schema.name}.${r.name}`,
                r.estimatedRows != null ? `≈ ${formatCount(r.estimatedRows)} rows (estimate)` : null,
                r.sizeBytes != null ? formatBytes(r.sizeBytes) : null,
                r.comment,
              ]
                .filter(Boolean)
                .join("\n"),
              expandable: true,
              expanded: ropen,
              onActivate: () => openRelation(schema.name, r.name, r.kind),
              onToggle: () => {
                if (!ropen) ensureColumns(schema.name, r.name);
                toggle(rid);
              },
              action: {
                label: "New query",
                icon: SquareTerminal,
                run: () => openQuery(`SELECT *\nFROM ${qualified(schema.name, r.name)}\nLIMIT 100;`, r.name),
              },
              menu: {
                kind: "relation",
                schema: schema.name,
                name: r.name,
                relationKind: r.kind,
                nodeId: rid,
                expanded: ropen,
              },
            });
            if (ropen) {
              children.push(
                ...columnRows(rid, schema.name, r.name, columns[key], () => ensureColumns(schema.name, r.name)),
              );
            }
          }
        }

        if (query && children.length === 0) continue;
        const gopen = query ? true : isOpen(gid, group.defaultOpen);
        out.push({
          id: gid,
          parentId: sid,
          depth: 1,
          label: group.label,
          detail: String(byGroup[group.key]),
          tone: "muted",
          expandable: true,
          expanded: gopen,
          onActivate: () => toggle(gid),
          onToggle: () => toggle(gid),
          menu: { kind: "group", schema: schema.name, label: group.label, nodeId: gid, expanded: gopen },
        });
        if (gopen) out.push(...children);
      }
    }
    return out;
  }, [visibleSchemas, objects, columns, query, isOpen, toggle, ensureObjects, ensureColumns, openRelation, openQuery]);

  /* ---- Keyboard navigation (WAI-ARIA tree pattern) ---- */
  const scrollRef = useRef<HTMLDivElement>(null);
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 20,
  });
  const activeIndex = rows.findIndex((r) => r.id === activeId);

  function focusRow(index: number) {
    const row = rows[index];
    if (!row) return;
    setActiveId(row.id);
    virtualizer.scrollToIndex(index);
  }

  function onKeyDown(e: KeyboardEvent<HTMLDivElement>) {
    const current = rows[activeIndex];
    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        focusRow(activeIndex < 0 ? 0 : Math.min(rows.length - 1, activeIndex + 1));
        break;
      case "ArrowUp":
        e.preventDefault();
        focusRow(Math.max(0, activeIndex - 1));
        break;
      case "Home":
        e.preventDefault();
        focusRow(0);
        break;
      case "End":
        e.preventDefault();
        focusRow(rows.length - 1);
        break;
      case "ArrowRight":
        e.preventDefault();
        if (current?.expandable && !current.expanded) current.onToggle?.();
        else if (current?.expanded) focusRow(activeIndex + 1);
        break;
      case "ArrowLeft":
        e.preventDefault();
        if (current?.expandable && current.expanded) current.onToggle?.();
        else if (current?.parentId) focusRow(rows.findIndex((r) => r.id === current.parentId));
        break;
      case "Enter":
        e.preventDefault();
        current?.onActivate?.();
        break;
    }
  }

  const filterRef = useRef<HTMLInputElement>(null);

  return (
    <div className="flex h-full min-h-0 flex-col bg-surface">
      <div className="flex h-9 shrink-0 items-center justify-between border-b border-border pr-1.5 pl-3">
        <span className="text-[13px] font-semibold text-foreground">Explorer</span>
        <div className="flex items-center">
          <Tooltip content="Refresh">
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label="Refresh explorer"
              onClick={() => setReloadKey((k) => k + 1)}
            >
              <RotateCw />
            </Button>
          </Tooltip>
          <DropdownMenu>
            <Tooltip content="More">
              <DropdownMenuTrigger asChild>
                <Button variant="ghost" size="icon-sm" aria-label="Explorer options">
                  <MoreHorizontal />
                </Button>
              </DropdownMenuTrigger>
            </Tooltip>
            <DropdownMenuContent align="end">
              <DropdownMenuItem onSelect={() => setShowSystem((v) => !v)}>
                {showSystem ? "Hide system schemas" : "Show system schemas"}
              </DropdownMenuItem>
              <DropdownMenuItem onSelect={() => setToggled(new Set())}>Collapse all</DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </div>

      {database && (
        <div
          className="flex shrink-0 items-center gap-2 border-b border-border px-3 py-2"
          title={database.serverVersion}
        >
          <Database className="size-3.5 shrink-0 text-primary-text" />
          <span className="min-w-0 truncate text-[13px] font-medium">{database.name}</span>
          {database.sizeBytes != null && (
            <span className="tabular ml-auto shrink-0 text-[11px] text-subtle-foreground">
              {formatBytes(database.sizeBytes)}
            </span>
          )}
        </div>
      )}

      <div className="relative shrink-0 border-b border-border p-2">
        <Search className="pointer-events-none absolute top-1/2 left-4 size-3.5 -translate-y-1/2 text-subtle-foreground" />
        <input
          ref={filterRef}
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") setFilter("");
            if (e.key === "ArrowDown") {
              e.preventDefault();
              scrollRef.current?.focus();
              focusRow(0);
            }
          }}
          placeholder="Filter tables, views, functions"
          aria-label="Filter database objects"
          spellCheck={false}
          className="h-7 w-full border border-border bg-surface-2 pr-7 pl-7 text-xs outline-none placeholder:text-subtle-foreground focus-visible:border-primary"
        />
        {filter && (
          <Tooltip content="Clear filter" shortcut="Esc">
            <button
              type="button"
              aria-label="Clear filter"
              onClick={() => {
                setFilter("");
                filterRef.current?.focus();
              }}
              className="absolute top-1/2 right-3.5 -translate-y-1/2 text-subtle-foreground hover:text-foreground"
            >
              <X className="size-3.5" />
            </button>
          </Tooltip>
        )}
      </div>

      {schemas.status === "loading" && (
        <div className="flex items-center gap-2 px-3 py-3 text-xs text-muted-foreground">
          <Loader2 className="size-3.5 animate-spin" /> Loading schemas…
        </div>
      )}
      {schemas.status === "error" && (
        <div className="m-3 flex flex-col items-start gap-2 border border-border bg-danger-soft p-3 text-xs">
          <span className="flex items-center gap-1.5 font-medium text-foreground">
            <AlertCircle className="size-3.5 text-danger" /> Couldn't load schemas
          </span>
          <span className="text-muted-foreground">{schemas.message}</span>
          <Button size="sm" onClick={() => setReloadKey((k) => k + 1)}>
            <RotateCw /> Try again
          </Button>
        </div>
      )}
      {schemas.status === "ready" && rows.length === 0 && (
        <p className="px-3 py-4 text-xs text-subtle-foreground">
          {query ? `Nothing matches "${filter.trim()}".` : "This database has no schemas you can see."}
        </p>
      )}

      <ContextMenu>
        <ContextMenuTrigger asChild>
          <div
            ref={scrollRef}
            role="tree"
            // Reset first; the row's own handler (bubbling, after this capture) sets its target.
            onContextMenuCapture={() => setMenuTarget(null)}
            aria-label="Database objects"
            tabIndex={0}
            aria-activedescendant={activeId ? domId(activeId) : undefined}
            onKeyDown={onKeyDown}
            onFocus={() => activeIndex < 0 && rows.length > 0 && setActiveId(rows[0]!.id)}
            className="min-h-0 flex-1 overflow-y-auto py-1 outline-none"
          >
            <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
              {virtualizer.getVirtualItems().map((v) => {
                const row = rows[v.index]!;
                return (
                  <TreeRow
                    key={row.id}
                    row={row}
                    active={row.id === activeId}
                    style={{ transform: `translateY(${v.start}px)`, height: ROW_HEIGHT }}
                    onClick={() => {
                      setActiveId(row.id);
                      row.onActivate?.();
                    }}
                    onContextMenu={() => {
                      setActiveId(row.id);
                      setMenuTarget(row.menu ?? null);
                    }}
                  />
                );
              })}
            </div>
          </div>
        </ContextMenuTrigger>
        <ExplorerMenu
          target={menuTarget}
          showSystem={showSystem}
          onToggle={(nodeId) => rows.find((r) => r.id === nodeId)?.onToggle?.()}
          onRefreshSchema={(schema) => ensureObjects(schema, true)}
          onRefreshRelation={refreshRelation}
          onRefreshAll={() => setReloadKey((k) => k + 1)}
          onCollapseAll={() => setToggled(new Set())}
          onToggleSystem={() => setShowSystem((v) => !v)}
        />
      </ContextMenu>
    </div>
  );
}

function columnRows(
  parentId: string,
  schema: string,
  table: string,
  state: Load<ColumnInfo[]> | undefined,
  retry: () => void,
): Row[] {
  const key = `${schema}.${table}`;
  if (!state || state.status === "loading") {
    return [
      {
        id: `${parentId}:loading`,
        parentId,
        depth: 3,
        label: "Loading columns…",
        tone: "loading",
        expandable: false,
        expanded: false,
      },
    ];
  }
  if (state.status === "error") {
    return [
      {
        id: `${parentId}:error`,
        parentId,
        depth: 3,
        label: `Couldn't load columns`,
        title: state.message,
        tone: "error",
        expandable: false,
        expanded: false,
        onActivate: retry,
      },
    ];
  }
  return state.data.map((c) => ({
    id: `c:${key}.${c.name}`,
    parentId,
    depth: 3,
    label: c.name,
    icon: c.isPrimaryKey ? KeyRound : c.foreignKey ? Link2 : undefined,
    iconColor: c.isPrimaryKey ? "var(--primary-text)" : "var(--syntax-type)",
    detail: <span className="font-mono">{shorten(c.dataType, 22)}</span>,
    title: [
      `${c.name} ${c.dataType}${c.nullable ? "" : " NOT NULL"}`,
      c.isPrimaryKey ? "Primary key" : null,
      c.foreignKey ? `References ${c.foreignKey.schema}.${c.foreignKey.table}(${c.foreignKey.column})` : null,
      c.defaultValue ? `Default: ${c.defaultValue}` : null,
      c.comment,
    ]
      .filter(Boolean)
      .join("\n"),
    expandable: false,
    expanded: false,
    menu: { kind: "column", schema, table, column: c },
  }));
}

const shorten = (s: string, max: number) => (s.length > max ? `${s.slice(0, max - 1)}…` : s);
const domId = (id: string) => `tree-${id.replace(/[^a-zA-Z0-9_-]/g, "_")}`;

function TreeRow({
  row,
  active,
  style,
  onClick,
  onContextMenu,
}: {
  row: Row;
  active: boolean;
  style: CSSProperties;
  onClick: () => void;
  onContextMenu: () => void;
}) {
  const Icon = row.icon;
  return (
    <div
      id={domId(row.id)}
      role="treeitem"
      aria-level={row.depth + 1}
      aria-expanded={row.expandable ? row.expanded : undefined}
      aria-selected={active}
      title={row.title}
      onClick={onClick}
      onContextMenu={onContextMenu}
      className={cn(
        "group absolute inset-x-0 flex cursor-pointer items-center gap-1.5 pr-2 text-[12.5px] select-none",
        "hover:bg-surface-2",
        active && "bg-surface-2 shadow-[inset_2px_0_0_var(--primary)]",
      )}
      style={{ ...style, paddingLeft: 8 + row.depth * INDENT }}
    >
      <span className="flex size-4 shrink-0 items-center justify-center">
        {row.expandable && (
          <button
            type="button"
            tabIndex={-1}
            aria-label={row.expanded ? "Collapse" : "Expand"}
            onClick={(e) => {
              e.stopPropagation();
              row.onToggle?.();
            }}
            className="flex size-4 items-center justify-center text-subtle-foreground hover:text-foreground"
          >
            <ChevronRight className={cn("size-3.5 transition-transform", row.expanded && "rotate-90")} />
          </button>
        )}
      </span>
      {row.tone === "loading" ? (
        <Loader2 className="size-3.5 shrink-0 animate-spin text-subtle-foreground" />
      ) : row.tone === "error" ? (
        <AlertCircle className="size-3.5 shrink-0 text-danger" />
      ) : Icon ? (
        <Icon className="size-3.5 shrink-0" style={{ color: row.iconColor }} />
      ) : row.depth === 3 ? (
        <span className="flex size-3.5 shrink-0 items-center justify-center">
          <span className="size-1 bg-subtle-foreground" />
        </span>
      ) : null}
      <span
        className={cn(
          "min-w-0 flex-1 truncate",
          row.tone === "muted" && "text-[11.5px] font-medium text-muted-foreground",
          row.tone === "loading" && "text-subtle-foreground",
          row.tone === "error" && "text-danger",
        )}
      >
        {row.label}
      </span>
      {row.detail != null && (
        <span className={cn("tabular shrink-0 text-[11px] text-subtle-foreground", row.action && "group-hover:hidden")}>
          {row.detail}
        </span>
      )}
      {row.action && (
        <Tooltip content={row.action.label}>
          <button
            type="button"
            tabIndex={-1}
            aria-label={`${row.action.label}: ${row.label}`}
            onClick={(e) => {
              e.stopPropagation();
              row.action!.run();
            }}
            className="hidden size-5 shrink-0 items-center justify-center text-muted-foreground group-hover:flex hover:bg-surface hover:text-primary-text"
          >
            <row.action.icon className="size-3.5" />
          </button>
        </Tooltip>
      )}
    </div>
  );
}
