import * as React from "react";
import { Tooltip as TooltipPrimitive } from "radix-ui";
import { cn } from "@/lib/utils";

const TooltipProvider = TooltipPrimitive.Provider;

type TooltipProps = {
  content: React.ReactNode;
  /** Keyboard shortcut shown after the label, e.g. "Ctrl+T". */
  shortcut?: string;
  side?: React.ComponentProps<typeof TooltipPrimitive.Content>["side"];
  /** Overrides the app-wide delay, e.g. 0 for an instant tooltip. */
  delay?: number;
  children: React.ReactNode;
};

/** Hover/focus label for icon-only controls. The trigger must forward refs (buttons do). */
function Tooltip({ content, shortcut, side = "bottom", delay, children }: TooltipProps) {
  return (
    <TooltipPrimitive.Root delayDuration={delay}>
      <TooltipPrimitive.Trigger asChild>{children}</TooltipPrimitive.Trigger>
      <TooltipPrimitive.Portal>
        <TooltipPrimitive.Content
          data-slot="tooltip-content"
          side={side}
          sideOffset={6}
          collisionPadding={8}
          className={cn(
            "z-50 flex items-center gap-2 border border-border-strong bg-surface px-2 py-1 text-xs text-foreground shadow-[0_8px_24px_-8px_rgb(0_0_0/0.4)]",
          )}
        >
          {content}
          {shortcut && <kbd className="font-mono text-[10.5px] text-subtle-foreground">{shortcut}</kbd>}
        </TooltipPrimitive.Content>
      </TooltipPrimitive.Portal>
    </TooltipPrimitive.Root>
  );
}

export { Tooltip, TooltipProvider };
