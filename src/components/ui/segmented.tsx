import * as React from "react";
import { RadioGroup as RadioGroupPrimitive } from "radix-ui";
import { cn } from "@/lib/utils";

/** A row of mutually exclusive options that reads as one control (keyboard: arrow keys). */
function Segmented({ className, ...props }: React.ComponentProps<typeof RadioGroupPrimitive.Root>) {
  return (
    <RadioGroupPrimitive.Root
      data-slot="segmented"
      orientation="horizontal"
      className={cn("grid auto-cols-fr grid-flow-col border border-border bg-surface-2", className)}
      {...props}
    />
  );
}

function SegmentedItem({ className, ...props }: React.ComponentProps<typeof RadioGroupPrimitive.Item>) {
  return (
    <RadioGroupPrimitive.Item
      data-slot="segmented-item"
      className={cn(
        "inline-flex h-[30px] items-center justify-center gap-2 border-r border-border px-3 text-[13px] text-muted-foreground transition-colors outline-none last:border-r-0",
        "hover:text-foreground focus-visible:shadow-[inset_0_0_0_1px_var(--primary)]",
        "data-[state=checked]:bg-surface data-[state=checked]:text-foreground data-[state=checked]:shadow-[inset_0_-2px_0_var(--seg-accent,var(--primary))]",
        className,
      )}
      {...props}
    />
  );
}

export { Segmented, SegmentedItem };
