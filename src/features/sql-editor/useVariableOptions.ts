import { useCallback, useEffect, useState } from "react";
import { client, type VariableDef, type VariableOption, type VariableOptionsOutcome } from "@/services/client";

export type OptionsState =
  | { status: "idle" | "loading"; options: VariableOption[] }
  | { status: "ready"; options: VariableOption[]; truncated: boolean; column?: string }
  | { status: "error"; options: VariableOption[]; message: string };

/** Results per request key, so every control asking the same thing loads once. */
const cache = new Map<string, Promise<OptionsState>>();

function toState(outcome: VariableOptionsOutcome): OptionsState {
  return outcome.ok
    ? { status: "ready", options: outcome.options, truncated: outcome.truncated, column: outcome.column ?? undefined }
    : { status: "error", options: [], message: outcome.hint ? `${outcome.message} ${outcome.hint}` : outcome.message };
}

function load(key: string, request: () => Promise<VariableOptionsOutcome>, refresh: boolean): Promise<OptionsState> {
  const cached = cache.get(key);
  if (cached && !refresh) return cached;
  const pending = request()
    .then(toState)
    .catch((e: unknown) => ({
      status: "error" as const,
      options: [],
      message: e instanceof Error ? e.message : "The values could not be loaded.",
    }));
  cache.set(key, pending);
  // A failure is not kept, so the next attempt really retries.
  void pending.then((s) => s.status === "error" && cache.delete(key));
  return pending;
}

/**
 * Choices for an enum or multi-select variable:
 * - a fixed list, as written by the author;
 * - a lookup query's rows (debounced while the query is edited);
 * - for a Multi-select without a list, the distinct values of the column it is compared with
 *   in `sql`, loaded only while `active` (the dropdown is open).
 */
export function useVariableOptions(connectionId: string, def: VariableDef, sql = "", active = true) {
  const source = def.options;
  const lookupSql = source?.source === "query" ? source.sql.trim() : "";
  const fromColumn = def.type === "multi_enum" && source == null;

  const key = lookupSql
    ? `${connectionId}\u0000q\u0000${lookupSql}`
    : fromColumn && sql.trim()
      ? `${connectionId}\u0000c\u0000${def.name}\u0000${sql}`
      : "";
  const request = useCallback(
    (): Promise<VariableOptionsOutcome> =>
      lookupSql
        ? client.sql.variableOptions(connectionId, lookupSql)
        : client.sql.columnValues(connectionId, sql, def.name),
    [connectionId, lookupSql, sql, def.name],
  );
  const [state, setState] = useState<OptionsState>({ status: "idle", options: [] });

  const fetch = useCallback(
    (refresh: boolean) => {
      if (!key) return () => undefined;
      let live = true;
      setState((s) => ({ status: "loading", options: s.options }));
      void load(key, request, refresh).then((s) => live && setState(s));
      return () => {
        live = false;
      };
    },
    [key, request],
  );

  // Lookup queries load (debounced) as they are edited; column values when the dropdown opens.
  const enabled = fromColumn ? active : true;
  useEffect(() => {
    if (!enabled) return;
    let cancel: (() => void) | undefined;
    const timer = setTimeout(
      () => {
        cancel = fetch(false);
      },
      lookupSql ? 500 : 0,
    );
    return () => {
      clearTimeout(timer);
      cancel?.();
    };
  }, [fetch, enabled, lookupSql]);

  if (source?.source === "list") {
    return {
      state: { status: "ready", options: source.values, truncated: false } as OptionsState,
      reload: () => undefined,
    };
  }
  if (!key) return { state: { status: "idle", options: [] } as OptionsState, reload: () => undefined };
  return { state, reload: () => void fetch(true) };
}

export const optionLabel = (o: VariableOption) => o.label || o.value;
