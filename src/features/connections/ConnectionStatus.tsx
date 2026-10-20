import { CheckCircle2, CircleSlash, Loader2, XCircle } from "lucide-react";
import type { ConnectionTestResult } from "@/services/client";
import { ERROR_TITLES } from "./presentation";

export type Phase =
  | { kind: "idle" }
  | { kind: "busy"; action: "test" | "save" }
  | { kind: "result"; action: "test" | "save"; result: ConnectionTestResult }
  | { kind: "cancelled" };

/** One-line status in the footer, announced to screen readers as it changes. */
export function StatusLine({ phase }: { phase: Phase }) {
  return (
    <div role="status" aria-live="polite" className="flex min-w-0 items-center gap-2 text-xs">
      {phase.kind === "busy" && (
        <>
          <Loader2 className="size-3.5 animate-spin text-primary-text" />
          <span className="text-muted-foreground">
            {phase.action === "test" ? "Testing connection…" : "Connecting and saving…"}
          </span>
        </>
      )}
      {phase.kind === "cancelled" && (
        <>
          <CircleSlash className="size-3.5 text-subtle-foreground" />
          <span className="text-muted-foreground">Cancelled</span>
        </>
      )}
      {phase.kind === "result" &&
        (phase.result.ok ? (
          <>
            <CheckCircle2 className="size-3.5 text-success" />
            <span className="text-foreground">{phase.action === "save" ? "Saved and connected" : "Connected"}</span>
            <span className="tabular text-subtle-foreground">· {phase.result.latencyMs} ms</span>
          </>
        ) : (
          <>
            <XCircle className="size-3.5 text-danger" />
            <span className="truncate text-foreground">{ERROR_TITLES[phase.result.category]}</span>
          </>
        ))}
    </div>
  );
}

/** Detailed outcome shown just above the footer after a test or save finishes. */
export function ResultPanel({ phase }: { phase: Phase }) {
  if (phase.kind !== "result") return null;
  const { result, action } = phase;

  if (result.ok) {
    return (
      <div className="shrink-0 border-t border-border bg-success-soft py-3.5 pr-[10px] shadow-[inset_0_2px_0_var(--success)]">
        <div className="mx-auto flex w-full max-w-[760px] items-center gap-8 px-10">
          <div className="flex items-center gap-2.5">
            <CheckCircle2 className="size-4 text-success" />
            <div>
              <p className="text-[13px] font-medium">
                {action === "save" ? "Connection saved" : "Connection successful"}
              </p>
              {result.simulated && <SimulatedNote />}
            </div>
          </div>
          <dl className="ml-auto grid grid-cols-3 gap-8 text-xs">
            <Meta label="Server" value={result.serverVersion} />
            <Meta label="Latency" value={`${result.latencyMs} ms`} />
            <Meta label="Encryption" value={result.tls ?? "Not encrypted"} warn={!result.tls} />
          </dl>
        </div>
      </div>
    );
  }

  return (
    <div className="shrink-0 border-t border-border bg-danger-soft py-3.5 pr-[10px] shadow-[inset_0_2px_0_var(--danger)]">
      <div className="mx-auto flex w-full max-w-[760px] items-start gap-2.5 px-10">
        <XCircle className="mt-0.5 size-4 shrink-0 text-danger" />
        <div className="min-w-0 text-xs">
          <p className="text-[13px] font-medium">{ERROR_TITLES[result.category]}</p>
          <p className="mt-0.5 text-muted-foreground">{result.message}</p>
          {result.hint && <p className="mt-0.5 text-subtle-foreground">{result.hint}</p>}
          {result.simulated && <SimulatedNote />}
        </div>
      </div>
    </div>
  );
}

function Meta({ label, value, warn }: { label: string; value: string; warn?: boolean }) {
  return (
    <div>
      <dt className="text-subtle-foreground">{label}</dt>
      <dd className={warn ? "mt-0.5 font-medium text-warning" : "tabular mt-0.5 font-medium"}>{value}</dd>
    </div>
  );
}

function SimulatedNote() {
  return <p className="mt-0.5 text-[11px] text-subtle-foreground">Simulated result. No real database was contacted.</p>;
}
