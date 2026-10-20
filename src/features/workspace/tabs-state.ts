import type { RelationKind } from "@/services/client";

export type RelationView = "columns" | "data" | "indexes" | "constraints" | "ddl";

export interface QueryTab {
  id: string;
  kind: "query";
  title: string;
  sql: string;
  /** SQL the tab was opened with; edits beyond this count as unsaved work. */
  initialSql: string;
  /** Run the SQL as soon as the tab opens (e.g. "View data" from the explorer). */
  runOnOpen?: boolean;
}

export interface RelationTab {
  id: string;
  kind: "relation";
  schema: string;
  name: string;
  relationKind: RelationKind;
  view: RelationView;
}

export type Tab = QueryTab | RelationTab;

export interface TabsState {
  tabs: Tab[];
  activeId: string | null;
  /** Monotonic counter for "Query N" titles; never reused within a session. */
  queryCounter: number;
}

export type TabsAction =
  | { type: "open_query"; sql?: string; title?: string; run?: boolean }
  | { type: "open_relation"; schema: string; name: string; relationKind: RelationKind; view?: RelationView }
  | { type: "activate"; id: string }
  | { type: "close"; id: string }
  | { type: "close_many"; ids: string[] }
  | { type: "duplicate"; id: string }
  | { type: "update_sql"; id: string; sql: string }
  | { type: "set_view"; id: string; view: RelationView }
  | { type: "cycle"; direction: 1 | -1 };

export const initialTabsState: TabsState = {
  tabs: [{ id: "query-1", kind: "query", title: "Query 1", sql: "", initialSql: "" }],
  activeId: "query-1",
  queryCounter: 1,
};

export const relationTabId = (schema: string, name: string) => `rel:${schema}.${name}`;

/** True when closing the tab would throw away SQL the user typed. */
export const hasUnsavedWork = (tab: Tab) => tab.kind === "query" && tab.sql.trim() !== "" && tab.sql !== tab.initialSql;

export function tabsReducer(state: TabsState, action: TabsAction): TabsState {
  switch (action.type) {
    case "open_query": {
      const n = state.queryCounter + 1;
      const id = `query-${n}`;
      const sql = action.sql ?? "";
      const tab: QueryTab = {
        id,
        kind: "query",
        title: action.title ?? `Query ${n}`,
        sql,
        initialSql: sql,
        runOnOpen: action.run,
      };
      return { tabs: [...state.tabs, tab], activeId: id, queryCounter: n };
    }

    case "open_relation": {
      const id = relationTabId(action.schema, action.name);
      const existing = state.tabs.find((t) => t.id === id);
      if (existing) {
        // Re-opening focuses the tab; an explicit view (e.g. "data") switches to it.
        const tabs = action.view
          ? state.tabs.map((t) => (t.id === id && t.kind === "relation" ? { ...t, view: action.view! } : t))
          : state.tabs;
        return { ...state, tabs, activeId: id };
      }
      const tab: RelationTab = {
        id,
        kind: "relation",
        schema: action.schema,
        name: action.name,
        relationKind: action.relationKind,
        view: action.view ?? "columns",
      };
      // Open next to the active tab, like a browser.
      const at = state.tabs.findIndex((t) => t.id === state.activeId);
      const tabs = [...state.tabs];
      tabs.splice(at < 0 ? tabs.length : at + 1, 0, tab);
      return { ...state, tabs, activeId: id };
    }

    case "activate":
      return state.tabs.some((t) => t.id === action.id) ? { ...state, activeId: action.id } : state;

    case "close": {
      const index = state.tabs.findIndex((t) => t.id === action.id);
      if (index < 0) return state;
      const tabs = state.tabs.filter((t) => t.id !== action.id);
      let activeId = state.activeId;
      if (state.activeId === action.id) activeId = (tabs[index] ?? tabs[index - 1])?.id ?? null;
      return { ...state, tabs, activeId };
    }

    case "close_many": {
      const closing = new Set(action.ids);
      const tabs = state.tabs.filter((t) => !closing.has(t.id));
      if (state.activeId === null || !closing.has(state.activeId)) return { ...state, tabs };
      // The active tab closed: activate the nearest survivor to its left, else the first one.
      const index = state.tabs.findIndex((t) => t.id === state.activeId);
      const left = state.tabs
        .slice(0, index)
        .reverse()
        .find((t) => !closing.has(t.id));
      return { ...state, tabs, activeId: left?.id ?? tabs[0]?.id ?? null };
    }

    case "duplicate": {
      const source = state.tabs.find((t) => t.id === action.id);
      if (source?.kind !== "query") return state;
      const n = state.queryCounter + 1;
      const id = `query-${n}`;
      const copy: QueryTab = { id, kind: "query", title: `${source.title} (copy)`, sql: source.sql, initialSql: "" };
      const tabs = [...state.tabs];
      tabs.splice(tabs.indexOf(source) + 1, 0, copy);
      return { tabs, activeId: id, queryCounter: n };
    }

    case "update_sql":
      return {
        ...state,
        tabs: state.tabs.map((t) => (t.id === action.id && t.kind === "query" ? { ...t, sql: action.sql } : t)),
      };

    case "set_view":
      return {
        ...state,
        tabs: state.tabs.map((t) => (t.id === action.id && t.kind === "relation" ? { ...t, view: action.view } : t)),
      };

    case "cycle": {
      if (state.tabs.length < 2) return state;
      const index = state.tabs.findIndex((t) => t.id === state.activeId);
      const next = (index + action.direction + state.tabs.length) % state.tabs.length;
      return { ...state, activeId: state.tabs[next]?.id ?? state.activeId };
    }
  }
}
