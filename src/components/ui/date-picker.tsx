/**
 * Date picker in the app's design: a field showing the date, and a month calendar in a
 * popover. Values are calendar dates as `YYYY-MM-DD` (no time zone involved).
 *
 * Click the month or year in the header to pick them from a grid.
 * Keyboard: arrows move by day / week, PageUp/PageDown by month (with Shift: by year),
 * Home/End to the start/end of the week, Enter or Space to choose, Escape to close.
 */
import { useEffect, useMemo, useRef, useState } from "react";
import { CalendarDays, ChevronLeft, ChevronRight } from "lucide-react";
import { cn } from "@/lib/utils";
import { Popover, PopoverContent, PopoverTrigger } from "./popover";

interface Ymd {
  y: number;
  m: number; // 0-11
  d: number;
}

const MONTHS = [
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
];
const WEEKDAYS = ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"];

const pad = (n: number) => String(n).padStart(2, "0");
const toIso = ({ y, m, d }: Ymd) => `${String(y).padStart(4, "0")}-${pad(m + 1)}-${pad(d)}`;
const daysIn = (y: number, m: number) => new Date(y, m + 1, 0).getDate();

/** "2026-11-13" → parts, or null when not a real date. */
function parseIso(value: string): Ymd | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (!match) return null;
  const y = Number(match[1]);
  const m = Number(match[2]) - 1;
  const d = Number(match[3]);
  if (y < 1 || m < 0 || m > 11 || d < 1 || d > daysIn(y, m)) return null;
  return { y, m, d };
}

/** Moves a date by days (via a Date so month and year roll over). */
function addDays(p: Ymd, days: number): Ymd {
  const dt = new Date(p.y, p.m, p.d + days);
  return { y: dt.getFullYear(), m: dt.getMonth(), d: dt.getDate() };
}

/** Moves by months, keeping the day where possible (31 Jan + 1 month → 28/29 Feb). */
function addMonths(p: Ymd, months: number): Ymd {
  const total = p.y * 12 + p.m + months;
  const y = Math.floor(total / 12);
  const m = total - y * 12;
  return { y, m, d: Math.min(p.d, daysIn(y, m)) };
}

function today(): Ymd {
  const now = new Date();
  return { y: now.getFullYear(), m: now.getMonth(), d: now.getDate() };
}

/** "13 Nov 2026" */
export function formatDisplayDate(value: string): string {
  const p = parseIso(value);
  return p ? `${p.d} ${MONTHS[p.m]!.slice(0, 3)} ${p.y}` : value;
}

type Props = {
  value: string;
  onChange: (value: string) => void;
  /** Accessible name of the field. */
  label: string;
  placeholder?: string;
  invalid?: boolean;
  className?: string;
  /** Shades the days of a range in the calendar (for from/to pickers). */
  range?: { from: string; to: string };
};

export function DatePicker({ value, onChange, label, placeholder = "Pick a date", invalid, className, range }: Props) {
  const [open, setOpen] = useState(false);
  const selected = parseIso(value);
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          type="button"
          aria-label={value ? `${label}: ${formatDisplayDate(value)}` : label}
          aria-invalid={invalid || undefined}
          className={cn(
            "flex h-8 w-[136px] items-center gap-2 border border-border bg-surface-2 px-2.5 text-left text-[13px] outline-none",
            "hover:border-border-strong focus-visible:border-primary focus-visible:shadow-[0_0_0_1px_var(--primary)] aria-invalid:border-danger",
            "data-[state=open]:border-primary",
            className,
          )}
        >
          <CalendarDays className="size-3.5 shrink-0 text-muted-foreground" />
          <span className={cn("tabular truncate", !selected && "text-subtle-foreground")}>
            {selected ? formatDisplayDate(value) : placeholder}
          </span>
        </button>
      </PopoverTrigger>
      <PopoverContent align="start" className="w-[252px] p-2">
        <Calendar
          selected={selected}
          range={range}
          onSelect={(p) => {
            onChange(toIso(p));
            setOpen(false);
          }}
          onClear={() => {
            onChange("");
            setOpen(false);
          }}
        />
      </PopoverContent>
    </Popover>
  );
}

function Calendar({
  selected,
  range,
  onSelect,
  onClear,
}: {
  selected: Ymd | null;
  range?: { from: string; to: string };
  onSelect: (p: Ymd) => void;
  onClear: () => void;
}) {
  const now = useMemo(today, []);
  /** The day that has keyboard focus; its month is the month shown. */
  const [focus, setFocus] = useState<Ymd>(selected ?? now);
  /** Days of a month, the 12 months of a year, or a page of 12 years. */
  const [view, setView] = useState<"days" | "months" | "years">("days");
  const gridRef = useRef<HTMLDivElement>(null);
  const keyboard = useRef(false);

  // After a keyboard move, put focus on the new day.
  useEffect(() => {
    if (!keyboard.current) return;
    keyboard.current = false;
    gridRef.current?.querySelector<HTMLButtonElement>('[data-focus="true"]')?.focus();
  }, [focus]);

  // Focus the active day when the calendar opens.
  useEffect(() => {
    gridRef.current?.querySelector<HTMLButtonElement>('[data-focus="true"]')?.focus();
  }, []);

  const first = new Date(focus.y, focus.m, 1).getDay();
  const cells: Ymd[] = Array.from({ length: 42 }, (_, i) => addDays({ y: focus.y, m: focus.m, d: 1 }, i - first));
  const iso = (p: Ymd) => toIso(p);
  const from = range?.from || "";
  const to = range?.to || "";

  const move = (next: Ymd) => {
    keyboard.current = true;
    setFocus(next);
  };
  const onKeyDown = (e: React.KeyboardEvent) => {
    const keys: Record<string, () => Ymd> = {
      ArrowLeft: () => addDays(focus, -1),
      ArrowRight: () => addDays(focus, 1),
      ArrowUp: () => addDays(focus, -7),
      ArrowDown: () => addDays(focus, 7),
      PageUp: () => addMonths(focus, e.shiftKey ? -12 : -1),
      PageDown: () => addMonths(focus, e.shiftKey ? 12 : 1),
      Home: () => addDays(focus, -new Date(focus.y, focus.m, focus.d).getDay()),
      End: () => addDays(focus, 6 - new Date(focus.y, focus.m, focus.d).getDay()),
    };
    const go = keys[e.key];
    if (go) {
      e.preventDefault();
      move(go());
    }
  };

  const navButton =
    "flex size-7 items-center justify-center text-muted-foreground outline-none hover:bg-surface-2 hover:text-foreground focus-visible:shadow-[inset_0_0_0_1px_var(--primary)]";
  const headerButton =
    "h-7 px-1.5 text-[13px] font-semibold outline-none hover:bg-surface-2 focus-visible:shadow-[inset_0_0_0_1px_var(--primary)]";
  const pickCell = (active: boolean, current: boolean) =>
    cn(
      "flex items-center justify-center text-[12.5px] outline-none hover:bg-surface-2 focus-visible:shadow-[inset_0_0_0_1px_var(--primary)]",
      current && !active && "font-semibold text-primary-text",
      active && "bg-primary font-semibold text-primary-foreground hover:bg-primary-hover",
    );

  // ‹ › move by a month, a year or a page of years, depending on the view.
  const step = view === "days" ? 1 : view === "months" ? 12 : 144;
  const yearPageStart = focus.y - ((((focus.y - 1) % 12) + 12) % 12);
  const stepLabel = view === "days" ? "month" : view === "months" ? "year" : "12 years";

  return (
    <div>
      <div className="mb-1.5 flex items-center">
        <button
          type="button"
          aria-label={`Previous ${stepLabel}`}
          className={navButton}
          onClick={() => setFocus(addMonths(focus, -step))}
        >
          <ChevronLeft className="size-3.5" />
        </button>
        <div className="flex flex-1 items-center justify-center gap-0.5" aria-live="polite">
          {view === "days" && (
            <button type="button" className={headerButton} onClick={() => setView("months")}>
              {MONTHS[focus.m]}
            </button>
          )}
          {view === "years" ? (
            <span className="px-1.5 text-[13px] font-semibold">
              {yearPageStart} – {yearPageStart + 11}
            </span>
          ) : (
            <button type="button" className={headerButton} onClick={() => setView("years")}>
              {focus.y}
            </button>
          )}
        </div>
        <button
          type="button"
          aria-label={`Next ${stepLabel}`}
          className={navButton}
          onClick={() => setFocus(addMonths(focus, step))}
        >
          <ChevronRight className="size-3.5" />
        </button>
      </div>

      {view === "months" && (
        <div className="grid h-[220px] grid-cols-3 grid-rows-4" role="listbox" aria-label={`Months of ${focus.y}`}>
          {MONTHS.map((name, m) => (
            <button
              key={name}
              type="button"
              role="option"
              aria-selected={selected?.y === focus.y && selected.m === m}
              className={pickCell(selected?.y === focus.y && selected.m === m, now.y === focus.y && now.m === m)}
              onClick={() => {
                setFocus({ y: focus.y, m, d: Math.min(focus.d, daysIn(focus.y, m)) });
                setView("days");
              }}
            >
              {name.slice(0, 3)}
            </button>
          ))}
        </div>
      )}

      {view === "years" && (
        <div className="grid h-[220px] grid-cols-3 grid-rows-4" role="listbox" aria-label="Years">
          {Array.from({ length: 12 }, (_, i) => yearPageStart + i).map((y) => (
            <button
              key={y}
              type="button"
              role="option"
              aria-selected={selected?.y === y}
              className={cn("tabular", pickCell(selected?.y === y, now.y === y))}
              onClick={() => {
                setFocus({ y, m: focus.m, d: Math.min(focus.d, daysIn(y, focus.m)) });
                setView("months");
              }}
            >
              {y}
            </button>
          ))}
        </div>
      )}

      {view === "days" && (
        <>
          <div className="grid grid-cols-7">
            {WEEKDAYS.map((w) => (
              <span key={w} className="flex h-7 items-center justify-center text-[11px] text-subtle-foreground">
                {w}
              </span>
            ))}
          </div>
          <div
            ref={gridRef}
            role="grid"
            aria-label={`${MONTHS[focus.m]} ${focus.y}`}
            className="grid grid-cols-7"
            onKeyDown={onKeyDown}
          >
            {cells.map((c) => {
              const key = iso(c);
              const outside = c.m !== focus.m;
              const isSelected = selected != null && key === iso(selected);
              const isToday = key === iso(now);
              const isFocus = key === iso(focus);
              const inRange = from && to && key >= from && key <= to;
              return (
                <button
                  key={key}
                  type="button"
                  role="gridcell"
                  tabIndex={isFocus ? 0 : -1}
                  data-focus={isFocus}
                  aria-selected={isSelected}
                  aria-label={formatDisplayDate(key)}
                  onClick={() => onSelect(c)}
                  onFocus={() => !isFocus && setFocus(c)}
                  className={cn(
                    "tabular relative flex h-8 items-center justify-center text-[12.5px] outline-none",
                    "hover:bg-surface-2 focus-visible:shadow-[inset_0_0_0_1px_var(--primary)]",
                    outside ? "text-subtle-foreground/60" : "text-foreground",
                    inRange && !isSelected && "bg-primary-soft",
                    isToday && !isSelected && "font-semibold text-primary-text",
                    isSelected && "bg-primary font-semibold text-primary-foreground hover:bg-primary-hover",
                  )}
                >
                  {c.d}
                  {isToday && !isSelected && <span className="absolute bottom-1 size-1 bg-primary" />}
                </button>
              );
            })}
          </div>
        </>
      )}

      <div className="mt-1.5 flex border-t border-border pt-1.5 text-xs">
        <button
          type="button"
          className="flex-1 py-1 text-muted-foreground outline-none hover:bg-surface-2 hover:text-foreground focus-visible:shadow-[inset_0_0_0_1px_var(--primary)]"
          onClick={() => onSelect(now)}
        >
          Today
        </button>
        <button
          type="button"
          className="flex-1 py-1 text-muted-foreground outline-none hover:bg-surface-2 hover:text-foreground focus-visible:shadow-[inset_0_0_0_1px_var(--primary)]"
          onClick={onClear}
        >
          Clear
        </button>
      </div>
    </div>
  );
}
