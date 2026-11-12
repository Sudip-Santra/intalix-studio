/**
 * Settings panel for a report template. Every change is applied immediately to the draft, so
 * the preview updates as you type; nothing is saved until the user clicks Save.
 */
import { useId, useRef, useState, type ReactNode } from "react";
import { ChevronDown, ImagePlus, Trash2 } from "lucide-react";
import { normalizeHex } from "@/app/accent";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Segmented, SegmentedItem } from "@/components/ui/segmented";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Tooltip } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import {
  DATE_FORMATS,
  FONTS,
  FONT_META,
  MAX_LOGO_BYTES,
  NUMBER_STYLES,
  PAGE_SIZES,
  type TemplateDefinition,
} from "./model";
import { TOKENS } from "./format";

type Section = Exclude<keyof TemplateDefinition, "schemaVersion" | "description">;

type InspectorProps = {
  definition: TemplateDefinition;
  onChange: (definition: TemplateDefinition) => void;
};

const ACCENT_SWATCHES = [
  "#1f3a5f",
  "#2563eb",
  "#0f766e",
  "#0f5132",
  "#7a1f2b",
  "#b91c1c",
  "#ea580c",
  "#7c3aed",
  "#18181b",
];

export function TemplateInspector({ definition: d, onChange }: InspectorProps) {
  function set<K extends Section>(section: K, patch: Partial<TemplateDefinition[K]>) {
    onChange({ ...d, [section]: { ...d[section], ...patch } });
  }

  return (
    <div className="flex flex-col">
      <Group title="Page" defaultOpen>
        <Row label="Paper size">
          <Choice
            value={d.page.size}
            onChange={(size) => set("page", { size })}
            options={PAGE_SIZES.map((s) => ({ value: s, label: s }))}
          />
        </Row>
        <Row label="Orientation">
          <Seg
            value={d.page.orientation}
            onChange={(orientation) => set("page", { orientation })}
            options={[
              { value: "portrait", label: "Portrait" },
              { value: "landscape", label: "Landscape" },
            ]}
          />
        </Row>
        <Row label="Margins">
          <Seg
            value={d.page.margins}
            onChange={(margins) => set("page", { margins })}
            options={[
              { value: "narrow", label: "Narrow" },
              { value: "normal", label: "Normal" },
              { value: "wide", label: "Wide" },
            ]}
          />
        </Row>
      </Group>

      <Group title="Colors & Fonts" defaultOpen>
        <Row label="Accent color">
          <ColorInput value={d.theme.accent} onChange={(accent) => set("theme", { accent })} />
          <div className="mt-2 flex flex-wrap gap-1.5">
            {ACCENT_SWATCHES.map((hex) => (
              <Tooltip key={hex} content={hex}>
                <button
                  type="button"
                  aria-label={`Use ${hex}`}
                  onClick={() => set("theme", { accent: hex })}
                  className={cn(
                    "size-5 border border-black/10 outline-none focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-primary",
                    d.theme.accent === hex && "outline-2 outline-offset-1 outline-foreground",
                  )}
                  style={{ background: hex }}
                />
              </Tooltip>
            ))}
          </div>
        </Row>
        <div className="grid grid-cols-2 gap-3">
          <Row label="Text color">
            <ColorInput value={d.theme.text} onChange={(text) => set("theme", { text })} />
          </Row>
          <Row label="Secondary text">
            <ColorInput value={d.theme.muted} onChange={(muted) => set("theme", { muted })} />
          </Row>
        </div>
        <div className="grid grid-cols-2 gap-3">
          <Row label="Body font">
            <Choice
              value={d.theme.font}
              onChange={(font) => set("theme", { font })}
              options={FONTS.map((f) => ({ value: f, label: FONT_META[f].label }))}
              renderLabel={(f) => <span style={{ fontFamily: FONT_META[f].stack }}>{FONT_META[f].label}</span>}
            />
          </Row>
          <Row label="Heading font">
            <Choice
              value={d.theme.headingFont}
              onChange={(headingFont) => set("theme", { headingFont })}
              options={FONTS.map((f) => ({ value: f, label: FONT_META[f].label }))}
              renderLabel={(f) => <span style={{ fontFamily: FONT_META[f].stack }}>{FONT_META[f].label}</span>}
            />
          </Row>
        </div>
        <Row label="Text size">
          <Choice
            value={String(d.theme.baseSize)}
            onChange={(v) => set("theme", { baseSize: Number(v) })}
            options={[7, 8, 9, 10, 11, 12, 13].map((n) => ({ value: String(n), label: `${n} pt` }))}
          />
        </Row>
      </Group>

      <Group title="Header" defaultOpen>
        <Toggle label="Show header" checked={d.header.visible} onChange={(visible) => set("header", { visible })} />
        {d.header.visible && (
          <>
            <Row label="Layout">
              <Seg
                value={d.header.layout}
                onChange={(layout) => set("header", { layout })}
                options={[
                  { value: "left", label: "Left" },
                  { value: "center", label: "Centered" },
                  { value: "split", label: "Split" },
                ]}
              />
            </Row>
            <Row label="Style">
              <Seg
                value={d.header.style}
                onChange={(style) => set("header", { style })}
                options={[
                  { value: "plain", label: "Plain" },
                  { value: "rule", label: "Rule" },
                  { value: "band", label: "Color Band" },
                ]}
              />
            </Row>
            <TextRow
              label="Organization"
              value={d.header.organization}
              maxLength={120}
              hint="Used by {organization} in the footer and for the logo initials."
              onChange={(organization) => set("header", { organization })}
            />
            <TextRow
              label="Title"
              value={d.header.title}
              maxLength={160}
              hint="{title} is replaced by the report name."
              onChange={(title) => set("header", { title })}
            />
            <TextRow
              label="Subtitle"
              value={d.header.subtitle}
              maxLength={200}
              onChange={(subtitle) => set("header", { subtitle })}
            />
            <Toggle
              label="Show logo"
              checked={d.header.showLogo}
              onChange={(showLogo) => set("header", { showLogo })}
            />
            {d.header.showLogo && (
              <>
                <LogoPicker logo={d.header.logo} onChange={(logo) => set("header", { logo })} />
                <Row label="Logo size">
                  <Seg
                    value={d.header.logoSize}
                    onChange={(logoSize) => set("header", { logoSize })}
                    options={[
                      { value: "small", label: "Small" },
                      { value: "medium", label: "Medium" },
                      { value: "large", label: "Large" },
                    ]}
                  />
                </Row>
              </>
            )}
            <Toggle
              label="Show report parameters"
              description="Filters such as the date range, under the title."
              checked={d.header.showParameters}
              onChange={(showParameters) => set("header", { showParameters })}
            />
          </>
        )}
      </Group>

      <Group title="Table" defaultOpen>
        <Row label="Column headings">
          <Choice
            value={d.table.headerStyle}
            onChange={(headerStyle) => set("table", { headerStyle })}
            options={[
              { value: "filled", label: "Filled with accent" },
              { value: "tinted", label: "Light accent" },
              { value: "underline", label: "Accent underline" },
              { value: "plain", label: "Plain" },
            ]}
          />
        </Row>
        <Row label="Gridlines">
          <Seg
            value={d.table.gridlines}
            onChange={(gridlines) => set("table", { gridlines })}
            options={[
              { value: "none", label: "None" },
              { value: "horizontal", label: "Rows" },
              { value: "all", label: "All" },
            ]}
          />
        </Row>
        <Row label="Row spacing">
          <Seg
            value={d.table.density}
            onChange={(density) => set("table", { density })}
            options={[
              { value: "compact", label: "Compact" },
              { value: "normal", label: "Normal" },
              { value: "relaxed", label: "Relaxed" },
            ]}
          />
        </Row>
        <Toggle label="Striped rows" checked={d.table.striped} onChange={(striped) => set("table", { striped })} />
        <Toggle
          label="Row numbers"
          checked={d.table.rowNumbers}
          onChange={(rowNumbers) => set("table", { rowNumbers })}
        />
        <Toggle
          label="Uppercase column headings"
          checked={d.table.uppercaseHeaders}
          onChange={(uppercaseHeaders) => set("table", { uppercaseHeaders })}
        />
        <Toggle
          label="Totals row"
          description="Sums numeric columns at the end of the report."
          checked={d.table.totalsRow}
          onChange={(totalsRow) => set("table", { totalsRow })}
        />
        {d.table.totalsRow && (
          <TextRow
            label="Totals label"
            value={d.table.totalsLabel}
            maxLength={40}
            onChange={(totalsLabel) => set("table", { totalsLabel })}
          />
        )}
      </Group>

      <Group title="Numbers & Dates">
        <Row label="Number format">
          <Choice
            value={d.formats.numbers}
            onChange={(numbers) => set("formats", { numbers })}
            options={NUMBER_STYLES.map((s) => ({
              value: s,
              label: s === "12,34,567.89" ? `${s}  (Indian)` : s.replace(/ /g, " "),
            }))}
          />
        </Row>
        <div className="grid grid-cols-2 gap-3">
          <Row label="Decimal places">
            <Choice
              value={String(d.formats.decimals)}
              onChange={(v) => set("formats", { decimals: Number(v) })}
              options={[0, 1, 2, 3, 4].map((n) => ({ value: String(n), label: String(n) }))}
            />
          </Row>
          <TextRow
            label="Currency symbol"
            value={d.formats.currency}
            maxLength={4}
            placeholder="None"
            onChange={(currency) => set("formats", { currency })}
          />
        </div>
        <Row label="Negative numbers">
          <Seg
            value={d.formats.negative}
            onChange={(negative) => set("formats", { negative })}
            options={[
              { value: "minus", label: "-1,234" },
              { value: "parentheses", label: "(1,234)" },
              { value: "red", label: "Red" },
            ]}
          />
        </Row>
        <Row label="Date format">
          <Choice
            value={d.formats.date}
            onChange={(date) => set("formats", { date })}
            options={DATE_FORMATS.map((f) => ({ value: f, label: f }))}
          />
        </Row>
      </Group>

      <Group title="Footer">
        <Toggle label="Show footer" checked={d.footer.visible} onChange={(visible) => set("footer", { visible })} />
        {d.footer.visible && (
          <>
            <Toggle
              label="Line above footer"
              checked={d.footer.style === "rule"}
              onChange={(on) => set("footer", { style: on ? "rule" : "plain" })}
            />
            <TextRow label="Left" value={d.footer.left} maxLength={200} onChange={(left) => set("footer", { left })} />
            <TextRow
              label="Center"
              value={d.footer.center}
              maxLength={200}
              onChange={(center) => set("footer", { center })}
            />
            <TextRow
              label="Right"
              value={d.footer.right}
              maxLength={200}
              onChange={(right) => set("footer", { right })}
            />
            <TokenHelp />
          </>
        )}
      </Group>

      <Group title="Watermark">
        <TextRow
          label="Text"
          value={d.watermark.text}
          maxLength={30}
          placeholder="e.g. DRAFT or CONFIDENTIAL"
          onChange={(text) => set("watermark", { text })}
        />
        {d.watermark.text.trim() && (
          <Row label={`Strength ${d.watermark.opacity}%`}>
            <input
              type="range"
              min={3}
              max={25}
              value={d.watermark.opacity}
              onChange={(e) => set("watermark", { opacity: Number(e.target.value) })}
              className="h-8 w-full cursor-pointer accent-(--primary)"
            />
          </Row>
        )}
      </Group>
    </div>
  );
}

/* ------------------------------------------------------------------------------------------
 * Building blocks
 * ---------------------------------------------------------------------------------------- */

function Group({
  title,
  defaultOpen = false,
  children,
}: {
  title: string;
  defaultOpen?: boolean;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(defaultOpen);
  const id = useId();
  return (
    <section className="border-b border-border">
      <button
        type="button"
        aria-expanded={open}
        aria-controls={id}
        onClick={() => setOpen((o) => !o)}
        className="flex h-10 w-full items-center justify-between px-4 text-[13px] font-semibold outline-none hover:bg-surface-2 focus-visible:bg-surface-2"
      >
        {title}
        <ChevronDown className={cn("size-3.5 text-muted-foreground transition-transform", open && "rotate-180")} />
      </button>
      {open && (
        <div id={id} className="flex flex-col gap-3.5 px-4 pt-1 pb-4">
          {children}
        </div>
      )}
    </section>
  );
}

function Row({
  label,
  htmlFor,
  hint,
  children,
}: {
  label: string;
  htmlFor?: string;
  hint?: string;
  children: ReactNode;
}) {
  return (
    <div className="flex min-w-0 flex-col gap-1.5">
      <Label htmlFor={htmlFor} className="text-xs font-medium text-muted-foreground">
        {label}
      </Label>
      {children}
      {hint && <p className="text-[11px] text-subtle-foreground">{hint}</p>}
    </div>
  );
}

function TextRow({
  label,
  value,
  onChange,
  maxLength,
  placeholder,
  hint,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  maxLength: number;
  placeholder?: string;
  hint?: string;
}) {
  const id = useId();
  return (
    <Row label={label} htmlFor={id} hint={hint}>
      <Input
        id={id}
        value={value}
        maxLength={maxLength}
        placeholder={placeholder}
        onChange={(e) => onChange(e.target.value)}
      />
    </Row>
  );
}

function Toggle({
  label,
  description,
  checked,
  onChange,
}: {
  label: string;
  description?: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  const id = useId();
  return (
    <div className="flex items-start gap-2.5">
      <Checkbox id={id} checked={checked} onCheckedChange={(v) => onChange(v === true)} className="mt-px" />
      <Label htmlFor={id} className="flex cursor-pointer flex-col gap-0.5 font-normal">
        <span className="text-[13px] text-foreground">{label}</span>
        {description && <span className="text-[11px] text-subtle-foreground">{description}</span>}
      </Label>
    </div>
  );
}

function Seg<T extends string>({
  value,
  onChange,
  options,
}: {
  value: T;
  onChange: (v: T) => void;
  options: { value: T; label: string }[];
}) {
  return (
    <Segmented value={value} onValueChange={(v) => onChange(v as T)}>
      {options.map((o) => (
        <SegmentedItem key={o.value} value={o.value} className="px-2 text-xs">
          {o.label}
        </SegmentedItem>
      ))}
    </Segmented>
  );
}

function Choice<T extends string>({
  value,
  onChange,
  options,
  renderLabel,
}: {
  value: T;
  onChange: (v: T) => void;
  options: { value: T; label: string }[];
  renderLabel?: (v: T) => ReactNode;
}) {
  return (
    <Select value={value} onValueChange={(v) => onChange(v as T)}>
      <SelectTrigger>
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {options.map((o) => (
          <SelectItem key={o.value} value={o.value}>
            {renderLabel ? renderLabel(o.value) : o.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}

/** Native colour picker plus a hex field; invalid hex text is kept locally until it's valid. */
function ColorInput({ value, onChange }: { value: string; onChange: (hex: string) => void }) {
  const [text, setText] = useState(value);
  const [synced, setSynced] = useState(value);
  if (synced !== value) {
    setSynced(value);
    setText(value);
  }
  const valid = normalizeHex(text) !== null;
  return (
    <div className="flex">
      <label
        className="relative size-8 shrink-0 cursor-pointer border border-r-0 border-border"
        style={{ background: value }}
        title="Pick a color"
      >
        <input
          type="color"
          value={value}
          onChange={(e) => onChange(e.target.value)}
          className="absolute inset-0 size-full cursor-pointer opacity-0"
          aria-label="Pick a color"
        />
      </label>
      <Input
        value={text}
        aria-invalid={!valid || undefined}
        maxLength={7}
        className="font-mono text-xs uppercase"
        onChange={(e) => {
          setText(e.target.value);
          const hex = normalizeHex(e.target.value);
          if (hex) onChange(hex);
        }}
        onBlur={() => setText(value)}
      />
    </div>
  );
}

function LogoPicker({ logo, onChange }: { logo: string | null; onChange: (logo: string | null) => void }) {
  const inputRef = useRef<HTMLInputElement>(null);
  const [error, setError] = useState<string | null>(null);

  function pick(file: File | undefined) {
    setError(null);
    if (!file) return;
    if (!["image/png", "image/jpeg", "image/webp"].includes(file.type)) {
      setError("Choose a PNG, JPEG or WebP image.");
      return;
    }
    if (file.size > MAX_LOGO_BYTES) {
      setError("That image is over 1 MB. Choose a smaller one.");
      return;
    }
    const reader = new FileReader();
    reader.onload = () => typeof reader.result === "string" && onChange(reader.result);
    reader.onerror = () => setError("The image could not be read.");
    reader.readAsDataURL(file);
  }

  return (
    <Row label="Logo image">
      <div className="flex items-center gap-2">
        <div className="flex h-12 w-24 shrink-0 items-center justify-center border border-border bg-white p-1">
          {logo ? (
            <img src={logo} alt="Logo" className="max-h-full max-w-full object-contain" />
          ) : (
            <span className="text-[10px] text-neutral-500">Monogram</span>
          )}
        </div>
        <Button size="sm" onClick={() => inputRef.current?.click()}>
          <ImagePlus /> {logo ? "Replace" : "Upload"}
        </Button>
        {logo && (
          <Tooltip content="Remove logo">
            <Button size="icon-sm" variant="ghost" aria-label="Remove logo" onClick={() => onChange(null)}>
              <Trash2 />
            </Button>
          </Tooltip>
        )}
        <input
          ref={inputRef}
          type="file"
          accept="image/png,image/jpeg,image/webp"
          className="hidden"
          onChange={(e) => {
            pick(e.target.files?.[0]);
            e.target.value = "";
          }}
        />
      </div>
      {error ? (
        <p role="alert" className="text-[11px] text-danger">
          {error}
        </p>
      ) : (
        <p className="text-[11px] text-subtle-foreground">
          PNG, JPEG or WebP, up to 1 MB. Without one, initials are shown.
        </p>
      )}
    </Row>
  );
}

function TokenHelp() {
  return (
    <div className="border border-border bg-surface-2 px-3 py-2.5">
      <p className="text-[11px] font-medium text-muted-foreground">Placeholders you can type</p>
      <dl className="mt-1.5 grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 text-[11px]">
        {TOKENS.map((t) => (
          <div key={t.token} className="contents">
            <dt className="font-mono text-foreground">{t.token}</dt>
            <dd className="text-subtle-foreground">{t.description}</dd>
          </div>
        ))}
      </dl>
    </div>
  );
}
