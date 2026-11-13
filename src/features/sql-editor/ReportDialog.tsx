/**
 * Generate Report: pick a template, orientation and formats, then the backend asks where to
 * save, runs the query again in full and writes PDF and/or Excel.
 *
 * Preview opens a pane beside the form with the first PDF pages (drawn by the backend from the
 * first rows); it redraws when the template or orientation changes.
 *
 * Steps: form → generating → done (files with Open / Show in folder), or an error.
 */
import { useEffect, useMemo, useRef, useState } from "react";
import {
  AlertCircle,
  AlertTriangle,
  CheckCircle2,
  Eye,
  FileSpreadsheet,
  FileText,
  FolderOpen,
  Loader2,
  RectangleHorizontal,
  RectangleVertical,
  X,
  type LucideIcon,
} from "lucide-react";
import {
  client,
  REPORT_EXCEL_MAX_ROWS,
  REPORT_PDF_MAX_ROWS,
  REPORT_PREVIEW_ROWS,
  type ReportOutcome,
  type ReportPreviewOutcome,
  type ReportRequest,
  type VariableDef,
} from "@/services/client";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { formatDisplayDate } from "@/components/ui/date-picker";
import { Dialog, DialogContent } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Segmented, SegmentedItem } from "@/components/ui/segmented";
import { formatCount } from "@/lib/format";
import { cn } from "@/lib/utils";
import { BUILT_IN_TEMPLATES, normalizeDefinition, type TemplateEntry } from "@/features/templates/model";
import { Thumbnail } from "@/features/templates/TemplatesPage";
import { TYPE_LABELS, labelFromName, valueProblem } from "./model";

type Props = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  connectionId: string;
  /** The query name; the default report title. */
  queryName: string;
  sql: string;
  variables: VariableDef[];
};

type Phase =
  | { step: "form" }
  | { step: "generating" }
  | { step: "done"; outcome: Extract<ReportOutcome, { ok: true }> }
  | { step: "failed"; message: string; hint: string | null };

type Orientation = "portrait" | "landscape";

type PreviewResult = Extract<ReportPreviewOutcome, { ok: true }>;

type Preview =
  | { status: "loading"; last: PreviewResult | null }
  | { status: "ready"; outcome: PreviewResult }
  | { status: "failed"; message: string; hint: string | null };

/** The last choices, so the next report starts from them. */
const LAST_KEY = "intalix.report.last";
interface LastChoice {
  templateId: string;
  pdf: boolean;
  xlsx: boolean;
}

function readLast(): LastChoice | null {
  try {
    const v = JSON.parse(localStorage.getItem(LAST_KEY) ?? "null") as LastChoice | null;
    return v && typeof v.templateId === "string" ? v : null;
  } catch {
    return null;
  }
}

function saveLast(choice: LastChoice) {
  try {
    localStorage.setItem(LAST_KEY, JSON.stringify(choice));
  } catch {
    // Remembering is a convenience only.
  }
}

function fileSize(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

/** A filter as the report header will show it. */
function filterSummary(def: VariableDef): string {
  if (!def.required) return "All (filter off)";
  const v = def.default;
  if (v == null) return "—";
  if (Array.isArray(v)) return v.length > 3 ? `${v.slice(0, 3).join(", ")} +${v.length - 3} more` : v.join(", ");
  if (typeof v === "object") return `${formatDisplayDate(v.from)} – ${formatDisplayDate(v.to)}`;
  return def.type === "date" ? formatDisplayDate(v) : v;
}

export function ReportDialog({ open, onOpenChange, connectionId, queryName, sql, variables }: Props) {
  const [saved, setSaved] = useState<TemplateEntry[]>([]);
  const [title, setTitle] = useState(queryName);
  const [templateId, setTemplateId] = useState(() => readLast()?.templateId ?? BUILT_IN_TEMPLATES[0]?.id ?? "");
  const [pdf, setPdf] = useState(() => readLast()?.pdf ?? true);
  const [xlsx, setXlsx] = useState(() => readLast()?.xlsx ?? true);
  const [orientation, setOrientation] = useState<Orientation>("portrait");
  const [phase, setPhase] = useState<Phase>({ step: "form" });
  const [previewing, setPreviewing] = useState(false);
  const [preview, setPreview] = useState<Preview | null>(null);
  const abortRef = useRef<AbortController | null>(null);

  // Fresh form each time the dialog opens.
  useEffect(() => {
    if (!open) return;
    setTitle(queryName);
    setPhase({ step: "form" });
    setPreviewing(false);
    setPreview(null);
    let live = true;
    client.templates
      .list()
      .then((list) => {
        if (!live) return;
        setSaved(
          list.map((t) => ({
            id: t.id,
            name: t.name,
            builtIn: false,
            definition: normalizeDefinition(t.definition),
            updatedAt: t.updatedAt,
          })),
        );
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [open, queryName]);

  const templates = useMemo(() => [...saved, ...BUILT_IN_TEMPLATES], [saved]);
  const template = templates.find((t) => t.id === templateId) ?? templates[0];
  const missing = variables.filter((v) => v.required && valueProblem(v, v.default) != null);
  const ready = Boolean(sql.trim()) && missing.length === 0 && template != null;
  const canGenerate = ready && (pdf || xlsx);

  // Each template starts in its own orientation; the user can then switch.
  const templateOrientation = template?.definition.page.orientation ?? "portrait";
  useEffect(() => {
    setOrientation(templateOrientation);
  }, [template?.id, templateOrientation]);

  const request = useMemo<ReportRequest | null>(() => {
    if (!template) return null;
    const definition = template.definition;
    return {
      title: title.trim() || queryName,
      sql,
      variables,
      values: Object.fromEntries(variables.map((v) => [v.name, v.default])),
      template: { ...definition, page: { ...definition.page, orientation } },
      pdf,
      xlsx,
    };
  }, [template, title, queryName, sql, variables, orientation, pdf, xlsx]);

  // Redraw the preview when what it shows changes (the formats don't affect it).
  const previewKey =
    previewing && ready && request
      ? JSON.stringify([request.title, request.sql, request.variables, request.values, request.template])
      : null;
  const latestRequest = useRef(request);
  latestRequest.current = request;
  useEffect(() => {
    const body = latestRequest.current;
    if (!open || previewKey == null || !body) return;
    const controller = new AbortController();
    setPreview((p) => ({
      status: "loading",
      last: p?.status === "ready" ? p.outcome : p?.status === "loading" ? p.last : null,
    }));
    const timer = window.setTimeout(() => {
      client.reports
        .preview(connectionId, body, controller.signal)
        .then((outcome) => {
          if (outcome.ok) setPreview({ status: "ready", outcome });
          else setPreview({ status: "failed", message: outcome.message, hint: outcome.hint });
        })
        .catch((e: unknown) => {
          if (e instanceof DOMException && e.name === "AbortError") return;
          setPreview({
            status: "failed",
            message: e instanceof Error ? e.message : "The preview failed.",
            hint: null,
          });
        });
    }, 300);
    return () => {
      window.clearTimeout(timer);
      controller.abort();
    };
  }, [open, previewKey, connectionId]);

  async function generate() {
    if (!canGenerate || !template || !request) return;
    saveLast({ templateId: template.id, pdf, xlsx });
    const controller = new AbortController();
    abortRef.current = controller;
    setPhase({ step: "generating" });
    try {
      const outcome = await client.reports.generate(connectionId, request, controller.signal);
      if (outcome.ok) setPhase({ step: "done", outcome });
      else setPhase({ step: "failed", message: outcome.message, hint: outcome.hint });
    } catch (e) {
      // Closing the Save dialog (or Cancel) just returns to the form.
      if (e instanceof DOMException && e.name === "AbortError") setPhase({ step: "form" });
      else setPhase({ step: "failed", message: e instanceof Error ? e.message : "The report failed.", hint: null });
    } finally {
      abortRef.current = null;
    }
  }

  const close = (next: boolean) => {
    if (!next && phase.step === "generating") abortRef.current?.abort();
    onOpenChange(next);
  };

  const wide = previewing && phase.step === "form";

  return (
    <Dialog open={open} onOpenChange={close}>
      <DialogContent
        title="Generate Report"
        description="Runs the query again with all rows and saves it with a template."
        className={wide ? "h-[calc(100vh-32px)] w-[min(1240px,calc(100vw-32px))]" : "w-[min(600px,calc(100vw-32px))]"}
        bodyClassName={phase.step === "form" ? "flex flex-auto overflow-hidden" : undefined}
      >
        {phase.step === "form" && (
          <>
            <div className={cn("flex min-h-0 flex-col", wide ? "w-[380px] shrink-0 border-r border-border" : "w-full")}>
              <div className="min-h-0 flex-1 space-y-5 overflow-y-auto px-5 py-4">
                <Field
                  label="Report name"
                  htmlFor="report-title"
                  hint="Used as {title} in the template and as the file name."
                >
                  <Input
                    id="report-title"
                    value={title}
                    maxLength={120}
                    onChange={(e) => setTitle(e.target.value)}
                    autoFocus
                  />
                </Field>

                <div className="space-y-2">
                  <p className="text-[13px] font-medium">Template</p>
                  <div
                    className={cn(
                      "grid max-h-[220px] gap-1.5 overflow-y-auto pr-1",
                      wide ? "grid-cols-1" : "grid-cols-2",
                    )}
                    role="radiogroup"
                    aria-label="Template"
                  >
                    {templates.map((t) => {
                      const selected = t.id === template?.id;
                      return (
                        <button
                          key={t.id}
                          type="button"
                          role="radio"
                          aria-checked={selected}
                          onClick={() => setTemplateId(t.id)}
                          className={cn(
                            "flex items-center gap-2.5 border px-2.5 py-2 text-left outline-none focus-visible:shadow-[0_0_0_1px_var(--primary)]",
                            selected
                              ? "border-primary bg-primary-soft"
                              : "border-border bg-surface-2 hover:border-border-strong",
                          )}
                        >
                          <Thumbnail definition={t.definition} />
                          <span className="min-w-0">
                            <span className="block truncate text-[13px] font-medium">{t.name}</span>
                            <span className="block truncate text-[11px] text-subtle-foreground">
                              {t.builtIn ? "Built-in" : "My template"} · {t.definition.page.size}
                            </span>
                          </span>
                        </button>
                      );
                    })}
                  </div>
                </div>

                <div className="space-y-2">
                  <p className="text-[13px] font-medium" id="report-orientation">
                    Orientation
                  </p>
                  <Segmented
                    value={orientation}
                    onValueChange={(v) => setOrientation(v as Orientation)}
                    aria-labelledby="report-orientation"
                  >
                    <SegmentedItem value="portrait">
                      <RectangleVertical className="size-3.5" /> Portrait
                    </SegmentedItem>
                    <SegmentedItem value="landscape">
                      <RectangleHorizontal className="size-3.5" /> Landscape
                    </SegmentedItem>
                  </Segmented>
                </div>

                <div className="space-y-2">
                  <p className="text-[13px] font-medium">Formats</p>
                  <div className={cn("grid gap-1.5", wide ? "grid-cols-1" : "grid-cols-2")}>
                    <FormatOption
                      icon={FileText}
                      label="PDF"
                      detail={`Up to ${formatCount(REPORT_PDF_MAX_ROWS)} rows`}
                      checked={pdf}
                      onChange={setPdf}
                    />
                    <FormatOption
                      icon={FileSpreadsheet}
                      label="Excel"
                      detail={`Up to ${formatCount(REPORT_EXCEL_MAX_ROWS)} rows`}
                      checked={xlsx}
                      onChange={setXlsx}
                    />
                  </div>
                  {!pdf && !xlsx && <p className="text-xs text-danger">Choose at least one format.</p>}
                </div>

                {variables.length > 0 && (
                  <div className="space-y-2">
                    <p className="text-[13px] font-medium">Filters</p>
                    <dl className="divide-y divide-border border border-border">
                      {variables.map((v) => {
                        const problem = v.required && valueProblem(v, v.default) != null;
                        return (
                          <div key={v.name} className="flex items-center gap-3 px-3 py-1.5 text-xs">
                            <dt className="w-32 shrink-0 truncate text-muted-foreground" title={TYPE_LABELS[v.type]}>
                              {v.label.trim() || labelFromName(v.name)}
                            </dt>
                            <dd className={cn("min-w-0 truncate", problem ? "text-danger" : "text-foreground")}>
                              {problem ? "Needs a value" : filterSummary(v)}
                            </dd>
                          </div>
                        );
                      })}
                    </dl>
                    {missing.length > 0 && (
                      <p className="text-xs text-danger">Fill in the required filters in the Variables panel first.</p>
                    )}
                  </div>
                )}
              </div>

              <div className="flex shrink-0 justify-end gap-2 border-t border-border px-5 py-3">
                <Button onClick={() => close(false)}>Cancel</Button>
                <Button onClick={() => setPreviewing((p) => !p)} disabled={!ready} aria-pressed={previewing}>
                  <Eye /> {previewing ? "Hide preview" : "Preview"}
                </Button>
                <Button variant="primary" onClick={() => void generate()} disabled={!canGenerate}>
                  Generate…
                </Button>
              </div>
            </div>

            {wide && (
              <PreviewPane
                preview={preview}
                orientation={orientation}
                onLandscape={() => setOrientation("landscape")}
                onClose={() => setPreviewing(false)}
              />
            )}
          </>
        )}

        {phase.step === "generating" && (
          <div className="flex flex-col items-center gap-3 px-5 py-10 text-center">
            <Loader2 className="size-6 animate-spin text-primary-text" />
            <div>
              <p className="text-[13px] font-medium">Generating the report…</p>
              <p className="mt-1 text-xs text-muted-foreground">
                Choose where to save it, then the query runs with all rows and the files are written.
              </p>
            </div>
            <Button size="sm" onClick={() => abortRef.current?.abort()}>
              Cancel
            </Button>
          </div>
        )}

        {phase.step === "done" && (
          <div className="space-y-4 px-5 py-4">
            <div className="flex items-start gap-2.5">
              <CheckCircle2 className="mt-0.5 size-4 shrink-0 text-success" />
              <div>
                <p className="text-[13px] font-medium">Report ready</p>
                <p className="text-xs text-muted-foreground">
                  {formatCount(phase.outcome.rows)} {phase.outcome.rows === 1 ? "row" : "rows"} ·{" "}
                  {(phase.outcome.durationMs / 1000).toFixed(1)} s
                </p>
              </div>
            </div>
            <ul className="divide-y divide-border border border-border">
              {phase.outcome.files.map((f) => {
                const Icon = f.format === "pdf" ? FileText : FileSpreadsheet;
                return (
                  <li key={f.path} className="flex items-center gap-3 px-3 py-2.5">
                    <Icon className="size-5 shrink-0 text-muted-foreground" strokeWidth={1.75} />
                    <div className="min-w-0 flex-1">
                      <p className="truncate text-[13px] font-medium" title={f.path}>
                        {f.name}
                      </p>
                      <p className="text-[11px] text-subtle-foreground">
                        {f.format === "pdf" ? "PDF" : "Excel"} · {fileSize(f.sizeBytes)}
                        {f.pages != null && ` · ${f.pages} ${f.pages === 1 ? "page" : "pages"}`}
                      </p>
                    </div>
                    <Button size="sm" variant="primary" onClick={() => void client.reports.open(f.path, false)}>
                      Open
                    </Button>
                    <Button size="sm" onClick={() => void client.reports.open(f.path, true)}>
                      <FolderOpen /> Show in folder
                    </Button>
                  </li>
                );
              })}
            </ul>
            {phase.outcome.warnings.map((w) => (
              <p
                key={w}
                className="flex items-start gap-2 border border-warning/40 bg-warning-soft px-3 py-2 text-xs text-foreground"
              >
                <AlertTriangle className="mt-0.5 size-3.5 shrink-0 text-warning" />
                {w}
              </p>
            ))}
            <div className="flex justify-end gap-2 border-t border-border pt-4">
              <Button onClick={() => setPhase({ step: "form" })}>Generate another</Button>
              <Button variant="primary" onClick={() => close(false)}>
                Done
              </Button>
            </div>
          </div>
        )}

        {phase.step === "failed" && (
          <div className="space-y-4 px-5 py-4">
            <div className="flex items-start gap-3 border border-border bg-danger-soft p-3.5 shadow-[inset_2px_0_0_var(--danger)]">
              <AlertCircle className="mt-0.5 size-4 shrink-0 text-danger" />
              <div className="min-w-0 text-xs">
                <p className="text-[13px] font-medium">The report could not be generated</p>
                <p className="mt-1 break-words">{phase.message}</p>
                {phase.hint && <p className="mt-1 text-muted-foreground">{phase.hint}</p>}
              </div>
            </div>
            <div className="flex justify-end gap-2 border-t border-border pt-4">
              <Button onClick={() => close(false)}>Close</Button>
              <Button variant="primary" onClick={() => setPhase({ step: "form" })}>
                Back
              </Button>
            </div>
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}

/** The first PDF pages, drawn by the backend; dimmed while a newer version is drawn. */
function PreviewPane({
  preview,
  orientation,
  onLandscape,
  onClose,
}: {
  preview: Preview | null;
  orientation: Orientation;
  onLandscape: () => void;
  onClose: () => void;
}) {
  const shown = preview?.status === "ready" ? preview.outcome : preview?.status === "loading" ? preview.last : null;
  const loading = preview == null || preview.status === "loading";

  let summary = "";
  if (shown) {
    const pages = `${shown.pageCount} ${shown.pageCount === 1 ? "page" : "pages"}`;
    summary = shown.moreRows
      ? `First ${formatCount(REPORT_PREVIEW_ROWS)} rows · ${pages}`
      : `${formatCount(shown.rows)} ${shown.rows === 1 ? "row" : "rows"} · ${pages}`;
    if (shown.pages.length < shown.pageCount) summary += ` · first ${shown.pages.length} shown`;
  }

  return (
    <section aria-label="Preview" className="flex min-w-0 flex-1 flex-col bg-surface-2">
      <header className="flex h-10 shrink-0 items-center gap-2 border-b border-border px-4 text-xs">
        <span className="text-[13px] font-medium">Preview</span>
        {summary && <span className="truncate text-subtle-foreground">{summary}</span>}
        {loading && <Loader2 className="size-3.5 shrink-0 animate-spin text-primary-text" />}
        <button
          type="button"
          aria-label="Hide preview"
          onClick={onClose}
          className="ml-auto flex size-6 items-center justify-center text-muted-foreground outline-none hover:bg-surface hover:text-foreground focus-visible:shadow-[inset_0_0_0_1px_var(--primary)]"
        >
          <X className="size-3.5" />
        </button>
      </header>

      {shown?.crowded && (
        <div className="flex shrink-0 items-center gap-3 border-b border-warning/40 bg-warning-soft px-4 py-2 text-xs">
          <AlertTriangle className="size-3.5 shrink-0 text-warning" />
          <p className="flex-1">
            Too many columns for this page width, so some values wrap.
            {orientation === "portrait"
              ? " Landscape gives them more room."
              : " Selecting fewer columns in the query helps."}
          </p>
          {orientation === "portrait" && (
            <Button size="sm" onClick={onLandscape}>
              <RectangleHorizontal /> Use landscape
            </Button>
          )}
        </div>
      )}
      {shown?.shortened && !shown.crowded && (
        <p className="shrink-0 border-b border-border px-4 py-2 text-xs text-muted-foreground">
          Long text is shortened to fit the columns. The Excel file has it in full.
        </p>
      )}

      <div className="min-h-0 flex-1 overflow-auto p-5">
        {preview?.status === "failed" ? (
          <div className="mx-auto flex max-w-md items-start gap-3 border border-border bg-danger-soft p-3.5 text-xs shadow-[inset_2px_0_0_var(--danger)]">
            <AlertCircle className="mt-0.5 size-4 shrink-0 text-danger" />
            <div className="min-w-0">
              <p className="text-[13px] font-medium">The preview could not be drawn</p>
              <p className="mt-1 break-words">{preview.message}</p>
              {preview.hint && <p className="mt-1 text-muted-foreground">{preview.hint}</p>}
            </div>
          </div>
        ) : shown ? (
          <div className={cn("space-y-4 transition-opacity", loading && "opacity-50")}>
            {shown.pages.map((src, i) => (
              <img
                key={`${i}-${src.length}`}
                src={src}
                alt={`Page ${i + 1}`}
                className={cn(
                  "mx-auto block w-full border border-border bg-white shadow-[0_4px_16px_-6px_rgb(0_0_0/0.35)]",
                  orientation === "landscape" ? "max-w-[1000px]" : "max-w-[700px]",
                )}
              />
            ))}
          </div>
        ) : (
          <div className="flex h-full flex-col items-center justify-center gap-2 text-xs text-muted-foreground">
            <Loader2 className="size-5 animate-spin text-primary-text" />
            Drawing the first pages…
          </div>
        )}
      </div>
    </section>
  );
}

function FormatOption({
  icon: Icon,
  label,
  detail,
  checked,
  onChange,
}: {
  icon: LucideIcon;
  label: string;
  detail: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <label
      className={cn(
        "flex cursor-pointer items-center gap-2.5 border px-3 py-2.5",
        checked ? "border-primary bg-primary-soft" : "border-border bg-surface-2 hover:border-border-strong",
      )}
    >
      <Checkbox checked={checked} onCheckedChange={(v) => onChange(v === true)} />
      <Icon className="size-4 shrink-0 text-muted-foreground" strokeWidth={1.75} />
      <span className="min-w-0">
        <span className="block text-[13px] font-medium">{label}</span>
        <span className="block truncate text-[11px] text-subtle-foreground">{detail}</span>
      </span>
    </label>
  );
}
