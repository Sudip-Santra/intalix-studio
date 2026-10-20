import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from "react";
import { Group, Panel, Separator } from "react-resizable-panels";
import { BarChart3, Database, History, Lock, Plus } from "lucide-react";
import {
  client,
  type CompletionCatalog,
  type ConnectionProfile,
  type DatabaseInfo,
  type RelationDetail,
} from "@/services/client";
import { Button } from "@/components/ui/button";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { Tooltip } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import { SchemaExplorer } from "./SchemaExplorer";
import { ConnectionMenu } from "./ConnectionMenu";
import { ColorThemeButton, ThemeToggle } from "@/app/AppChrome";
import { TabBar } from "./TabBar";
import { QueryTab } from "./QueryTab";
import { RelationTab } from "./RelationTab";
import { hasUnsavedWork, initialTabsState, tabsReducer } from "./tabs-state";
import { WorkspaceContext, type WorkspaceApi } from "./workspace-context";
import { SEPARATOR_CLASS } from "./styles";

type WorkspacePageProps = {
  profile: ConnectionProfile;
  onSwitch: (profile: ConnectionProfile) => void;
  onManageConnections: () => void;
  onDisconnect: () => void;
};

export function WorkspacePage({ profile, onSwitch, onManageConnections, onDisconnect }: WorkspacePageProps) {
  const [state, dispatch] = useReducer(tabsReducer, initialTabsState);
  const [database, setDatabase] = useState<DatabaseInfo | null>(null);
  const [catalog, setCatalog] = useState<CompletionCatalog>();
  /** Tabs waiting for "discard unsaved work?" confirmation before closing. */
  const [pendingClose, setPendingClose] = useState<string[] | null>(null);
  const relationCache = useRef(new Map<string, Promise<RelationDetail>>());

  useEffect(() => {
    let active = true;
    relationCache.current.clear();
    client.schema
      .database(profile.id)
      .then((d) => active && setDatabase(d))
      .catch(() => active && setDatabase(null));
    // Autocomplete is a convenience: if it fails the editor still works without it.
    client.schema
      .completionCatalog(profile.id)
      .then((c) => active && setCatalog(c))
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, [profile.id]);

  const loadRelation = useCallback<WorkspaceApi["loadRelation"]>(
    (schema, name, options) => {
      const key = `${schema}.${name}`;
      const cached = relationCache.current.get(key);
      if (cached && !options?.refresh) return cached;
      const request = client.schema.getRelation(profile.id, schema, name);
      relationCache.current.set(key, request);
      // Failures are not cached, so a retry really retries.
      request.catch(() => relationCache.current.delete(key));
      return request;
    },
    [profile.id],
  );

  const api = useMemo<WorkspaceApi>(
    () => ({
      profile,
      catalog,
      loadRelation,
      openRelation: (schema, name, relationKind, view) =>
        dispatch({ type: "open_relation", schema, name, relationKind, view }),
      openQuery: (sql, title, options) => dispatch({ type: "open_query", sql, title, run: options?.run }),
    }),
    [profile, catalog, loadRelation],
  );

  const requestCloseMany = useCallback(
    (ids: string[]) => {
      if (ids.length === 0) return;
      const dirty = state.tabs.some((t) => ids.includes(t.id) && hasUnsavedWork(t));
      if (dirty) setPendingClose(ids);
      else dispatch({ type: "close_many", ids });
    },
    [state.tabs],
  );
  const requestClose = useCallback((id: string) => requestCloseMany([id]), [requestCloseMany]);

  // App-wide tab shortcuts.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey)) return;
      const key = e.key.toLowerCase();
      if (key === "t" && !e.shiftKey) {
        e.preventDefault();
        dispatch({ type: "open_query" });
      } else if (key === "w" && state.activeId) {
        e.preventDefault();
        requestClose(state.activeId);
      } else if (e.key === "Tab") {
        e.preventDefault();
        dispatch({ type: "cycle", direction: e.shiftKey ? -1 : 1 });
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [state.activeId, requestClose]);

  const pendingDirty = state.tabs.filter((t) => pendingClose?.includes(t.id) && hasUnsavedWork(t));
  const pendingSingle = pendingClose?.length === 1 ? state.tabs.find((t) => t.id === pendingClose[0]) : undefined;

  return (
    <WorkspaceContext.Provider value={api}>
      <div className="flex h-full min-h-0 flex-col">
        <div className="flex min-h-0 flex-1">
          <IconRail />
          <Group orientation="horizontal" className="min-w-0 flex-1">
            <Panel defaultSize={300} minSize={220} maxSize="45%">
              <SchemaExplorer database={database} />
            </Panel>
            <Separator className={cn("w-px", SEPARATOR_CLASS)} />
            <Panel minSize={420}>
              <div className="flex h-full min-h-0 flex-col bg-background">
                <TabBar
                  tabs={state.tabs}
                  activeId={state.activeId}
                  onActivate={(id) => dispatch({ type: "activate", id })}
                  onClose={requestClose}
                  onCloseMany={requestCloseMany}
                  onDuplicate={(id) => dispatch({ type: "duplicate", id })}
                  onNewQuery={() => dispatch({ type: "open_query" })}
                />
                <div className="relative min-h-0 flex-1">
                  {/* Every tab stays mounted (hidden when inactive) so editors and results survive switching. */}
                  {state.tabs.map((tab) => {
                    const active = tab.id === state.activeId;
                    return (
                      <div key={tab.id} role="tabpanel" hidden={!active} className="absolute inset-0">
                        {tab.kind === "query" ? (
                          <QueryTab
                            tab={tab}
                            active={active}
                            onSqlChange={(sql) => dispatch({ type: "update_sql", id: tab.id, sql })}
                          />
                        ) : (
                          <RelationTab
                            tab={tab}
                            onViewChange={(view) => dispatch({ type: "set_view", id: tab.id, view })}
                          />
                        )}
                      </div>
                    );
                  })}
                  {state.tabs.length === 0 && <NoTabs onNewQuery={() => dispatch({ type: "open_query" })} />}
                </div>
              </div>
            </Panel>
          </Group>
        </div>
        <StatusBar
          profile={profile}
          database={database}
          simulated={client.simulated}
          onSwitch={onSwitch}
          onManage={onManageConnections}
          onDisconnect={onDisconnect}
        />
      </div>

      <ConfirmDialog
        open={pendingClose !== null}
        title={
          pendingSingle?.kind === "query" ? `Close ${pendingSingle.title}?` : `Close ${pendingClose?.length ?? 0} tabs?`
        }
        description={
          pendingDirty.length === 1
            ? `"${pendingDirty[0]?.kind === "query" ? pendingDirty[0].title : ""}" has a query that hasn't been saved. Closing discards it.`
            : `${pendingDirty.length} tabs have queries that haven't been saved. Closing discards them.`
        }
        confirmLabel="Discard and close"
        tone="danger"
        onConfirm={() => {
          if (pendingClose) dispatch({ type: "close_many", ids: pendingClose });
          setPendingClose(null);
        }}
        onCancel={() => setPendingClose(null)}
      />
    </WorkspaceContext.Provider>
  );
}

function IconRail() {
  const items = [
    { icon: Database, label: "Database explorer", active: true },
    { icon: BarChart3, label: "Reports (coming soon)", active: false },
    { icon: History, label: "Run history (coming soon)", active: false },
  ];
  return (
    <nav
      aria-label="Sections"
      className="flex w-11 shrink-0 flex-col items-center border-r border-border bg-surface py-1.5"
    >
      {items.map(({ icon: Icon, label, active }) => (
        <Tooltip key={label} content={label} side="right">
          <button
            type="button"
            aria-label={label}
            aria-current={active ? "page" : undefined}
            aria-disabled={!active}
            className={cn(
              "relative flex h-10 w-11 items-center justify-center outline-none focus-visible:shadow-[inset_0_0_0_1px_var(--primary)]",
              active ? "text-foreground" : "cursor-default text-subtle-foreground hover:text-muted-foreground",
            )}
          >
            {active && <span className="absolute inset-y-2 left-0 w-0.5 bg-primary" />}
            <Icon className="size-[18px]" strokeWidth={1.75} />
          </button>
        </Tooltip>
      ))}
    </nav>
  );
}

function NoTabs({ onNewQuery }: { onNewQuery: () => void }) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-4 text-center">
      <div>
        <p className="text-[15px] font-semibold">Nothing open</p>
        <p className="mt-1 text-xs text-muted-foreground">
          Open a table from the explorer to inspect it, or start a new query.
        </p>
      </div>
      <Button variant="primary" onClick={onNewQuery}>
        <Plus /> New query
        <kbd className="ml-1 font-mono text-[10.5px] opacity-70">Ctrl+T</kbd>
      </Button>
    </div>
  );
}

function StatusBar({
  profile,
  database,
  simulated,
  onSwitch,
  onManage,
  onDisconnect,
}: {
  profile: ConnectionProfile;
  database: DatabaseInfo | null;
  simulated: boolean;
  onSwitch: (profile: ConnectionProfile) => void;
  onManage: () => void;
  onDisconnect: () => void;
}) {
  return (
    <footer className="flex h-6 shrink-0 items-center gap-4 border-t border-border bg-surface pr-3 text-[11px] text-muted-foreground">
      <ConnectionMenu current={profile} onSwitch={onSwitch} onManage={onManage} onDisconnect={onDisconnect} />
      {database && <span className="hidden lg:inline">{database.serverVersion}</span>}
      {/* Grouped so the footer's wide gap doesn't separate these three. */}
      <span className="ml-auto flex h-full items-center gap-1">
        <span className="mr-1 flex items-center gap-1">
          <Lock className="size-3" /> Read-only
        </span>
        <ColorThemeButton compact />
        <ThemeToggle compact />
      </span>
      {simulated && (
        <span className="flex items-center gap-1.5">
          <span className="size-1.5 bg-warning" /> Simulated backend
        </span>
      )}
    </footer>
  );
}
