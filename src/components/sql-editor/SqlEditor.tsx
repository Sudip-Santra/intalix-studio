import { useMemo, useRef } from "react";
import CodeMirror, { EditorView, keymap, Prec, type Extension } from "@uiw/react-codemirror";
import { PostgreSQL, sql } from "@codemirror/lang-sql";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { tags as t } from "@lezer/highlight";
import type { CompletionCatalog } from "@/services/client";

// Colours come from CSS variables so the editor follows light/dark themes without remounting.
const editorTheme = EditorView.theme({
  "&": {
    height: "100%",
    fontSize: "13px",
    backgroundColor: "var(--surface-sunken)",
    color: "var(--foreground)",
  },
  // Same line height on code and gutter, or line numbers drift out of line.
  ".cm-scroller": { fontFamily: "var(--font-mono)", lineHeight: "21px" },
  ".cm-content, .cm-gutters, .cm-gutterElement, .cm-line": { lineHeight: "21px" },
  ".cm-content": { caretColor: "var(--primary)", padding: "10px 0" },
  ".cm-cursor, .cm-dropCursor": { borderLeftColor: "var(--primary)", borderLeftWidth: "2px" },
  "&.cm-focused": { outline: "none" },
  ".cm-gutters": {
    backgroundColor: "var(--surface-sunken)",
    color: "var(--subtle-foreground)",
    border: "none",
    borderRight: "1px solid var(--border)",
  },
  ".cm-lineNumbers .cm-gutterElement": { padding: "0 10px 0 14px", minWidth: "40px" },
  ".cm-activeLine": { backgroundColor: "color-mix(in srgb, var(--foreground) 4%, transparent)" },
  ".cm-activeLineGutter": { backgroundColor: "transparent", color: "var(--foreground)" },
  "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection": {
    backgroundColor: "color-mix(in srgb, var(--primary) 22%, transparent) !important",
  },
  ".cm-matchingBracket": {
    backgroundColor: "color-mix(in srgb, var(--primary) 18%, transparent)",
    outline: "1px solid color-mix(in srgb, var(--primary) 50%, transparent)",
  },
  ".cm-tooltip": {
    border: "1px solid var(--border-strong)",
    backgroundColor: "var(--surface)",
    boxShadow: "0 12px 32px -8px rgb(0 0 0 / 0.4)",
  },
  ".cm-tooltip-autocomplete > ul": { fontFamily: "var(--font-mono)", fontSize: "12px", maxHeight: "280px" },
  ".cm-tooltip-autocomplete > ul > li": { padding: "3px 10px 3px 6px" },
  ".cm-tooltip-autocomplete > ul > li[aria-selected]": {
    backgroundColor: "var(--primary-soft)",
    color: "var(--foreground)",
  },
  ".cm-completionDetail": { color: "var(--subtle-foreground)", fontStyle: "normal", marginLeft: "12px" },
  ".cm-completionIcon": { opacity: "0.6", width: "1.2em" },
  ".cm-placeholder": { color: "var(--subtle-foreground)" },
  // {{variables}}
  ".cm-sql-variable": {
    color: "var(--primary-text)",
    backgroundColor: "color-mix(in srgb, var(--primary) 12%, transparent)",
    fontWeight: "500",
  },
  // Diagnostics
  ".cm-lintRange-error": {
    backgroundImage: "none",
    textDecoration: "underline wavy var(--danger)",
    textDecorationSkipInk: "none",
    textUnderlineOffset: "3px",
  },
  ".cm-lintRange-warning": {
    backgroundImage: "none",
    textDecoration: "underline wavy var(--warning)",
    textDecorationSkipInk: "none",
    textUnderlineOffset: "3px",
  },
  ".cm-tooltip-lint": { maxWidth: "460px" },
  ".cm-diagnostic": {
    padding: "6px 10px 6px 12px",
    fontFamily: "var(--font-sans)",
    fontSize: "12px",
    whiteSpace: "pre-wrap",
    borderLeftWidth: "2px",
  },
  ".cm-diagnostic-error": { borderLeftColor: "var(--danger)" },
  ".cm-diagnostic-warning": { borderLeftColor: "var(--warning)" },
  ".cm-diagnosticSource": { color: "var(--subtle-foreground)", fontSize: "11px", marginTop: "2px" },
  ".cm-gutter-lint": { width: "14px" },
  ".cm-gutter-lint .cm-gutterElement": { padding: "0 2px" },
  ".cm-lint-marker": { width: "8px", height: "8px", marginTop: "6px", content: "none" },
  ".cm-lint-marker-error": { backgroundColor: "var(--danger)", backgroundImage: "none" },
  ".cm-lint-marker-warning": { backgroundColor: "var(--warning)", backgroundImage: "none" },
  // Signature help
  ".cm-sql-signature": { padding: "6px 10px", maxWidth: "520px", fontSize: "12px" },
  ".cm-sql-signature-label": { fontFamily: "var(--font-mono)" },
  ".cm-sql-signature-active": { color: "var(--primary-text)", fontWeight: "600" },
  ".cm-sql-signature-more": {
    fontFamily: "var(--font-mono)",
    color: "var(--subtle-foreground)",
    whiteSpace: "pre-wrap",
    marginTop: "2px",
  },
  ".cm-sql-signature-doc": { color: "var(--muted-foreground)", marginTop: "4px", fontFamily: "var(--font-sans)" },
  ".cm-completionInfo": { padding: "6px 10px", fontSize: "12px", maxWidth: "360px" },
});

const highlight = HighlightStyle.define([
  { tag: [t.keyword, t.operatorKeyword, t.modifier], color: "var(--syntax-keyword)", fontWeight: "500" },
  { tag: [t.string, t.special(t.string)], color: "var(--syntax-string)" },
  { tag: [t.number, t.bool, t.null], color: "var(--syntax-number)" },
  { tag: [t.comment, t.lineComment, t.blockComment], color: "var(--syntax-comment)", fontStyle: "italic" },
  { tag: [t.typeName, t.standard(t.name)], color: "var(--syntax-type)" },
  { tag: [t.function(t.variableName), t.function(t.name)], color: "var(--syntax-function)" },
  { tag: [t.operator, t.punctuation, t.separator, t.paren, t.bracket], color: "var(--muted-foreground)" },
  { tag: t.special(t.name), color: "var(--foreground)" },
]);

type Props = {
  value: string;
  onChange?: (value: string) => void;
  /** Called on Ctrl+Enter with the selected text, or the whole document when nothing is selected. */
  onRun?: (sql: string) => void;
  catalog?: CompletionCatalog;
  /**
   * The backend language service (see `language.ts`). When given it replaces the built-in,
   * schema-only completion; `catalog` is then ignored. Must be stable across renders.
   */
  language?: Extension;
  defaultSchema?: string;
  readOnly?: boolean;
  placeholder?: string;
  autoFocus?: boolean;
  ariaLabel: string;
  /** Receives the editor instance, e.g. so a toolbar button can read the selection. */
  onViewReady?: (view: EditorView) => void;
};

export function SqlEditor({
  value,
  onChange,
  onRun,
  catalog,
  language,
  defaultSchema = "public",
  readOnly = false,
  placeholder,
  autoFocus,
  ariaLabel,
  onViewReady,
}: Props) {
  // Kept in a ref so a new onRun callback does not rebuild the editor's extensions.
  const runRef = useRef(onRun);
  runRef.current = onRun;
  const canRun = Boolean(onRun);

  const extensions = useMemo(() => {
    const exts: Extension[] = [
      language
        ? sql({ dialect: PostgreSQL, upperCaseKeywords: true })
        : sql({ dialect: PostgreSQL, schema: catalog ?? {}, defaultSchema, upperCaseKeywords: true }),
      syntaxHighlighting(highlight),
      editorTheme,
      EditorView.contentAttributes.of({ "aria-label": ariaLabel }),
    ];
    if (canRun) {
      exts.push(
        Prec.highest(
          keymap.of([
            {
              key: "Mod-Enter",
              run: (view) => {
                runRef.current?.(sqlToRun(view));
                return true;
              },
            },
          ]),
        ),
      );
    }
    if (language && !readOnly) exts.push(language);
    return exts;
  }, [catalog, language, readOnly, defaultSchema, canRun, ariaLabel]);

  return (
    <CodeMirror
      value={value}
      onChange={onChange}
      extensions={extensions}
      readOnly={readOnly}
      editable={!readOnly}
      placeholder={placeholder}
      autoFocus={autoFocus}
      onCreateEditor={(view) => {
        // The mono web font may load after the first layout; re-measure so the gutter lines up.
        document.fonts?.ready.then(() => view.requestMeasure()).catch(() => undefined);
        onViewReady?.(view);
      }}
      theme="none"
      height="100%"
      className="h-full"
      basicSetup={{
        lineNumbers: true,
        foldGutter: false,
        highlightActiveLine: !readOnly,
        highlightActiveLineGutter: !readOnly,
        // The language service brings its own completion.
        autocompletion: !readOnly && !language,
        bracketMatching: true,
        closeBrackets: !readOnly,
        searchKeymap: true,
        tabSize: 2,
      }}
    />
  );
}

/** Text a Run action should execute: the selection when there is one, else the whole document. */
export function sqlToRun(view: EditorView): string {
  const sel = view.state.selection.main;
  return sel.empty ? view.state.doc.toString() : view.state.sliceDoc(sel.from, sel.to);
}
