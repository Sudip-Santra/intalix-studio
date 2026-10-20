import * as React from "react";
import { cn } from "@/lib/utils";

function Input({ className, type, ...props }: React.ComponentProps<"input">) {
  return (
    <input
      type={type}
      data-slot="input"
      spellCheck={false}
      autoComplete="off"
      className={cn(
        "h-8 w-full min-w-0 border border-border bg-surface-2 px-2.5 text-[13px] text-foreground transition-colors outline-none",
        "placeholder:text-subtle-foreground hover:border-border-strong",
        "focus-visible:border-primary focus-visible:shadow-[0_0_0_1px_var(--primary)]",
        "aria-invalid:border-danger aria-invalid:focus-visible:shadow-[0_0_0_1px_var(--danger)]",
        "disabled:cursor-not-allowed disabled:opacity-50",
        className,
      )}
      {...props}
    />
  );
}

export { Input };
