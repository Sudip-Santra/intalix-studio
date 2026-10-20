import * as React from "react";
import { cn } from "@/lib/utils";
import { Label } from "./label";

type FieldProps = {
  label: React.ReactNode;
  htmlFor?: string;
  error?: string;
  hint?: React.ReactNode;
  optional?: boolean;
  className?: string;
  children: React.ReactNode;
};

/** Label + control + hint/error. Pair with `fieldA11y(id, error)` on the control. */
function Field({ label, htmlFor, error, hint, optional, className, children }: FieldProps) {
  const messageId = htmlFor ? `${htmlFor}-message` : undefined;
  return (
    <div className={cn("flex min-w-0 flex-col gap-1.5", className)}>
      <Label htmlFor={htmlFor} className="flex items-center gap-1.5">
        {label}
        {optional && <span className="font-normal text-subtle-foreground">Optional</span>}
      </Label>
      {children}
      {error ? (
        <p id={messageId} role="alert" className="text-xs text-danger">
          {error}
        </p>
      ) : hint ? (
        <p id={messageId} className="text-xs text-subtle-foreground">
          {hint}
        </p>
      ) : null}
    </div>
  );
}

/** Props that link a control to its Field message for screen readers. */
function fieldA11y(id: string, error: string | undefined) {
  return {
    id,
    "aria-invalid": error ? true : undefined,
    "aria-describedby": `${id}-message`,
  } as const;
}

export { Field, fieldA11y };
