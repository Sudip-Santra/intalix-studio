/**
 * Report template designer.
 *
 * Left: built-in and saved templates. Centre: a live, true-proportion preview on sample data.
 * Right: every setting the template controls. Built-in templates are read-only starting
 * points: changing one and saving creates the user's own copy.
 */
import { useEffect, useMemo, useRef, useState } from "react";
import { Copy, LayoutTemplate, Lock, MoreVertical, Plus, RotateCcw, Save, Trash2 } from "lucide-react";
import { client, type StoredTemplate } from "@/services/client";
import { Button } from "@/components/ui/button";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { Segmented, SegmentedItem } from "@/components/ui/segmented";
import { Tooltip } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import {
  BUILT_IN_TEMPLATES,
  blankTemplate,
  copyName,
  normalizeDefinition,
  uniqueName,
  type TemplateDefinition,
  type TemplateEntry,
} from "./model";
import { ReportPreview, pageSizePx } from "./ReportPreview";
import { TemplateInspector } from "./TemplateInspector";

/** What is open in the editor. `id` is empty for a template that has never been saved. */
interface Draft {
  id: string;
  builtIn: boolean;
  name: string;
  definition: TemplateDefinition;
}

const toEntry = (t: StoredTemplate): TemplateEntry => ({
  id: t.id,
  name: t.name,
  builtIn: false,
  definition: normalizeDefinition(t.definition),
  updatedAt: t.updatedAt,
});

const toDraft = (e: TemplateEntry): Draft => ({
  id: e.id,
  builtIn: e.builtIn,
  name: e.name,
  definition: structuredClone(e.definition),
});

const sameDraft = (a: Draft, b: Draft) =>
  a.name === b.name && JSON.stringify(a.definition) === JSON.stringify(b.definition);

type Zoom = "fit" | "75" | "100";

export function TemplatesPage({ active }: { active: boolean }) {
  const [saved, setSaved] = useState<TemplateEntry[]>([]);
  const [loadError, setLoadError] = useState<string | null>(null);
  const first = BUILT_IN_TEMPLATES[0] ?? blankTemplate();
  /** The version last loaded or saved, to detect unsaved changes. */
  const [baseline, setBaseline] = useState<Draft>(() => toDraft(first));
  const [draft, setDraft] = useState<Draft>(() => toDraft(first));
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [justSaved, setJustSaved] = useState(false);
  /** An action waiting on "discard unsaved changes?". */
  const [pendingSwitch, setPendingSwitch] = useState<(() => void) | null>(null);
  const [pendingDelete, setPendingDelete] = useState<TemplateEntry | null>(null);
  const [zoom, setZoom] = useState<Zoom>("fit");

  const changed = !sameDraft(draft, baseline);
  const unsaved = draft.id === "";

  useEffect(() => {
    let live = true;
    client.templates
      .list()
      .then((list) => live && setSaved(list.map(toEntry)))
      .catch((e: unknown) => live && setLoadError(e instanceof Error ? e.message : "Templates could not be loaded."));
    return () => {
      live = false;
    };
  }, []);

  const allNames = useMemo(() => [...BUILT_IN_TEMPLATES, ...saved].map((t) => t.name), [saved]);

  /** Runs `action` now, or after the user agrees to discard unsaved changes. */
  function guard(action: () => void) {
    if (changed) setPendingSwitch(() => action);
    else action();
  }

  function open(entry: TemplateEntry) {
    const d = toDraft(entry);
    setDraft(d);
    setBaseline(d);
    setSaveError(null);
  }

  function startNew() {
    const fresh = toDraft(blankTemplate(uniqueName("Untitled Template", allNames)));
    setDraft(fresh);
    setBaseline(fresh);
    setSaveError(null);
  }

  function duplicate(entry: TemplateEntry) {
    const copy: Draft = { ...toDraft(entry), id: "", builtIn: false, name: copyName(entry.name, allNames) };
    setDraft(copy);
    setBaseline(copy);
    setSaveError(null);
  }

  async function save() {
    if (saving) return;
    const name = draft.name.trim();
    if (!name) {
      setSaveError("Give the template a name.");
      return;
    }
    // Saving a built-in creates the user's own copy; built-ins themselves never change.
    const asNew = draft.builtIn || draft.id === "";
    const finalName =
      draft.builtIn && BUILT_IN_TEMPLATES.some((b) => b.name.toLowerCase() === name.toLowerCase())
        ? copyName(name, allNames)
        : name;
    setSaving(true);
    setSaveError(null);
    try {
      const stored = await client.templates.save(finalName, draft.definition, asNew ? undefined : draft.id);
      const entry = toEntry(stored);
      setSaved((list) => [entry, ...list.filter((t) => t.id !== entry.id)]);
      open(entry);
      setJustSaved(true);
    } catch (e) {
      setSaveError(e instanceof Error ? e.message : "The template could not be saved.");
    } finally {
      setSaving(false);
    }
  }

  async function remove(entry: TemplateEntry) {
    setPendingDelete(null);
    try {
      await client.templates.remove(entry.id);
      setSaved((list) => list.filter((t) => t.id !== entry.id));
      if (draft.id === entry.id) open(first);
    } catch (e) {
      setSaveError(e instanceof Error ? e.message : "The template could not be deleted.");
    }
  }

  // "Saved" confirmation fades after a moment.
  useEffect(() => {
    if (!justSaved) return;
    const t = setTimeout(() => setJustSaved(false), 1800);
    return () => clearTimeout(t);
  }, [justSaved]);

  // Ctrl+S saves while this page is showing.
  const saveRef = useRef(save);
  useEffect(() => {
    saveRef.current = save;
  });
  useEffect(() => {
    if (!active) return;
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
        e.preventDefault();
        void saveRef.current();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [active]);

  const selectedId = draft.id;

  return (
    <div className="flex h-full min-h-0">
      {/* ---------------------------------- template list ---------------------------------- */}
      <aside className="flex w-[260px] shrink-0 flex-col border-r border-border bg-surface">
        <div className="flex h-10 shrink-0 items-center justify-between border-b border-border pr-1.5 pl-4">
          <h2 className="text-[13px] font-semibold">Report Templates</h2>
          <Tooltip content="New template">
            <Button size="icon-sm" variant="ghost" aria-label="New template" onClick={() => guard(startNew)}>
              <Plus />
            </Button>
          </Tooltip>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto pb-3">
          <ListHeading>Built-in</ListHeading>
          {BUILT_IN_TEMPLATES.map((t) => (
            <TemplateItem
              key={t.id}
              entry={t}
              selected={selectedId === t.id}
              onOpen={() => guard(() => open(t))}
              onDuplicate={() => guard(() => duplicate(t))}
            />
          ))}

          <ListHeading>My Templates</ListHeading>
          {unsaved && (
            <TemplateItem
              entry={{ ...blankTemplate(draft.name || "Untitled"), definition: draft.definition }}
              selected
              unsaved
            />
          )}
          {saved.map((t) => (
            <TemplateItem
              key={t.id}
              entry={t}
              selected={selectedId === t.id}
              onOpen={() => guard(() => open(t))}
              onDuplicate={() => guard(() => duplicate(t))}
              onDelete={() => setPendingDelete(t)}
            />
          ))}
          {loadError && (
            <p role="alert" className="mx-4 mt-1 text-xs text-danger">
              {loadError}
            </p>
          )}
          {!loadError && saved.length === 0 && !unsaved && (
            <p className="mx-4 mt-1 text-xs leading-relaxed text-subtle-foreground">
              Templates you save appear here. Open a built-in one and change anything to start your own.
            </p>
          )}
        </div>
      </aside>

      {/* ------------------------------------ preview ------------------------------------ */}
      <section className="flex min-w-0 flex-1 flex-col bg-surface-sunken">
        <div className="flex h-10 shrink-0 items-center gap-3 border-b border-border bg-background px-3">
          <Input
            aria-label="Template name"
            value={draft.name}
            maxLength={80}
            onChange={(e) => setDraft({ ...draft, name: e.target.value })}
            className="h-7 max-w-[280px] border-transparent bg-transparent px-1.5 text-[13px] font-semibold hover:border-border focus-visible:bg-surface-2"
          />
          {draft.builtIn && (
            <span className="flex shrink-0 items-center gap-1 text-[11px] text-subtle-foreground">
              <Lock className="size-3" /> Built-in · saving creates your copy
            </span>
          )}
          {!draft.builtIn && (changed || unsaved) && (
            <span className="shrink-0 text-[11px] text-subtle-foreground">
              {unsaved ? "Not saved yet" : "Unsaved changes"}
            </span>
          )}
          {justSaved && !changed && <span className="shrink-0 text-[11px] text-success">Saved</span>}
          {saveError && (
            <span role="alert" className="min-w-0 truncate text-[11px] text-danger" title={saveError}>
              {saveError}
            </span>
          )}

          <div className="ml-auto flex shrink-0 items-center gap-2">
            <Segmented value={zoom} onValueChange={(v) => setZoom(v as Zoom)} aria-label="Zoom">
              <SegmentedItem value="fit" className="h-[26px] px-2 text-xs">
                Fit
              </SegmentedItem>
              <SegmentedItem value="75" className="h-[26px] px-2 text-xs">
                75%
              </SegmentedItem>
              <SegmentedItem value="100" className="h-[26px] px-2 text-xs">
                100%
              </SegmentedItem>
            </Segmented>
            {changed && (
              <Tooltip content="Discard changes">
                <Button size="icon-sm" variant="ghost" aria-label="Discard changes" onClick={() => setDraft(baseline)}>
                  <RotateCcw />
                </Button>
              </Tooltip>
            )}
            <Tooltip content="Save" shortcut="Ctrl+S">
              <Button
                size="sm"
                variant="primary"
                onClick={() => void save()}
                disabled={saving || (!changed && !draft.builtIn && !unsaved)}
              >
                <Save /> {draft.builtIn ? "Save As My Template" : "Save Template"}
              </Button>
            </Tooltip>
          </div>
        </div>

        <PreviewCanvas definition={draft.definition} zoom={zoom} />
      </section>

      {/* ----------------------------------- settings ----------------------------------- */}
      <aside className="flex w-[320px] shrink-0 flex-col border-l border-border bg-surface">
        <div className="flex h-10 shrink-0 items-center border-b border-border px-4">
          <h2 className="text-[13px] font-semibold">Design</h2>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto">
          <TemplateInspector
            definition={draft.definition}
            onChange={(definition) => setDraft({ ...draft, definition })}
          />
        </div>
      </aside>

      <ConfirmDialog
        open={pendingSwitch !== null}
        title="Discard unsaved changes?"
        description={`Your changes to "${draft.name}" haven't been saved. They will be lost.`}
        confirmLabel="Discard changes"
        tone="danger"
        onConfirm={() => {
          pendingSwitch?.();
          setPendingSwitch(null);
        }}
        onCancel={() => setPendingSwitch(null)}
      />
      <ConfirmDialog
        open={pendingDelete !== null}
        title={`Delete "${pendingDelete?.name ?? ""}"?`}
        description="This permanently removes the template from this computer. Reports already exported are not affected."
        confirmLabel="Delete template"
        tone="danger"
        onConfirm={() => pendingDelete && void remove(pendingDelete)}
        onCancel={() => setPendingDelete(null)}
      />
    </div>
  );
}

function ListHeading({ children }: { children: string }) {
  return <p className="px-4 pt-4 pb-1.5 text-[11px] font-semibold text-subtle-foreground">{children}</p>;
}

/** Tiny page sketch in the template's colours, so templates are recognisable at a glance. */
export function Thumbnail({ definition: d }: { definition: TemplateDefinition }) {
  const band = d.header.style === "band";
  const filled = d.table.headerStyle === "filled";
  const landscape = d.page.orientation === "landscape";
  return (
    <span
      aria-hidden
      className={cn(
        "flex shrink-0 flex-col gap-[2px] border border-black/10 bg-white p-[3px]",
        landscape ? "h-7 w-9" : "h-9 w-7",
      )}
    >
      <span
        className="h-[5px] w-full"
        style={{
          background: band ? d.theme.accent : "transparent",
          borderBottom: band ? undefined : `1.5px solid ${d.theme.accent}`,
        }}
      />
      <span
        className="h-[3px] w-full"
        style={{ background: filled ? d.theme.accent : `color-mix(in srgb, ${d.theme.accent} 25%, white)` }}
      />
      {[0, 1, 2].map((i) => (
        <span
          key={i}
          className="h-[2px] w-full"
          style={{ background: `color-mix(in srgb, ${d.theme.text} ${i % 2 ? 10 : 18}%, white)` }}
        />
      ))}
    </span>
  );
}

function TemplateItem({
  entry,
  selected,
  unsaved,
  onOpen,
  onDuplicate,
  onDelete,
}: {
  entry: TemplateEntry;
  selected: boolean;
  unsaved?: boolean;
  onOpen?: () => void;
  onDuplicate?: () => void;
  onDelete?: () => void;
}) {
  return (
    <div className={cn("group relative flex", selected && "bg-surface-2")}>
      {selected && <span aria-hidden className="absolute inset-y-0 left-0 w-0.5 bg-primary" />}
      <button
        type="button"
        onClick={onOpen}
        aria-current={selected ? "true" : undefined}
        className="flex min-w-0 flex-1 items-center gap-3 py-2 pr-1 pl-4 text-left outline-none hover:bg-surface-2 focus-visible:bg-surface-2"
      >
        <Thumbnail definition={entry.definition} />
        <span className="min-w-0 flex-1">
          <span className="block truncate text-[13px] font-medium">{entry.name}</span>
          <span className="block truncate text-[11px] text-subtle-foreground">
            {unsaved ? "Not saved yet" : entry.builtIn ? entry.definition.description : describe(entry.definition)}
          </span>
        </span>
      </button>
      {onDuplicate && (
        <DropdownMenu>
          <Tooltip content="More actions">
            <DropdownMenuTrigger asChild>
              <button
                type="button"
                aria-label={`Actions for ${entry.name}`}
                className="flex w-8 shrink-0 items-center justify-center text-muted-foreground opacity-0 outline-none group-hover:opacity-100 hover:text-foreground focus-visible:opacity-100 data-[state=open]:opacity-100"
              >
                <MoreVertical className="size-4" />
              </button>
            </DropdownMenuTrigger>
          </Tooltip>
          <DropdownMenuContent align="end" className="min-w-[170px]">
            <DropdownMenuItem onSelect={onDuplicate}>
              <Copy /> Duplicate
            </DropdownMenuItem>
            {onDelete && (
              <>
                <DropdownMenuSeparator />
                <DropdownMenuItem
                  onSelect={onDelete}
                  className="text-danger data-[highlighted]:bg-danger-soft data-[highlighted]:text-danger [&_svg]:text-danger!"
                >
                  <Trash2 /> Delete Template
                </DropdownMenuItem>
              </>
            )}
          </DropdownMenuContent>
        </DropdownMenu>
      )}
    </div>
  );
}

function describe(d: TemplateDefinition) {
  return `${d.page.size} ${d.page.orientation === "portrait" ? "Portrait" : "Landscape"}`;
}

/** Scrollable canvas that scales the true-size pages. */
function PreviewCanvas({ definition, zoom }: { definition: TemplateDefinition; zoom: Zoom }) {
  const ref = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const observer = new ResizeObserver(([entry]) => entry && setWidth(entry.contentRect.width));
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const page = pageSizePx(definition);
  const fitScale = width > 0 ? Math.min(1, Math.max(0.3, (width - 64) / page.width)) : 0.6;
  const scale = zoom === "fit" ? fitScale : Number(zoom) / 100;

  return (
    <div ref={ref} className="bg-grid min-h-0 flex-1 overflow-auto">
      <div className="flex min-w-max justify-center px-8 py-8">
        {width > 0 ? (
          <ReportPreview definition={definition} scale={scale} />
        ) : (
          <div className="flex items-center gap-2 text-xs text-muted-foreground">
            <LayoutTemplate className="size-4" /> Preparing preview…
          </div>
        )}
      </div>
    </div>
  );
}
