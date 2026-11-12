/**
 * Live preview of a template applied to the sample dataset.
 *
 * Pages are laid out at true size (millimetres converted to CSS pixels) and scaled to fit, so
 * proportions match the exported document. Rows per page are measured from the real layout:
 * the first page and the last page are shown, and the page count in the footer is computed
 * from how many rows actually fit with the chosen fonts, density and header.
 */
import { useLayoutEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { contrast } from "@/app/accent";
import { FONT_META, MARGIN_MM, PAGE_MM, type TemplateDefinition } from "./model";
import { fillTokens, formatDate, formatDecimal, type TokenValues } from "./format";
import {
  SAMPLE_COLUMNS,
  SAMPLE_PARAMETERS,
  SAMPLE_ROWS,
  SAMPLE_TITLE,
  SAMPLE_TOTALS,
  type SampleColumn,
  type SampleRow,
} from "./sample";

const PX_PER_MM = 96 / 25.4;
const GENERATED = { date: "2026-03-31", time: "18:30" };

export function pageSizePx(def: TemplateDefinition) {
  const { w, h } = PAGE_MM[def.page.size];
  const portrait = def.page.orientation === "portrait";
  return { width: (portrait ? w : h) * PX_PER_MM, height: (portrait ? h : w) * PX_PER_MM };
}

/* ------------------------------------------------------------------------------------------
 * Colour helpers
 * ---------------------------------------------------------------------------------------- */

const rgb = (hex: string): [number, number, number] => {
  const n = Number.parseInt(hex.slice(1), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
};

/** White or near-black, whichever reads better on `hex`. */
function readableOn(hex: string): string {
  return contrast(rgb(hex), [255, 255, 255]) >= contrast(rgb(hex), [20, 20, 22]) ? "#ffffff" : "#141416";
}

const tint = (hex: string, percent: number) => `color-mix(in srgb, ${hex} ${percent}%, white)`;

/* ------------------------------------------------------------------------------------------
 * Pagination
 * ---------------------------------------------------------------------------------------- */

interface Geometry {
  /** Height available to table body rows. */
  body: number;
  row: number;
  totals: number;
}

const sameGeometry = (a: Geometry | null, b: Geometry) =>
  a !== null && a.body === b.body && a.row === b.row && a.totals === b.totals;

function fits(available: number, row: number) {
  // One pixel of slack absorbs sub-pixel rounding in the measured heights.
  return row > 0 ? Math.max(1, Math.floor((available - 1) / row)) : 1;
}

/* ------------------------------------------------------------------------------------------
 * Preview
 * ---------------------------------------------------------------------------------------- */

type ReportPreviewProps = {
  definition: TemplateDefinition;
  /** Scale applied to the true-size page. */
  scale: number;
};

export function ReportPreview({ definition, scale }: ReportPreviewProps) {
  const [first, setFirst] = useState<Geometry | null>(null);
  const [last, setLast] = useState<Geometry | null>(null);
  const total = SAMPLE_ROWS.length;

  const firstCount = first ? Math.min(total, fits(first.body, first.row)) : 24;
  const rest = total - firstCount;
  const lastCapacity = last ? fits(last.body - last.totals, last.row) : 30;
  const middleCapacity = last ? fits(last.body, last.row) : 31;
  const middlePages = rest <= lastCapacity ? 0 : Math.ceil((rest - lastCapacity) / middleCapacity);
  const singlePage = rest <= 0;
  const pages = singlePage ? 1 : 2 + middlePages;
  const lastCount = singlePage ? 0 : rest - middlePages * middleCapacity;

  const { width, height } = pageSizePx(definition);

  return (
    <div className="flex flex-col items-center gap-6">
      <ScaledPage width={width} height={height} scale={scale} label={`Page 1 of ${pages}`}>
        <Page
          definition={definition}
          kind="first"
          rows={SAMPLE_ROWS.slice(0, singlePage ? total : firstCount)}
          firstIndex={0}
          showTotals={singlePage}
          page={1}
          pages={pages}
          onGeometry={(g) => setFirst((prev) => (sameGeometry(prev, g) ? prev : g))}
        />
      </ScaledPage>
      {!singlePage && (
        <ScaledPage width={width} height={height} scale={scale} label={`Page ${pages} of ${pages}`}>
          <Page
            definition={definition}
            kind="last"
            rows={SAMPLE_ROWS.slice(total - lastCount)}
            firstIndex={total - lastCount}
            showTotals
            page={pages}
            pages={pages}
            onGeometry={(g) => setLast((prev) => (sameGeometry(prev, g) ? prev : g))}
          />
        </ScaledPage>
      )}
    </div>
  );
}

function ScaledPage({
  width,
  height,
  scale,
  label,
  children,
}: {
  width: number;
  height: number;
  scale: number;
  label: string;
  children: ReactNode;
}) {
  return (
    <figure className="flex flex-col items-center gap-2">
      <div
        className="relative shrink-0 shadow-[0_1px_2px_rgb(0_0_0/0.12),0_12px_32px_-8px_rgb(0_0_0/0.28)]"
        style={{ width: width * scale, height: height * scale }}
      >
        <div className="absolute top-0 left-0 origin-top-left" style={{ width, height, transform: `scale(${scale})` }}>
          {children}
        </div>
      </div>
      <figcaption className="text-[11px] text-subtle-foreground">{label}</figcaption>
    </figure>
  );
}

/* ------------------------------------------------------------------------------------------
 * One page
 * ---------------------------------------------------------------------------------------- */

type PageProps = {
  definition: TemplateDefinition;
  kind: "first" | "last";
  rows: SampleRow[];
  firstIndex: number;
  showTotals: boolean;
  page: number;
  pages: number;
  onGeometry: (g: Geometry) => void;
};

function Page({ definition: d, kind, rows, firstIndex, showTotals, page, pages, onGeometry }: PageProps) {
  const areaRef = useRef<HTMLDivElement>(null);
  const theadRef = useRef<HTMLTableSectionElement>(null);
  const rowRef = useRef<HTMLTableRowElement>(null);
  const totalsRef = useRef<HTMLTableRowElement>(null);
  const probeTotalsRef = useRef<HTMLTableRowElement>(null);

  // Measure on every render; the parent ignores unchanged results, so this cannot loop.
  useLayoutEffect(() => {
    const area = areaRef.current;
    if (!area) return;
    onGeometry({
      body: area.clientHeight - (theadRef.current?.offsetHeight ?? 0),
      row: rowRef.current?.offsetHeight ?? 0,
      totals: (totalsRef.current ?? probeTotalsRef.current)?.offsetHeight ?? 0,
    });
  });

  const margin = MARGIN_MM[d.page.margins] * PX_PER_MM;
  const tokens: TokenValues = {
    title: SAMPLE_TITLE,
    organization: d.header.organization,
    date: formatDate(GENERATED.date, d.formats.date),
    time: GENERATED.time,
    page,
    pages,
  };

  const pageStyle: CSSProperties = {
    padding: margin,
    fontFamily: FONT_META[d.theme.font].stack,
    fontSize: `${d.theme.baseSize}pt`,
    color: d.theme.text,
    lineHeight: 1.35,
  };

  return (
    <div className="relative flex h-full w-full flex-col overflow-hidden bg-white" style={pageStyle}>
      {d.watermark.text.trim() && <Watermark definition={d} />}

      {kind === "first" && d.header.visible && <Header definition={d} tokens={tokens} margin={margin} />}
      {kind === "last" && d.header.visible && (
        <p className="mb-[3mm] flex justify-between" style={{ fontSize: "0.85em", color: d.theme.muted }}>
          <span>{fillTokens(d.header.title, tokens)}</span>
          <span>Continued</span>
        </p>
      )}

      <div ref={areaRef} className="relative mt-[3mm] min-h-0 flex-1 overflow-hidden">
        <DataTable
          definition={d}
          rows={rows}
          firstIndex={firstIndex}
          showTotals={showTotals}
          theadRef={theadRef}
          rowRef={rowRef}
          totalsRef={totalsRef}
        />
        {/* Measures the totals row height on pages that don't show one. */}
        {!showTotals && (
          <table aria-hidden className="pointer-events-none invisible absolute top-0 left-0 w-full">
            <tbody>
              <TotalsRow definition={d} rowRef={probeTotalsRef} />
            </tbody>
          </table>
        )}
      </div>

      {d.footer.visible && <Footer definition={d} tokens={tokens} />}
    </div>
  );
}

function Watermark({ definition: d }: { definition: TemplateDefinition }) {
  return (
    <div className="pointer-events-none absolute inset-0 z-10 flex items-center justify-center overflow-hidden">
      <span
        className="font-bold whitespace-nowrap"
        style={{
          transform: "rotate(-32deg)",
          fontSize: "64pt",
          letterSpacing: "0.04em",
          color: d.theme.text,
          opacity: d.watermark.opacity / 100,
          fontFamily: FONT_META[d.theme.headingFont].stack,
        }}
      >
        {d.watermark.text}
      </span>
    </div>
  );
}

/* ------------------------------------------------------------------------------------------
 * Header
 * ---------------------------------------------------------------------------------------- */

const LOGO_PX = { small: 30, medium: 42, large: 58 } as const;

function initials(name: string) {
  const words = name.trim().split(/\s+/).filter(Boolean);
  return (
    words.length > 1 ? `${words[0]?.[0] ?? ""}${words[1]?.[0] ?? ""}` : (words[0] ?? "").slice(0, 2)
  ).toUpperCase();
}

function Logo({ definition: d, onBand }: { definition: TemplateDefinition; onBand: boolean }) {
  const size = LOGO_PX[d.header.logoSize];
  if (d.header.logo) {
    return (
      <img
        src={d.header.logo}
        alt=""
        className="block shrink-0 object-contain"
        style={{ height: size, maxWidth: size * 3.2 }}
      />
    );
  }
  const bg = onBand ? readableOn(d.theme.accent) : d.theme.accent;
  return (
    <span
      className="flex shrink-0 items-center justify-center font-bold"
      style={{
        width: size,
        height: size,
        background: bg,
        color: onBand ? d.theme.accent : readableOn(d.theme.accent),
        fontSize: size * 0.38,
        fontFamily: FONT_META[d.theme.headingFont].stack,
      }}
    >
      {initials(d.header.organization) || "IS"}
    </span>
  );
}

function Header({
  definition: d,
  tokens,
  margin,
}: {
  definition: TemplateDefinition;
  tokens: TokenValues;
  margin: number;
}) {
  const band = d.header.style === "band";
  const fg = band ? readableOn(d.theme.accent) : d.theme.text;
  const muted = band ? fg : d.theme.muted;
  const center = d.header.layout === "center";
  const split = d.header.layout === "split";
  const showLogo = d.header.showLogo;

  const text = (
    <div className={center ? "text-center" : "min-w-0"}>
      <h1
        className="font-bold"
        style={{
          fontFamily: FONT_META[d.theme.headingFont].stack,
          fontSize: "2.05em",
          lineHeight: 1.15,
          marginTop: "0.15em",
          color: fg,
        }}
      >
        {fillTokens(d.header.title, tokens) || SAMPLE_TITLE}
      </h1>
      {d.header.subtitle.trim() && (
        <p style={{ color: muted, opacity: band ? 0.8 : 1, marginTop: "0.25em" }}>
          {fillTokens(d.header.subtitle, tokens)}
        </p>
      )}
    </div>
  );

  return (
    <header
      style={{
        ...(band
          ? {
              background: d.theme.accent,
              margin: `-${margin}px -${margin}px 0`,
              padding: `${margin * 0.75}px ${margin}px`,
            }
          : { paddingBottom: "3mm" }),
        borderBottom: d.header.style === "rule" ? `2px solid ${d.theme.accent}` : undefined,
      }}
    >
      <div
        className={
          center
            ? "flex flex-col items-center gap-[3mm]"
            : split
              ? "flex items-center justify-between gap-[6mm]"
              : "flex items-center gap-[4mm]"
        }
      >
        {showLogo && !split && <Logo definition={d} onBand={band} />}
        {text}
        {showLogo && split && <Logo definition={d} onBand={band} />}
      </div>
      {d.header.showParameters && (
        <dl
          className={center ? "flex justify-center gap-[6mm]" : "flex gap-[6mm]"}
          style={{ marginTop: "2mm", fontSize: "0.92em", color: muted, opacity: band ? 0.9 : 1 }}
        >
          {SAMPLE_PARAMETERS.map((p) => (
            <div key={p.label} className="flex gap-[1.5mm]">
              <dt>{p.label}:</dt>
              <dd className="font-semibold" style={{ color: fg }}>
                {p.value.includes("|")
                  ? p.value
                      .split("|")
                      .map((v) => formatDate(v, d.formats.date))
                      .join(" – ")
                  : p.value}
              </dd>
            </div>
          ))}
        </dl>
      )}
    </header>
  );
}

/* ------------------------------------------------------------------------------------------
 * Table
 * ---------------------------------------------------------------------------------------- */

const DENSITY_PAD = { compact: "1.1mm", normal: "1.9mm", relaxed: "2.8mm" } as const;

function cellText(d: TemplateDefinition, col: SampleColumn, value: string): { text: string; negative: boolean } {
  const f = d.formats;
  switch (col.kind) {
    case "date":
      return { text: formatDate(value, f.date), negative: false };
    case "int":
      return formatDecimal(value, { style: f.numbers, decimals: 0, negative: f.negative });
    case "money":
      return formatDecimal(value, {
        style: f.numbers,
        decimals: f.decimals,
        negative: f.negative,
        currency: f.currency,
      });
    case "decimal":
      return formatDecimal(value, { style: f.numbers, decimals: Math.min(f.decimals, 2), negative: f.negative });
    case "text":
      return { text: value, negative: false };
  }
}

const isNumeric = (col: SampleColumn) => col.kind === "int" || col.kind === "money" || col.kind === "decimal";

function tableStyles(d: TemplateDefinition) {
  const t = d.table;
  const line = tint(d.theme.text, 14);
  const pad = DENSITY_PAD[t.density];
  const cell = (numeric: boolean): CSSProperties => ({
    padding: `${pad} 2mm`,
    textAlign: numeric ? "right" : "left",
    borderBottom: t.gridlines !== "none" ? `1px solid ${line}` : undefined,
    borderRight: t.gridlines === "all" ? `1px solid ${line}` : undefined,
    whiteSpace: "nowrap",
    overflow: "hidden",
    textOverflow: "ellipsis",
    fontVariantNumeric: numeric ? "tabular-nums" : undefined,
  });

  let head: CSSProperties;
  switch (t.headerStyle) {
    case "filled":
      head = { background: d.theme.accent, color: readableOn(d.theme.accent) };
      break;
    case "tinted":
      head = {
        background: tint(d.theme.accent, 12),
        color: d.theme.text,
        borderBottom: `1.5px solid ${tint(d.theme.accent, 45)}`,
      };
      break;
    case "underline":
      head = { color: d.theme.text, borderBottom: `1.5px solid ${d.theme.accent}` };
      break;
    case "plain":
      head = { color: d.theme.text, borderBottom: `1.5px solid ${d.theme.text}` };
      break;
  }
  return { cell, head, line };
}

function TotalsRow({
  definition: d,
  rowRef,
}: {
  definition: TemplateDefinition;
  rowRef?: React.Ref<HTMLTableRowElement>;
}) {
  const { cell } = tableStyles(d);
  const style: CSSProperties = {
    fontWeight: 700,
    background: d.table.headerStyle === "plain" ? undefined : tint(d.theme.accent, 7),
    borderTop: `1.5px solid ${d.theme.text}`,
  };
  return (
    <tr ref={rowRef} style={style}>
      {d.table.rowNumbers && <td style={cell(true)} />}
      {SAMPLE_COLUMNS.map((col, i) => {
        const total = SAMPLE_TOTALS[col.key];
        return (
          <td key={col.key} style={{ ...cell(isNumeric(col)), borderBottom: "none" }}>
            {i === 0 ? d.table.totalsLabel : total !== undefined ? cellText(d, col, total).text : ""}
          </td>
        );
      })}
    </tr>
  );
}

function DataTable({
  definition: d,
  rows,
  firstIndex,
  showTotals,
  theadRef,
  rowRef,
  totalsRef,
}: {
  definition: TemplateDefinition;
  rows: SampleRow[];
  firstIndex: number;
  showTotals: boolean;
  theadRef: React.Ref<HTMLTableSectionElement>;
  rowRef: React.Ref<HTMLTableRowElement>;
  totalsRef: React.Ref<HTMLTableRowElement>;
}) {
  const { cell, head } = tableStyles(d);
  const t = d.table;
  const totalWidth = SAMPLE_COLUMNS.reduce((a, c) => a + c.width, 0) + (t.rowNumbers ? 5 : 0);
  const stripe = tint(d.theme.accent, 5);

  return (
    <table className="w-full border-collapse" style={{ tableLayout: "fixed" }}>
      <colgroup>
        {t.rowNumbers && <col style={{ width: `${(5 / totalWidth) * 100}%` }} />}
        {SAMPLE_COLUMNS.map((c) => (
          <col key={c.key} style={{ width: `${(c.width / totalWidth) * 100}%` }} />
        ))}
      </colgroup>
      <thead ref={theadRef}>
        <tr style={head}>
          {t.rowNumbers && <th style={{ ...cell(true), ...head, fontWeight: 600 }}>#</th>}
          {SAMPLE_COLUMNS.map((c) => (
            <th
              key={c.key}
              style={{
                ...cell(isNumeric(c)),
                ...head,
                fontWeight: 600,
                textTransform: t.uppercaseHeaders ? "uppercase" : undefined,
                fontSize: t.uppercaseHeaders ? "0.88em" : undefined,
                letterSpacing: t.uppercaseHeaders ? "0.03em" : undefined,
              }}
            >
              {c.label}
            </th>
          ))}
        </tr>
      </thead>
      <tbody>
        {rows.map((r, i) => (
          <tr
            key={r.invoice}
            ref={i === 0 ? rowRef : undefined}
            style={{ background: t.striped && (firstIndex + i) % 2 === 1 ? stripe : undefined }}
          >
            {t.rowNumbers && <td style={{ ...cell(true), color: d.theme.muted }}>{firstIndex + i + 1}</td>}
            {SAMPLE_COLUMNS.map((c) => {
              const v = cellText(d, c, r[c.key]);
              return (
                <td
                  key={c.key}
                  style={{
                    ...cell(isNumeric(c)),
                    color: v.negative && d.formats.negative === "red" ? "#c81e1e" : undefined,
                  }}
                >
                  {v.text}
                </td>
              );
            })}
          </tr>
        ))}
        {showTotals && t.totalsRow && <TotalsRow definition={d} rowRef={totalsRef} />}
      </tbody>
    </table>
  );
}

/* ------------------------------------------------------------------------------------------
 * Footer
 * ---------------------------------------------------------------------------------------- */

function Footer({ definition: d, tokens }: { definition: TemplateDefinition; tokens: TokenValues }) {
  return (
    <footer
      className="grid shrink-0 grid-cols-3 gap-[4mm]"
      style={{
        marginTop: "3mm",
        paddingTop: "2mm",
        borderTop: d.footer.style === "rule" ? `1px solid ${tint(d.theme.text, 22)}` : undefined,
        color: d.theme.muted,
        fontSize: "0.85em",
      }}
    >
      <span className="truncate">{fillTokens(d.footer.left, tokens)}</span>
      <span className="truncate text-center">{fillTokens(d.footer.center, tokens)}</span>
      <span className="truncate text-right">{fillTokens(d.footer.right, tokens)}</span>
    </footer>
  );
}
