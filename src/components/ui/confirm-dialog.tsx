import { AlertDialog as AlertPrimitive } from "radix-ui";
import { buttonVariants } from "./button";
import { cn } from "@/lib/utils";

type ConfirmDialogProps = {
  open: boolean;
  title: string;
  description: string;
  confirmLabel: string;
  tone?: "primary" | "danger";
  onConfirm: () => void;
  onCancel: () => void;
};

/** Blocking yes/no question. Escape and the Cancel button both cancel; focus starts on Cancel. */
export function ConfirmDialog({
  open,
  title,
  description,
  confirmLabel,
  tone = "primary",
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  return (
    <AlertPrimitive.Root open={open} onOpenChange={(o) => !o && onCancel()}>
      <AlertPrimitive.Portal>
        <AlertPrimitive.Overlay className="fixed inset-0 z-50 bg-black/50" />
        <AlertPrimitive.Content className="fixed top-1/2 left-1/2 z-50 w-[420px] -translate-x-1/2 -translate-y-1/2 border border-border-strong bg-surface p-5 shadow-[0_24px_64px_-12px_rgb(0_0_0/0.5)]">
          <AlertPrimitive.Title className="text-[15px] font-semibold">{title}</AlertPrimitive.Title>
          <AlertPrimitive.Description className="mt-1.5 text-[13px] text-muted-foreground">
            {description}
          </AlertPrimitive.Description>
          <div className="mt-5 flex justify-end gap-2">
            <AlertPrimitive.Cancel className={buttonVariants({ variant: "secondary" })}>Cancel</AlertPrimitive.Cancel>
            <AlertPrimitive.Action
              onClick={onConfirm}
              className={cn(buttonVariants({ variant: tone === "danger" ? "danger" : "primary" }))}
            >
              {confirmLabel}
            </AlertPrimitive.Action>
          </div>
        </AlertPrimitive.Content>
      </AlertPrimitive.Portal>
    </AlertPrimitive.Root>
  );
}
