import { createContext, useContext } from "react";
import type { CompletionCatalog, ConnectionProfile, RelationDetail, RelationKind } from "@/services/client";
import type { RelationView } from "./tabs-state";

export interface WorkspaceApi {
  profile: ConnectionProfile;
  catalog: CompletionCatalog | undefined;
  openRelation(schema: string, name: string, relationKind: RelationKind, view?: RelationView): void;
  openQuery(sql?: string, title?: string, options?: { run?: boolean }): void;
  /** Cached per workspace so the explorer and tabs share one request per relation. */
  loadRelation(schema: string, name: string, options?: { refresh?: boolean }): Promise<RelationDetail>;
}

export const WorkspaceContext = createContext<WorkspaceApi | null>(null);

export function useWorkspace(): WorkspaceApi {
  const api = useContext(WorkspaceContext);
  if (!api) throw new Error("useWorkspace must be used inside <WorkspacePage>");
  return api;
}

/** Double-quotes an identifier only when PostgreSQL requires it. */
export function quoteIdent(name: string): string {
  return /^[a-z_][a-z0-9_$]*$/.test(name) ? name : `"${name.replace(/"/g, '""')}"`;
}

export const qualified = (schema: string, name: string) => `${quoteIdent(schema)}.${quoteIdent(name)}`;
