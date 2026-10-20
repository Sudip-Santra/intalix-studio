import { useMemo, useRef, useState, type KeyboardEvent, type PointerEvent } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { Cell, ResultColumn } from "@/services/client";
import { Braces, Copy, Rows3, TableProperties } from "lucide-react";
import { asTsv, copyText, displayCell, rowAsJson } from "@/lib/format";
import { copyToClipboard } from "@/lib/clipboard";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { cn } from "@/lib/utils";

const ROW_HEIGHT = 28;
const ROW_NUMBER_WIDTH = 56;
const MIN_COL = 64;
const MAX_INITIAL_COL = 360;

/** Starting width from the header text and the kind of data the column holds. */
function initialWidth(col: ResultColumn): number {
  const byType: Record<string, number> = {
    bool: 84,
    int2: 96,
    int4: 104,
    int8: 128,
    numeric: 148,
    float8: 128,
    date: 116,
    timestamptz: 228,
    timestamp: 200,
    uuid: 304,
    jsonb: 280,
    json: 280,
  };
  const header = col.name.length * 7.4 + col.pgType.length * 6.2 + 40;
  return Math.min(MAX_INITIAL_COL, Math.max(byType[col.pgType] ?? 180, header));
}

type Props = {
  columns: ResultColumn[];
  rows: Cell[][];
  className?: string;
  "aria-label"?: string;
};

/**
 * Read-only, virtualised result grid. Only visible rows are in the DOM, so thousands of rows
 * scroll smoothly. Click a cell to select it; arrow keys move; Ctrl+C copies the raw value.
 */
export function DataGrid({ columns, rows, className, ...aria }: Props) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const [widths, setWidths] = useState(() => columns.map(initialWidth));
  const [selected, setSelected] = useState<{ row: number; col: number } | null>(null);
  const [copied, setCopied] = useState(false);

  const totalWidth = useMemo(() => ROW_NUMBER_WIDTH + widths.reduce((a, b) => a + b, 0), [widths]);

  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 16,
  });

  function startResize(index: number, e: PointerEvent<HTMLDivElement>) {
    e.preventDefault();
    e.stopPropagation();
    const startX = e.clientX;
    const startWidth = widths[index] ?? MIN_COL;
    const target = e.currentTarget;
    target.setPointerCapture(e.pointerId);
    const onMove = (ev: globalThis.PointerEvent) => {
      const next = Math.max(MIN_COL, startWidth + ev.clientX - startX);
      setWidths((w) => w.map((v, i) => (i === index ? next : v)));
    };
    const onUp = () => {
      target.removeEventListener("pointermove", onMove);
      target.removeEventListener("pointerup", onUp);
    };
    target.addEventListener("pointermove", onMove);
    target.addEventListener("pointerup", onUp);
  }

  function onKeyDown(e: KeyboardEvent<HTMLDivElement>) {
    if (!selected) return;
    const moves: Record<string, [number, number]> = {
      ArrowUp: [-1, 0],
      ArrowDown: [1, 0],
      ArrowLeft: [0, -1],
      ArrowRight: [0, 1],
    };
    const move = moves[e.key];
    if (move) {
      e.preventDefault();
      const row = Math.min(rows.length - 1, Math.max(0, selected.row + move[0]));
      const col = Math.min(columns.length - 1, Math.max(0, selected.col + move[1]));
      setSelected({ row, col });
      virtualizer.scrollToIndex(row);
      return;
    }
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "c") {
      const cell = rows[selected.row]?.[selected.col];
      if (!cell) return;
      e.preventDefault();
      copy(copyText(cell));
    }
  }

  function copy(text: string) {
    void copyToClipboard(text).then((ok) => {
      if (!ok) return; // Clipboard unavailable; the selection is still visible.
      setCopied(true);
      setTimeout(() => setCopied(false), 1200);
    });
  }

  const selectedRow = selected ? rows[selected.row] : undefined;
  const selectedCell = selected ? selectedRow?.[selected.col] : undefined;
  const selectedColumn = selected ? columns[selected.col] : undefined;

  const template = `${ROW_NUMBER_WIDTH}px ${widths.map((w) => `${w}px`).join(" ")}`;

  return (
    <div className={cn("relative min-h-0", className)}>
      <ContextMenu>
        <ContextMenuTrigger asChild>
          <div
            ref={scrollRef}
            tabIndex={0}
            role="grid"
            aria-rowcount={rows.length + 1}
            aria-colcount={columns.length}
            onKeyDown={onKeyDown}
            className="h-full overflow-auto outline-none focus-visible:shadow-[inset_0_0_0_1px_var(--primary)]"
            {...aria}
          >
            <div style={{ width: totalWidth, minWidth: "100%" }}>
              {/* Header */}
              <div
                role="row"
                className="sticky top-0 z-20 grid border-b border-border-strong bg-surface"
                style={{ gridTemplateColumns: template, width: totalWidth }}
              >
                <div className="sticky left-0 z-10 border-r border-border bg-surface" />
                {columns.map((col, i) => (
                  <div
                    key={`${col.name}-${i}`}
                    role="columnheader"
                    className="relative flex h-[34px] min-w-0 flex-col justify-center border-r border-border px-2.5"
                    title={`${col.name} · ${col.pgType}`}
                  >
                    <span className="truncate text-xs font-semibold">{col.name}</span>
                    <span className="truncate font-mono text-[10px] text-subtle-foreground">{col.pgType}</span>
                    <div
                      role="separator"
                      aria-orientation="vertical"
                      aria-label={`Resize ${col.name}`}
                      onPointerDown={(e) => startResize(i, e)}
                      // Invisible grab area; the resize cursor is the only cue.
                      className="absolute inset-y-0 -right-[3px] z-10 w-[7px] cursor-col-resize"
                    />
                  </div>
                ))}
              </div>

              {/* Body */}
              <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
                {virtualizer.getVirtualItems().map((v) => {
                  const row = rows[v.index] ?? [];
                  return (
                    <div
                      key={v.key}
                      role="row"
                      aria-rowindex={v.index + 2}
                      className="group absolute left-0 grid border-b border-border hover:bg-surface-2"
                      style={{
                        gridTemplateColumns: template,
                        width: totalWidth,
                        height: ROW_HEIGHT,
                        transform: `translateY(${v.start}px)`,
                      }}
                    >
                      <div className="tabular sticky left-0 z-10 flex items-center justify-end border-r border-border bg-surface pr-2 font-mono text-[10.5px] text-subtle-foreground group-hover:bg-surface-2">
                        {v.index + 1}
                      </div>
                      {row.map((cell, c) => {
                        const d = displayCell(cell);
                        const isSelected = selected?.row === v.index && selected.col === c;
                        return (
                          <div
                            key={c}
                            role="gridcell"
                            aria-selected={isSelected}
                            onMouseDown={() => setSelected({ row: v.index, col: c })}
                            className={cn(
                              "flex min-w-0 items-center border-r border-border px-2.5 text-[12.5px]",
                              d.align === "right" && "justify-end",
                              d.align === "center" && "justify-center",
                              d.mono && "tabular font-mono text-[12px]",
                              d.muted && "text-subtle-foreground italic",
                              isSelected && "bg-primary-soft shadow-[inset_0_0_0_1px_var(--primary)]",
                            )}
                          >
                            <span className="truncate">{d.text}</span>
                          </div>
                        );
                      })}
                    </div>
                  );
                })}
              </div>
            </div>
          </div>
        </ContextMenuTrigger>
        <ContextMenuContent>
          <ContextMenuItem
            disabled={!selectedCell}
            onSelect={() => selectedCell && copy(copyText(selectedCell))}
            shortcut="Ctrl+C"
          >
            <Copy /> Copy value
          </ContextMenuItem>
          <ContextMenuItem
            disabled={!selectedRow}
            onSelect={() => selectedRow && copy(asTsv([selectedRow.map(copyText)]))}
          >
            <Rows3 /> Copy row
          </ContextMenuItem>
          <ContextMenuItem
            disabled={!selectedRow}
            onSelect={() => selectedRow && copy(rowAsJson(columns, selectedRow))}
          >
            <Braces /> Copy row as JSON
          </ContextMenuItem>
          <ContextMenuItem
            disabled={!selected}
            onSelect={() => selected && copy(rows.map((r) => copyText(r[selected.col] ?? { t: "null" })).join("\n"))}
          >
            <TableProperties /> Copy column values
          </ContextMenuItem>
          <ContextMenuSeparator />
          <ContextMenuItem disabled={!selectedColumn} onSelect={() => selectedColumn && copy(selectedColumn.name)}>
            Copy column name
          </ContextMenuItem>
          <ContextMenuItem
            disabled={rows.length === 0}
            onSelect={() => copy(asTsv([columns.map((c) => c.name), ...rows.map((r) => r.map(copyText))]))}
          >
            Copy all rows with headers
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>
      {copied && (
        <div
          role="status"
          className="absolute right-3 bottom-3 border border-border-strong bg-surface px-2 py-1 text-xs shadow-lg"
        >
          Copied
        </div>
      )}
    </div>
  );
}
