/**
 * Connects the CodeMirror editor to the backend's PostgreSQL language service
 * (src-tauri/src/lang): completions, live diagnostics, signature help and `{{variable}}`
 * highlighting.
 *
 * Diagnostics come in two layers:
 * 1. Instant offline checks (brackets, quotes, variables, read-only policy) on every pause.
 * 2. When those pass, PostgreSQL parses and analyses the query without running it, which
 *    catches every syntax error, unknown table or column and type mismatch exactly.
 */
import {
  autocompletion,
  snippet,
  type Completion,
  type CompletionContext,
  type CompletionResult,
} from "@codemirror/autocomplete";
import { linter, lintGutter, setDiagnostics, type Diagnostic } from "@codemirror/lint";
import { StateEffect, StateField, type EditorState, type Extension } from "@codemirror/state";
import {
  Decoration,
  EditorView,
  MatchDecorator,
  ViewPlugin,
  showTooltip,
  type DecorationSet,
  type Tooltip,
  type ViewUpdate,
} from "@codemirror/view";
import {
  client,
  type SqlAnalysis,
  type SqlCheckOutcome,
  type SqlCompletionItem,
  type SqlSignatureHelp,
  type VariableDef,
} from "@/services/client";

/** What the server check last reported, for a status indicator next to the editor. */
export type CheckState =
  | { status: "idle" }
  | { status: "checking" }
  /** Offline checks found errors (or several statements), so the server was not asked. */
  | { status: "skipped" }
  | { status: "done"; outcome: SqlCheckOutcome };

export interface LanguageServiceOptions {
  connectionId: string;
  /** Current variable definitions, or null where variables are not available. Read per request. */
  getVariables: () => VariableDef[] | null;
  onAnalysis?: (analysis: SqlAnalysis) => void;
  onCheck?: (state: CheckState) => void;
}

const CM_TYPES: Record<SqlCompletionItem["kind"], string> = {
  keyword: "keyword",
  function: "function",
  table: "class",
  view: "interface",
  column: "property",
  schema: "namespace",
  type: "type",
  variable: "variable",
  alias: "constant",
};

function toCompletion(item: SqlCompletionItem): Completion {
  return {
    label: item.label,
    type: CM_TYPES[item.kind],
    detail: item.detail ?? undefined,
    info: item.info ?? undefined,
    boost: Math.max(-99, Math.min(99, item.boost)),
    apply: item.apply == null ? undefined : item.snippet ? snippet(item.apply) : item.apply,
  };
}

/** 1-based character (code point) offset → UTF-16 offset, as PostgreSQL and the editor count. */
export function charToUtf16(text: string, position: number): number {
  let chars = 0;
  let i = 0;
  while (i < text.length && chars < position - 1) {
    const code = text.charCodeAt(i);
    i += code >= 0xd800 && code <= 0xdbff ? 2 : 1;
    chars++;
  }
  return i;
}

/** End of the word starting at `from`, so a server error underlines a whole token. */
function wordEnd(text: string, from: number): number {
  const match = /^("(?:[^"]|"")*"|[\w$.{}]+|\S)/.exec(text.slice(from));
  return from + (match?.[0].length ?? 1);
}

function serverDiagnostic(sql: string, outcome: Extract<SqlCheckOutcome, { ok: false }>): Diagnostic {
  const from = outcome.position != null ? Math.min(charToUtf16(sql, outcome.position), sql.length) : 0;
  const to = outcome.position != null ? Math.min(wordEnd(sql, from), sql.length) : Math.min(sql.length, 1);
  return {
    from,
    to: Math.max(to, from),
    severity: "error",
    source: "PostgreSQL",
    message: outcome.hint ? `${outcome.message}\n${outcome.hint}` : outcome.message,
  };
}

/* ------------------------------------------------------------------------------------------
 * Signature help tooltip
 * ---------------------------------------------------------------------------------------- */

const setSignature = StateEffect.define<{ help: SqlSignatureHelp; pos: number } | null>();

const signatureField = StateField.define<Tooltip | null>({
  create: () => null,
  update(value, tr) {
    for (const e of tr.effects) {
      if (e.is(setSignature)) {
        const next = e.value;
        return next ? { pos: next.pos, above: true, create: () => ({ dom: signatureDom(next.help) }) } : null;
      }
    }
    return tr.docChanged && value ? { ...value, pos: tr.changes.mapPos(value.pos) } : value;
  },
  provide: (f) => showTooltip.from(f),
});

function signatureDom(help: SqlSignatureHelp): HTMLElement {
  const dom = document.createElement("div");
  dom.className = "cm-sql-signature";
  const sig = document.createElement("div");
  sig.className = "cm-sql-signature-label";
  const first = help.signatures[0] ?? help.name;
  const open = first.indexOf("(");
  const close = first.lastIndexOf(")");
  if (open < 0 || close < open || help.parameters.length === 0) {
    sig.textContent = first;
  } else {
    sig.append(first.slice(0, open + 1));
    help.parameters.forEach((p, i) => {
      if (i > 0) sig.append(", ");
      const span = document.createElement("span");
      span.textContent = p;
      if (i === Math.min(help.activeParameter, help.parameters.length - 1)) span.className = "cm-sql-signature-active";
      sig.append(span);
    });
    sig.append(first.slice(close));
  }
  dom.append(sig);
  if (help.signatures.length > 1) {
    const more = document.createElement("div");
    more.className = "cm-sql-signature-more";
    more.textContent = help.signatures.slice(1, 4).join("\n");
    dom.append(more);
  }
  if (help.description) {
    const desc = document.createElement("div");
    desc.className = "cm-sql-signature-doc";
    desc.textContent = help.description;
    dom.append(desc);
  }
  return dom;
}

function signaturePlugin(options: LanguageServiceOptions) {
  return ViewPlugin.fromClass(
    class {
      timer: ReturnType<typeof setTimeout> | undefined;
      request = 0;
      constructor(readonly view: EditorView) {}
      update(update: ViewUpdate) {
        if (!update.docChanged && !update.selectionSet && !update.focusChanged) return;
        clearTimeout(this.timer);
        const { state } = update;
        const sel = state.selection.main;
        if (!update.view.hasFocus || !sel.empty) {
          if (state.field(signatureField)) this.clear();
          return;
        }
        this.timer = setTimeout(() => this.fetch(state), 120);
      }
      fetch(state: EditorState) {
        const id = ++this.request;
        const pos = state.selection.main.head;
        client.sql
          .signature(options.connectionId, state.doc.toString(), pos)
          .then((help) => {
            if (id !== this.request || this.view.state.doc !== state.doc) return;
            const shown = this.view.state.field(signatureField);
            if (!help && !shown) return;
            this.view.dispatch({ effects: setSignature.of(help ? { help, pos } : null) });
          })
          .catch(() => undefined);
      }
      clear() {
        this.request++;
        queueMicrotask(() => this.view.dispatch({ effects: setSignature.of(null) }));
      }
      destroy() {
        clearTimeout(this.timer);
        this.request++;
      }
    },
  );
}

/* ------------------------------------------------------------------------------------------
 * {{variable}} highlighting
 * ---------------------------------------------------------------------------------------- */

const variableMatcher = new MatchDecorator({
  regexp: /\{\{\s*[A-Za-z_]\w*(?:\.[A-Za-z_]\w*)?\s*\}\}/g,
  decoration: Decoration.mark({ class: "cm-sql-variable" }),
});

const variableHighlight = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = variableMatcher.createDeco(view);
    }
    update(update: ViewUpdate) {
      this.decorations = variableMatcher.updateDeco(update, this.decorations);
    }
  },
  { decorations: (v) => v.decorations },
);

/** Re-runs diagnostics without a document change, e.g. after variables were defined. */
const refreshDiagnostics = StateEffect.define<null>();

export function refreshSqlDiagnostics(view: EditorView) {
  view.dispatch({ effects: refreshDiagnostics.of(null) });
}

/* ------------------------------------------------------------------------------------------
 * The extension
 * ---------------------------------------------------------------------------------------- */

export function sqlLanguageService(options: LanguageServiceOptions): Extension {
  const { connectionId, getVariables } = options;

  async function complete(ctx: CompletionContext): Promise<CompletionResult | null> {
    const word = ctx.matchBefore(/[\w$]+/);
    const before = ctx.state.sliceDoc(Math.max(0, ctx.pos - 2), ctx.pos);
    const triggered = before.endsWith(".") || before === "{{" || before === "::";
    if (!ctx.explicit && !word && !triggered) return null;
    try {
      const result = await client.sql.complete(connectionId, ctx.state.doc.toString(), ctx.pos, getVariables());
      if (ctx.aborted || result.items.length === 0) return null;
      return { from: result.from, options: result.items.map(toCompletion) };
    } catch {
      return null;
    }
  }

  let check: AbortController | null = null;

  const lintSource = async (view: EditorView): Promise<Diagnostic[]> => {
    const state = view.state;
    const sql = state.doc.toString();
    const variables = getVariables();
    check?.abort();
    check = null;
    let analysis: SqlAnalysis;
    try {
      analysis = await client.sql.analyze(sql, variables);
    } catch {
      return [];
    }
    options.onAnalysis?.(analysis);
    const local: Diagnostic[] = analysis.diagnostics.map((d) => ({
      from: Math.min(d.from, sql.length),
      to: Math.min(d.to, sql.length),
      severity: d.severity,
      message: d.hint ? `${d.message}\n${d.hint}` : d.message,
    }));

    const runnable = sql.trim() !== "" && analysis.statements === 1 && !local.some((d) => d.severity === "error");
    if (!runnable) {
      options.onCheck?.(sql.trim() ? { status: "skipped" } : { status: "idle" });
      return local;
    }

    // The server check runs after the local diagnostics are shown and adds its error, if any.
    const controller = new AbortController();
    check = controller;
    options.onCheck?.({ status: "checking" });
    client.sql
      .check(connectionId, sql, variables ?? [], controller.signal)
      .then((outcome) => {
        if (controller.signal.aborted || view.state.doc !== state.doc) return;
        check = null;
        options.onCheck?.({ status: "done", outcome });
        if (!outcome.ok) view.dispatch(setDiagnostics(view.state, [...local, serverDiagnostic(sql, outcome)]));
      })
      .catch(() => {
        if (!controller.signal.aborted) options.onCheck?.({ status: "idle" });
      });
    return local;
  };

  return [
    autocompletion({ override: [complete], icons: true, maxRenderedOptions: 120 }),
    linter(lintSource, {
      delay: 450,
      needsRefresh: (u) => u.transactions.some((tr) => tr.effects.some((e) => e.is(refreshDiagnostics))),
    }),
    lintGutter(),
    signatureField,
    signaturePlugin(options),
    variableHighlight,
    EditorView.updateListener.of((u) => {
      // Typing invalidates an in-flight server check.
      if (u.docChanged && check) {
        check.abort();
        check = null;
      }
    }),
  ];
}
