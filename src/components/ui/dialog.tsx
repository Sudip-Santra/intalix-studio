import * as React from "react";
import { Dialog as DialogPrimitive } from "radix-ui";
import { X } from "lucide-react";
import { cn } from "@/lib/utils";

const Dialog = DialogPrimitive.Root;

type DialogContentProps = React.ComponentProps<typeof DialogPrimitive.Content> & {
  title: string;
  description?: string;
};

/** Centred modal with a title bar and close button. Esc and clicking outside close it. */
function DialogContent({ title, description, className, children, ...props }: DialogContentProps) {
  return (
    <DialogPrimitive.Portal>
      <DialogPrimitive.Overlay className="fixed inset-0 z-50 bg-black/50" />
      <DialogPrimitive.Content
        className={cn(
          "fixed top-1/2 left-1/2 z-50 flex max-h-[calc(100vh-32px)] w-[min(520px,calc(100vw-32px))] -translate-x-1/2 -translate-y-1/2 flex-col border border-border-strong bg-surface shadow-[0_24px_64px_-12px_rgb(0_0_0/0.5)] outline-none",
          className,
        )}
        {...props}
      >
        <div className="flex shrink-0 items-start justify-between gap-4 border-b border-border px-5 py-4">
          <div>
            <DialogPrimitive.Title className="text-[15px] font-semibold">{title}</DialogPrimitive.Title>
            {description ? (
              <DialogPrimitive.Description className="mt-0.5 text-xs text-muted-foreground">
                {description}
              </DialogPrimitive.Description>
            ) : (
              <DialogPrimitive.Description className="sr-only">{title}</DialogPrimitive.Description>
            )}
          </div>
          <DialogPrimitive.Close
            aria-label="Close"
            className="-mt-0.5 -mr-1.5 flex size-7 items-center justify-center text-muted-foreground outline-none hover:bg-surface-2 hover:text-foreground focus-visible:shadow-[inset_0_0_0_1px_var(--primary)]"
          >
            <X className="size-4" />
          </DialogPrimitive.Close>
        </div>
        <div className="min-h-0 overflow-y-auto">{children}</div>
      </DialogPrimitive.Content>
    </DialogPrimitive.Portal>
  );
}

const DialogClose = DialogPrimitive.Close;

export { Dialog, DialogClose, DialogContent };
