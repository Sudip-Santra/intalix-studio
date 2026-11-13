/**
 * Input control for one variable value, by type, shown in the Variables panel.
 */
import { useMemo, useState } from "react";
import { CalendarRange, ChevronDown, Loader2, X } from "lucide-react";
import type { DateRangeValue, VariableDef, VariableValue } from "@/services/client";
import { DatePicker } from "@/components/ui/date-picker";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Tooltip } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import { rangePresets } from "./model";
import { optionLabel, useVariableOptions } from "./useVariableOptions";

/** Radix Select can't use "" as a value; this stands for "no value". */
const ANY = "\u0000any";

type Props = {
  connectionId: string;
  def: VariableDef;
  value: VariableValue;
  onChange: (value: VariableValue) => void;
  invalid?: boolean;
  /** Accessible name, e.g. "Start date". */
  label: string;
  className?: string;
  /** The query, so a Multi-select can offer the values of the column it filters. */
  sql?: string;
};

export function ValueInput({ connectionId, def, value, onChange, invalid, label, className, sql = "" }: Props) {
  switch (def.type) {
    case "text":
    case "number":
      return (
        <Input
          aria-label={label}
          aria-invalid={invalid || undefined}
          inputMode={def.type === "number" ? "decimal" : undefined}
          placeholder={def.required ? "Enter a value" : "Not applied"}
          value={typeof value === "string" ? value : ""}
          onChange={(e) => onChange(e.target.value)}
          className={cn(def.type === "number" && "tabular font-mono text-[12.5px]", className)}
        />
      );
    case "date":
      return (
        <DateField
          label={label}
          value={typeof value === "string" ? value : ""}
          onChange={(v) => onChange(v || null)}
          invalid={invalid}
          className={className}
        />
      );
    case "date_range":
      return (
        <DateRangeField
          label={label}
          value={value && typeof value === "object" && !Array.isArray(value) ? value : { from: "", to: "" }}
          onChange={(v) => onChange(v.from || v.to ? v : null)}
          invalid={invalid}
          className={className}
        />
      );
    case "enum":
      return (
        <EnumField
          connectionId={connectionId}
          def={def}
          label={label}
          value={typeof value === "string" ? value : ""}
          onChange={(v) => onChange(v || null)}
          invalid={invalid}
          className={className}
        />
      );
    case "multi_enum":
      return (
        <MultiEnumField
          connectionId={connectionId}
          def={def}
          sql={sql}
          label={label}
          value={Array.isArray(value) ? value : []}
          onChange={(v) => onChange(v.length ? v : null)}
          invalid={invalid}
          className={className}
        />
      );
  }
}

function DateField({
  label,
  value,
  onChange,
  invalid,
  className,
  range,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  invalid?: boolean;
  className?: string;
  range?: DateRangeValue;
}) {
  return (
    <DatePicker label={label} value={value} onChange={onChange} invalid={invalid} range={range} className={className} />
  );
}

function DateRangeField({
  label,
  value,
  onChange,
  invalid,
  className,
}: {
  label: string;
  value: DateRangeValue;
  onChange: (v: DateRangeValue) => void;
  invalid?: boolean;
  className?: string;
}) {
  const presets = useMemo(() => rangePresets(), []);
  return (
    <div className={cn("flex items-center", className)}>
      <DateField
        label={`${label}, from`}
        value={value.from}
        onChange={(from) => onChange({ ...value, from })}
        invalid={invalid}
        range={value}
        className="min-w-0 flex-1 px-2"
      />
      <span className="px-1 text-xs text-subtle-foreground">–</span>
      <DateField
        label={`${label}, to`}
        value={value.to}
        onChange={(to) => onChange({ ...value, to })}
        invalid={invalid}
        range={value}
        className="min-w-0 flex-1 px-2"
      />
      <DropdownMenu>
        <Tooltip content="Quick ranges">
          <DropdownMenuTrigger asChild>
            <button
              type="button"
              aria-label={`Quick ranges for ${label}`}
              className="ml-1 flex size-8 shrink-0 items-center justify-center border border-border bg-surface-2 text-muted-foreground outline-none hover:border-border-strong hover:text-foreground focus-visible:border-primary"
            >
              <CalendarRange className="size-3.5" />
            </button>
          </DropdownMenuTrigger>
        </Tooltip>
        <DropdownMenuContent align="end" className="min-w-[180px]">
          {presets.map((p) => (
            <DropdownMenuItem key={p.label} onSelect={() => onChange(p.value)}>
              {p.label}
            </DropdownMenuItem>
          ))}
          <DropdownMenuSeparator />
          <DropdownMenuItem onSelect={() => onChange({ from: "", to: "" })}>Clear</DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}

function OptionsStatus({ status, message }: { status: string; message?: string }) {
  if (status === "loading")
    return (
      <span className="flex items-center gap-2 px-2 py-1.5 text-xs text-muted-foreground">
        <Loader2 className="size-3.5 animate-spin" /> Loading choices…
      </span>
    );
  if (status === "error") return <span className="block max-w-[280px] px-2 py-1.5 text-xs text-danger">{message}</span>;
  return <span className="block px-2 py-1.5 text-xs text-subtle-foreground">No choices yet.</span>;
}

function EnumField({
  connectionId,
  def,
  label,
  value,
  onChange,
  invalid,
  className,
}: {
  connectionId: string;
  def: VariableDef;
  label: string;
  value: string;
  onChange: (v: string) => void;
  invalid?: boolean;
  className?: string;
}) {
  const { state } = useVariableOptions(connectionId, def);
  const known = state.options.some((o) => o.value === value);
  return (
    <Select value={value || ANY} onValueChange={(v) => onChange(v === ANY ? "" : v)}>
      <SelectTrigger
        aria-label={label}
        aria-invalid={invalid || undefined}
        className={cn("w-[180px] aria-invalid:border-danger", className)}
      >
        <SelectValue placeholder={def.required ? "Choose…" : "Not applied"} />
      </SelectTrigger>
      <SelectContent className="max-h-[320px] overflow-y-auto">
        {!def.required && <SelectItem value={ANY}>Empty (no filter)</SelectItem>}
        {/* Keeps a saved value visible even if the list no longer offers it. */}
        {value && !known && <SelectItem value={value}>{value}</SelectItem>}
        {state.options.map((o) => (
          <SelectItem key={o.value} value={o.value}>
            {optionLabel(o)}
          </SelectItem>
        ))}
        {state.options.length === 0 && (
          <OptionsStatus status={state.status} message={state.status === "error" ? state.message : undefined} />
        )}
      </SelectContent>
    </Select>
  );
}

function MultiEnumField({
  connectionId,
  def,
  sql,
  label,
  value,
  onChange,
  invalid,
  className,
}: {
  connectionId: string;
  def: VariableDef;
  sql: string;
  label: string;
  value: string[];
  onChange: (v: string[]) => void;
  invalid?: boolean;
  className?: string;
}) {
  // Column values load when the dropdown opens, from the query as it is now.
  const [open, setOpen] = useState(false);
  const { state } = useVariableOptions(connectionId, def, sql, open);
  const [search, setSearch] = useState("");
  const labels = new Map(state.options.map((o) => [o.value, optionLabel(o)]));
  const summary =
    value.length === 0
      ? def.required
        ? "Choose values…"
        : "Not applied"
      : value.length <= 2
        ? value.map((v) => labels.get(v) ?? v).join(", ")
        : `${value.length} selected`;
  const query = search.trim().toLowerCase();
  const shown = query
    ? state.options.filter((o) => optionLabel(o).toLowerCase().includes(query) || o.value.toLowerCase().includes(query))
    : state.options;
  const toggle = (v: string, on: boolean) => onChange(on ? [...value, v] : value.filter((x) => x !== v));

  return (
    <DropdownMenu
      onOpenChange={(next) => {
        setOpen(next);
        if (!next) setSearch("");
      }}
    >
      <DropdownMenuTrigger asChild>
        <button
          type="button"
          aria-label={label}
          aria-invalid={invalid || undefined}
          className={cn(
            "flex h-8 w-[200px] items-center justify-between gap-2 border border-border bg-surface-2 px-2.5 text-left text-[13px] outline-none hover:border-border-strong focus-visible:border-primary focus-visible:shadow-[0_0_0_1px_var(--primary)] aria-invalid:border-danger",
            value.length === 0 && "text-subtle-foreground",
            className,
          )}
        >
          <span className="truncate">{summary}</span>
          <span className="flex shrink-0 items-center gap-1">
            {value.length > 0 && (
              <span
                role="button"
                tabIndex={-1}
                aria-label={`Clear ${label}`}
                onPointerDown={(e) => {
                  e.preventDefault();
                  e.stopPropagation();
                  onChange([]);
                }}
                className="text-muted-foreground hover:text-foreground"
              >
                <X className="size-3.5" />
              </span>
            )}
            <ChevronDown className="size-3.5 text-muted-foreground" />
          </span>
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-[280px] p-0">
        {state.status === "ready" && state.column && (
          <p className="truncate border-b border-border px-2.5 py-1.5 text-[11px] text-subtle-foreground">
            Values of <span className="font-mono text-muted-foreground">{state.column}</span>
            {state.truncated && " (first 1,000)"}
          </p>
        )}
        {state.options.length > 8 && (
          <div className="border-b border-border p-1.5">
            <Input
              autoFocus
              placeholder="Search choices"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              // Typing must reach the input, not the menu's type-ahead.
              onKeyDown={(e) => e.stopPropagation()}
              className="h-7"
            />
          </div>
        )}
        <div className="max-h-[300px] overflow-y-auto p-1">
          {shown.map((o) => (
            <DropdownMenuCheckboxItem
              key={o.value}
              checked={value.includes(o.value)}
              onCheckedChange={(on) => toggle(o.value, on === true)}
              onSelect={(e) => e.preventDefault()}
            >
              <span className="truncate">{optionLabel(o)}</span>
            </DropdownMenuCheckboxItem>
          ))}
          {state.options.length === 0 && (
            <OptionsStatus status={state.status} message={state.status === "error" ? state.message : undefined} />
          )}
          {state.options.length > 0 && shown.length === 0 && (
            <DropdownMenuLabel className="font-normal">No match</DropdownMenuLabel>
          )}
        </div>
        {state.options.length > 1 && (
          <div className="flex border-t border-border text-xs">
            <button
              type="button"
              className="flex-1 py-1.5 text-muted-foreground hover:bg-surface-2 hover:text-foreground"
              onClick={() => onChange(state.options.map((o) => o.value))}
            >
              Select all
            </button>
            <button
              type="button"
              className="flex-1 border-l border-border py-1.5 text-muted-foreground hover:bg-surface-2 hover:text-foreground"
              onClick={() => onChange([])}
            >
              Clear
            </button>
          </div>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
