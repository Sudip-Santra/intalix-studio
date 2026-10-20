import { useState } from "react";
import { Moon, Palette, Sun } from "lucide-react";
import { client } from "@/services/client";
import { Tooltip } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import { setThemePreference, useResolvedTheme } from "./theme";
import { ColorThemeDialog } from "./ColorThemeDialog";
import { useAccent } from "./accent";

/** Light/dark switch. `compact` fits the 24px status bar. */
export function ThemeToggle({ compact = false, side = "top" }: { compact?: boolean; side?: "top" | "bottom" }) {
  const theme = useResolvedTheme();
  const next = theme === "dark" ? "light" : "dark";
  const label = theme === "dark" ? "Switch to light theme" : "Switch to dark theme";
  return (
    <Tooltip content={label} side={side} delay={0}>
      <button
        type="button"
        aria-label={label}
        onClick={() => setThemePreference(next)}
        className={cn(
          "flex items-center justify-center text-muted-foreground outline-none hover:bg-surface-2 hover:text-foreground focus-visible:shadow-[inset_0_0_0_1px_var(--primary)]",
          compact ? "h-full w-7" : "size-7",
        )}
      >
        {theme === "dark" ? (
          <Sun className={compact ? "size-3" : "size-3.5"} />
        ) : (
          <Moon className={compact ? "size-3" : "size-3.5"} />
        )}
      </button>
    </Tooltip>
  );
}

/** Opens the accent colour picker. The icon carries a small swatch of the current colour. */
export function ColorThemeButton({ compact = false, side = "top" }: { compact?: boolean; side?: "top" | "bottom" }) {
  const [open, setOpen] = useState(false);
  const accent = useAccent();
  return (
    <>
      <Tooltip content="Color theme" side={side} delay={0}>
        <button
          type="button"
          aria-label="Color theme"
          onClick={() => setOpen(true)}
          className={cn(
            "relative flex items-center justify-center text-muted-foreground outline-none hover:bg-surface-2 hover:text-foreground focus-visible:shadow-[inset_0_0_0_1px_var(--primary)]",
            compact ? "h-full w-7" : "size-7",
          )}
        >
          <Palette className={compact ? "size-3" : "size-3.5"} />
          <span
            className={cn("absolute", compact ? "right-1.5 bottom-1 size-1" : "right-1 bottom-1 size-1.5")}
            style={{ background: accent }}
          />
        </button>
      </Tooltip>
      <ColorThemeDialog open={open} onOpenChange={setOpen} />
    </>
  );
}

/** Shown only in browser previews, where data comes from the simulated backend. */
export function SimulatedBadge() {
  if (!client.simulated) return null;
  return (
    <span
      className="inline-flex h-6 items-center gap-1.5 border border-border px-2 text-[11px] text-muted-foreground"
      title="The desktop backend is not connected. Data on these screens is simulated."
    >
      <span className="size-1.5 bg-warning" />
      Preview · simulated backend
    </span>
  );
}
