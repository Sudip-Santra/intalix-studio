import { useEffect, useState } from "react";
import { Check, RotateCcw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogClose, DialogContent } from "@/components/ui/dialog";
import { Tooltip } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import { ACCENT_PRESETS, DEFAULT_ACCENT, normalizeHex, setAccent, useAccent } from "./accent";

type Props = { open: boolean; onOpenChange: (open: boolean) => void };

/** Accent colour picker. Every choice applies (and is saved) immediately. */
export function ColorThemeDialog({ open, onOpenChange }: Props) {
  const accent = useAccent();
  const [draft, setDraft] = useState(accent);
  const isPreset = ACCENT_PRESETS.some((p) => p.hex === accent);

  // Keep the hex field in step with changes made elsewhere (swatches, reset).
  useEffect(() => setDraft(accent), [accent]);

  const draftValid = normalizeHex(draft) !== null;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent title="Color Theme" description="The accent colour used for buttons, highlights and links.">
        <div className="flex flex-col gap-6 px-5 py-5">
          <section>
            <h3 className="text-[13px] font-semibold">Presets</h3>
            <div role="radiogroup" aria-label="Preset colours" className="mt-2.5 grid grid-cols-6 gap-2">
              {ACCENT_PRESETS.map((p) => {
                const selected = p.hex === accent;
                return (
                  <Tooltip key={p.hex} content={p.name} delay={0}>
                    <button
                      type="button"
                      role="radio"
                      aria-checked={selected}
                      aria-label={p.name}
                      onClick={() => setAccent(p.hex)}
                      className={cn(
                        "relative flex h-10 items-center justify-center outline-none",
                        "focus-visible:shadow-[0_0_0_2px_var(--surface),0_0_0_4px_var(--foreground)]",
                        selected && "shadow-[0_0_0_2px_var(--surface),0_0_0_4px_var(--foreground)]",
                      )}
                      style={{ background: p.hex }}
                    >
                      {selected && <Check className="size-4 text-white" strokeWidth={3} />}
                    </button>
                  </Tooltip>
                );
              })}
            </div>
          </section>

          <section>
            <h3 className="text-[13px] font-semibold">Custom</h3>
            <p className="mt-0.5 text-xs text-subtle-foreground">
              Pick any colour or type a hex code. Very light or dark colours are adjusted so text stays readable.
            </p>
            <div className="mt-2.5 flex items-center gap-2">
              <label
                className={cn(
                  "relative size-8 shrink-0 cursor-pointer border border-border-strong",
                  !isPreset && "shadow-[0_0_0_2px_var(--surface),0_0_0_4px_var(--foreground)]",
                )}
                style={{ background: accent }}
              >
                <span className="sr-only">Open colour picker</span>
                <input
                  type="color"
                  value={accent}
                  onChange={(e) => setAccent(e.target.value)}
                  className="absolute inset-0 cursor-pointer opacity-0"
                />
              </label>
              <input
                value={draft}
                onChange={(e) => {
                  setDraft(e.target.value);
                  const hex = normalizeHex(e.target.value);
                  if (hex) setAccent(hex);
                }}
                onBlur={() => setDraft(accent)}
                spellCheck={false}
                autoComplete="off"
                maxLength={7}
                aria-label="Hex colour code"
                aria-invalid={!draftValid || undefined}
                placeholder="#ea580c"
                className="h-8 w-32 border border-border bg-surface-2 px-2.5 font-mono text-[12.5px] uppercase outline-none hover:border-border-strong focus-visible:border-primary aria-invalid:border-danger"
              />
              {!draftValid && <span className="text-xs text-danger">Use a hex code like #2563EB</span>}
            </div>
          </section>

          <section>
            <h3 className="text-[13px] font-semibold">Preview</h3>
            <div className="mt-2.5 flex flex-wrap items-center gap-3 border border-border bg-background p-3">
              <Button variant="primary" size="sm">
                Save & connect
              </Button>
              <span className="text-[13px] font-medium text-primary-text">Link text</span>
              <span className="border-b-2 border-primary pb-1 text-[13px] font-medium">Active tab</span>
              <span className="bg-primary-soft px-2 py-1 text-xs">Highlighted row</span>
            </div>
          </section>
        </div>

        <div className="flex items-center justify-between border-t border-border px-5 py-3">
          <Button
            variant="ghost"
            size="sm"
            onClick={() => setAccent(DEFAULT_ACCENT)}
            disabled={accent === DEFAULT_ACCENT}
          >
            <RotateCcw /> Reset to default
          </Button>
          <DialogClose asChild>
            <Button size="sm">Done</Button>
          </DialogClose>
        </div>
      </DialogContent>
    </Dialog>
  );
}
