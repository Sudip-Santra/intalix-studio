/**
 * The real client: every call goes to the Rust backend through a Tauri command.
 *
 * Cancellation: long-running calls get a request id. When the caller's AbortSignal fires we
 * send `request_cancel`, which aborts the work in Rust (and sends PostgreSQL's own cancel
 * request for running queries). The promise then rejects with an AbortError, exactly like the
 * simulated client, so the screens don't need to know which client they use.
 */
import { invoke } from "@tauri-apps/api/core";
import type { ErrorCategory, IntalixClient } from "./types";

/** Error raised for backend failures, carrying the stable category and an optional hint. */
export class ClientError extends Error {
  constructor(
    readonly category: ErrorCategory,
    message: string,
    readonly hint: string | null,
  ) {
    super(message);
    this.name = "ClientError";
  }
}

interface BackendError {
  category: ErrorCategory;
  message: string;
  hint: string | null;
}

const isBackendError = (e: unknown): e is BackendError =>
  typeof e === "object" && e !== null && "category" in e && "message" in e;

const abortError = () => new DOMException("Aborted", "AbortError");

function toError(e: unknown): Error {
  if (isBackendError(e)) return new ClientError(e.category, e.message, e.hint ?? null);
  if (e instanceof Error) return e;
  return new ClientError("internal", typeof e === "string" ? e : "Something went wrong.", null);
}

/** Plain command call; backend errors become `ClientError`s. */
async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (e) {
    throw toError(e);
  }
}

/** Command call that can be cancelled through an AbortSignal. */
async function cancellable<T>(command: string, args: Record<string, unknown>, signal?: AbortSignal): Promise<T> {
  if (signal?.aborted) throw abortError();
  const requestId = crypto.randomUUID();
  const onAbort = () => {
    invoke("request_cancel", { requestId }).catch(() => {
      // Nothing to do: the request may already have finished.
    });
  };
  signal?.addEventListener("abort", onAbort, { once: true });
  try {
    const result = await invoke<T>(command, { ...args, requestId });
    if (signal?.aborted) throw abortError();
    return result;
  } catch (e) {
    if (signal?.aborted || (isBackendError(e) && e.category === "cancelled")) throw abortError();
    throw toError(e);
  } finally {
    signal?.removeEventListener("abort", onAbort);
  }
}

export const tauriClient: IntalixClient = {
  simulated: false,

  connections: {
    list: () => call("connections_list"),
    test: (draft, signal, existingId) =>
      cancellable("connections_test", { draft, existingId: existingId ?? null }, signal),
    save: (draft, signal, id) => cancellable("connections_save", { draft, id: id ?? null }, signal),
    remove: (id) => call("connections_delete", { id }),
    disconnect: (id) => call("connections_disconnect", { id }),
  },

  schema: {
    database: (connectionId) => call("schema_database", { connectionId }),
    listSchemas: (connectionId) => call("schema_list", { connectionId }),
    listObjects: (connectionId, schema) => call("schema_objects", { connectionId, schema }),
    getRelation: (connectionId, schema, name) => call("schema_relation", { connectionId, schema, name }),
    completionCatalog: (connectionId) => call("schema_completion", { connectionId }),
  },

  query: {
    execute: (connectionId, sql, { rowLimit }, signal) =>
      cancellable("query_execute", { connectionId, sql, rowLimit }, signal),
    preview: (connectionId, schema, name, { rowLimit }, signal) =>
      cancellable("query_preview", { connectionId, schema, name, rowLimit }, signal),
  },
};
