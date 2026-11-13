/**
 * Variables panel: one card per `{{variable}}` with its value, type and Required flag.
 * Cards appear automatically when the SQL references a new name.
 */
import type * as React from "react";
import { useState } from "react";
import {
  AlertTriangle,
  BookOpen,
  ChevronDown,
  ChevronRight,
  ChevronUp,
  Loader2,
  RefreshCw,
  Trash2,
} from "lucide-react";
import { VARIABLE_TYPES, type VariableDef, type VariableOptionsSource, type VariableType } from "@/services/client";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Field } from "@/components/ui/field";
import { Segmented, SegmentedItem } from "@/components/ui/segmented";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Tooltip } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import { TYPE_LABELS, changeType, choicesToText, textToChoices, valueProblem } from "./model";
import { ValueInput } from "./ValueInput";
import { useVariableOptions } from "./useVariableOptions";

type Props = {
  connectionId: string;
  /** Identifies the open query, so per-card editing state resets when another one opens. */
  queryKey: string;
  /** The query, so Multi-select can offer the values of the column it filters. */
  sql: string;
  variables: VariableDef[];
  /** Names referenced in the SQL right now. */
  used: Set<string>;
  /** Highlight variables without a valid value (after a run was attempted). */
  showProblems: boolean;
  onChange: (name: string, next: VariableDef) => void;
  onRemove: (name: string) => void;
};

export function VariablesPanel({
  connectionId,
  queryKey,
  sql,
  variables,
  used,
  showProblems,
  onChange,
  onRemove,
}: Props) {
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  /** The guide sits collapsed at the bottom once there are variables; open, it fills the panel. */
  const [guideOpen, setGuideOpen] = useState(false);
  const hasVariables = variables.length > 0;
  const toggle = (name: string) =>
    setCollapsed((s) => {
      const next = new Set(s);
      if (next.has(name)) next.delete(name);
      else next.add(name);
      return next;
    });

  return (
    <aside className="flex h-full min-h-0 flex-col border-l border-border bg-surface">
      <div className="flex h-9 shrink-0 items-center gap-2 border-b border-border px-3">
        <h2 className="text-[13px] font-semibold">{variables.length === 0 ? "Editor guide" : "Variables"}</h2>
        {variables.length > 0 && <span className="text-xs text-subtle-foreground">{variables.length}</span>}
      </div>
      <div className={cn("min-h-0 flex-1 overflow-y-auto", hasVariables && guideOpen && "hidden")}>
        {!hasVariables ? (
          <EditorGuide />
        ) : (
          variables.map((def) => (
            <VariableCard
              sql={sql}
              key={`${queryKey}:${def.name}`}
              connectionId={connectionId}
              def={def}
              used={used.has(def.name)}
              showProblems={showProblems}
              open={!collapsed.has(def.name)}
              onToggle={() => toggle(def.name)}
              onChange={(next) => onChange(def.name, next)}
              onRemove={() => onRemove(def.name)}
            />
          ))
        )}
      </div>

      {hasVariables && (
        <section className={cn("flex flex-col border-t border-border", guideOpen && "min-h-0 flex-1")}>
          <button
            type="button"
            aria-expanded={guideOpen}
            onClick={() => setGuideOpen((o) => !o)}
            className="flex h-9 shrink-0 items-center gap-2 px-3 text-left outline-none hover:bg-surface-2 focus-visible:shadow-[inset_0_0_0_1px_var(--primary)]"
          >
            <BookOpen className="size-3.5 text-muted-foreground" />
            <span className="flex-1 text-[13px] font-semibold">Editor guide</span>
            {guideOpen ? (
              <ChevronDown className="size-3.5 text-muted-foreground" />
            ) : (
              <ChevronUp className="size-3.5 text-muted-foreground" />
            )}
          </button>
          {guideOpen && (
            <div className="min-h-0 flex-1 overflow-y-auto border-t border-border">
              <EditorGuide />
            </div>
          )}
        </section>
      )}
    </aside>
  );
}

/** One variable type in the guide: name, what it is for, and how to write it. */
const TYPE_GUIDE: { type: string; what: string; sql: string }[] = [
  { type: "Text", what: "Any text, such as a code or name.", sql: "customer_name = {{customer}}" },
  { type: "Number", what: "A whole or decimal number.", sql: "amount >= {{min_amount}}" },
  { type: "Date", what: "One date, picked from a calendar.", sql: "order_date >= {{start_date}}" },
  {
    type: "Date range",
    what: "A from and a to date, both included.",
    sql: "order_date BETWEEN {{period.from}}\n  AND {{period.to}}",
  },
  { type: "Enum", what: "One choice from a list you type.", sql: "region = {{region}}" },
  {
    type: "Multi-select",
    what: "Several values, picked from the column's own values.",
    sql: "status IN ({{status}})",
  },
];

function GuideSection({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="space-y-2 border-b border-border px-4 py-3.5 last:border-b-0">
      <h3 className="text-[12px] font-semibold text-foreground">{title}</h3>
      {children}
    </section>
  );
}

function Code({ children }: { children: string }) {
  return (
    <pre className="border border-border bg-surface-sunken px-2.5 py-1.5 font-mono text-[11.5px] leading-relaxed whitespace-pre-wrap text-foreground">
      {children}
    </pre>
  );
}

function Key({ children }: { children: string }) {
  return (
    <kbd className="border border-border bg-surface-2 px-1 py-px font-mono text-[10.5px] text-muted-foreground">
      {children}
    </kbd>
  );
}

/** How the SQL Editor, variables and filters work. Fills the panel while there are no
 * variables; afterwards it is a collapsible section at the bottom. */
function EditorGuide() {
  return (
    <div className="text-xs leading-relaxed text-muted-foreground">
      <GuideSection title="Add a filter">
        <p>
          Write a name in double braces where a value belongs. It appears in this panel, where you pick its type and
          enter its value.
        </p>
        <Code>{"SELECT * FROM orders\nWHERE customer_name = {{customer}}"}</Code>
        <p>
          Names use lower-case letters, digits and <code className="font-mono">_</code>. Don't put quotes around a
          variable: <code className="font-mono">{"{{customer}}"}</code> is already a value.
        </p>
      </GuideSection>

      <GuideSection title="Types">
        <ul className="space-y-2.5">
          {TYPE_GUIDE.map((t) => (
            <li key={t.type} className="space-y-1">
              <p>
                <span className="font-medium text-foreground">{t.type}</span> · {t.what}
              </p>
              <Code>{t.sql}</Code>
            </li>
          ))}
        </ul>
        <p>The type is guessed from the name and how you use it; change it in the variable's settings.</p>
      </GuideSection>

      <GuideSection title="Required">
        <div className="flex gap-2.5">
          <Checkbox checked aria-hidden tabIndex={-1} className="pointer-events-none mt-0.5" />
          <p>
            <span className="font-medium text-foreground">Checked: the filter is applied.</span> It needs a value; the
            query won't run without one.
          </p>
        </div>
        <div className="flex gap-2.5">
          <Checkbox checked={false} aria-hidden tabIndex={-1} className="pointer-events-none mt-0.5" />
          <p>
            <span className="font-medium text-foreground">Unchecked: the filter is skipped,</span> even if a value is
            filled in. Its condition is left out, so you get all rows.
          </p>
        </div>
        <Code>
          {"WHERE customer_name = {{customer}}\n  AND total > 0\n-- unchecked runs as:\nWHERE TRUE AND total > 0"}
        </Code>
      </GuideSection>

      <GuideSection title="Shortcuts">
        <ul className="space-y-1.5">
          <li>
            <Key>Ctrl</Key> + <Key>Enter</Key> runs the query, or just the selected part.
          </li>
          <li>
            <Key>Ctrl</Key> + <Key>Space</Key> shows suggestions: tables, columns, functions.
          </li>
          <li>
            <Key>Ctrl</Key> + <Key>S</Key> saves the query.
          </li>
        </ul>
        <p>
          Problems are underlined as you type, and PostgreSQL checks the query without running it. Queries only read
          data.
        </p>
      </GuideSection>

      <GuideSection title="Safe by design">
        <p>Filter values are sent to PostgreSQL separately from the SQL, so a value can never change the query.</p>
      </GuideSection>
    </div>
  );
}

function VariableCard({
  connectionId,
  sql,
  def,
  used,
  open,
  showProblems,
  onToggle,
  onChange,
  onRemove,
}: {
  connectionId: string;
  sql: string;
  def: VariableDef;
  used: boolean;
  open: boolean;
  showProblems: boolean;
  onToggle: () => void;
  onChange: (next: VariableDef) => void;
  onRemove: () => void;
}) {
  const id = `var-${def.name}`;
  // Only single-choice Enum has a list of choices; Multi-select reads its column's values.
  const enumLike = def.type === "enum";
  const problem = def.required ? valueProblem(def, def.default) : null;
  const needsChoices =
    enumLike &&
    (def.options == null ||
      (def.options.source === "list" && def.options.values.length === 0) ||
      (def.options.source === "query" && !def.options.sql.trim()));

  return (
    <section className="border-b border-border">
      <div className="flex items-center">
        <button
          type="button"
          onClick={onToggle}
          aria-expanded={open}
          className="flex min-w-0 flex-1 items-center gap-2 py-2 pr-1 pl-2 text-left outline-none hover:bg-surface-2 focus-visible:bg-surface-2"
        >
          {open ? (
            <ChevronDown className="size-3.5 shrink-0 text-muted-foreground" />
          ) : (
            <ChevronRight className="size-3.5 shrink-0 text-muted-foreground" />
          )}
          <code className="truncate bg-primary-soft px-1 font-mono text-[12px] font-medium text-primary-text">
            {`{{${def.name}}}`}
          </code>
          <span className="truncate text-xs text-muted-foreground">{TYPE_LABELS[def.type]}</span>
          {(!used || needsChoices || (showProblems && problem)) && (
            <Tooltip content={!used ? "Not used in the SQL" : needsChoices ? "Add the choices" : (problem ?? "")}>
              <AlertTriangle
                className={cn("ml-auto size-3.5 shrink-0", showProblems && problem ? "text-danger" : "text-warning")}
              />
            </Tooltip>
          )}
        </button>
        <Tooltip content="Remove variable">
          <Button size="icon-sm" variant="ghost" aria-label={`Remove ${def.name}`} onClick={onRemove} className="mr-1">
            <Trash2 />
          </Button>
        </Tooltip>
      </div>

      {open && (
        <div className="space-y-3 px-3 pt-1 pb-4">
          {!used && <p className="text-xs text-warning">Not used in the SQL any more. Remove it or use it again.</p>}

          <div className="flex items-end gap-3">
            <Field label="Type" htmlFor={`${id}-type`} className="flex-1">
              <Select value={def.type} onValueChange={(t) => onChange(changeType(def, t as VariableType))}>
                <SelectTrigger id={`${id}-type`}>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {VARIABLE_TYPES.map((t) => (
                    <SelectItem key={t} value={t}>
                      {TYPE_LABELS[t]}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </Field>
            <label className="flex h-8 cursor-pointer items-center gap-2 text-[13px]">
              <Checkbox checked={def.required} onCheckedChange={(v) => onChange({ ...def, required: v === true })} />
              Required
            </label>
          </div>
          {!def.required && (
            <p className="-mt-1 text-xs text-subtle-foreground">
              Not applied: this filter is skipped when the query runs.
            </p>
          )}

          {enumLike && <ChoicesEditor connectionId={connectionId} def={def} onChange={onChange} />}

          <Field
            label="Value"
            error={showProblems && problem ? problem : undefined}
            className={cn(!def.required && "opacity-60")}
          >
            <ValueInput
              connectionId={connectionId}
              sql={sql}
              def={def}
              value={def.default}
              label={`Value of ${def.name}`}
              invalid={showProblems && problem != null}
              onChange={(v) => onChange({ ...def, default: v })}
              className="w-full"
            />
          </Field>
        </div>
      )}
    </section>
  );
}

function ChoicesEditor({
  connectionId,
  def,
  onChange,
}: {
  connectionId: string;
  def: VariableDef;
  onChange: (next: VariableDef) => void;
}) {
  const source: VariableOptionsSource = def.options ?? { source: "list", values: [] };
  // Raw text is kept locally so typing a trailing "|" or blank line isn't normalised away.
  const [text, setText] = useState(() => (source.source === "list" ? choicesToText(source.values) : ""));
  const [lookupSql, setLookupSql] = useState(() => (source.source === "query" ? source.sql : ""));
  const { state, reload } = useVariableOptions(connectionId, def);

  const setSource = (kind: string) => {
    if (kind === "list") onChange({ ...def, options: { source: "list", values: textToChoices(text) } });
    else onChange({ ...def, options: { source: "query", sql: lookupSql } });
  };

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between gap-2">
        <span className="text-[13px] font-medium">Choices</span>
        <Segmented value={source.source} onValueChange={setSource} aria-label="Where the choices come from">
          <SegmentedItem value="list" className="h-[26px] px-2 text-xs">
            Fixed list
          </SegmentedItem>
          <SegmentedItem value="query" className="h-[26px] px-2 text-xs">
            Lookup query
          </SegmentedItem>
        </Segmented>
      </div>
      {source.source === "list" ? (
        <>
          <textarea
            aria-label="Choices, one per line"
            value={text}
            rows={Math.min(8, Math.max(3, text.split("\n").length + 1))}
            spellCheck={false}
            placeholder={"north | North\nsouth | South"}
            onChange={(e) => {
              setText(e.target.value);
              onChange({ ...def, options: { source: "list", values: textToChoices(e.target.value) } });
            }}
            className="w-full resize-y border border-border bg-surface-2 px-2.5 py-1.5 font-mono text-[12px] leading-relaxed outline-none placeholder:text-subtle-foreground hover:border-border-strong focus-visible:border-primary focus-visible:shadow-[0_0_0_1px_var(--primary)]"
          />
          <p className="text-xs text-subtle-foreground">
            One per line. Add a label after <code className="font-mono">|</code> to show something friendlier than the
            value.
          </p>
        </>
      ) : (
        <>
          <textarea
            aria-label="Lookup query"
            value={lookupSql}
            rows={3}
            spellCheck={false}
            placeholder={"SELECT DISTINCT region\nFROM sales.orders\nORDER BY 1"}
            onChange={(e) => {
              setLookupSql(e.target.value);
              onChange({ ...def, options: { source: "query", sql: e.target.value } });
            }}
            className="w-full resize-y border border-border bg-surface-2 px-2.5 py-1.5 font-mono text-[12px] leading-relaxed outline-none placeholder:text-subtle-foreground hover:border-border-strong focus-visible:border-primary focus-visible:shadow-[0_0_0_1px_var(--primary)]"
          />
          <div className="flex items-start justify-between gap-2">
            <p
              className={cn("min-w-0 text-xs", state.status === "error" ? "text-danger" : "text-subtle-foreground")}
              role={state.status === "error" ? "alert" : undefined}
            >
              {state.status === "loading" && (
                <span className="inline-flex items-center gap-1.5">
                  <Loader2 className="size-3 animate-spin" /> Loading choices…
                </span>
              )}
              {state.status === "ready" &&
                `${state.options.length} choice${state.options.length === 1 ? "" : "s"}${state.truncated ? " (first 1,000 only)" : ""}.`}
              {state.status === "error" && state.message}
              {state.status === "idle" && "Column 1 is the value; an optional column 2 is the label."}
            </p>
            <Button
              size="sm"
              variant="secondary"
              className="shrink-0 px-2"
              onClick={reload}
              disabled={!lookupSql.trim()}
            >
              <RefreshCw /> Reload
            </Button>
          </div>
        </>
      )}
    </div>
  );
}
