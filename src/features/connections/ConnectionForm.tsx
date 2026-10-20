import { useEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { Controller, useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import {
  AlertTriangle,
  ArrowRight,
  ChevronDown,
  Eye,
  EyeOff,
  FolderOpen,
  Info,
  Link2,
  Loader2,
  SlidersHorizontal,
  Zap,
} from "lucide-react";
import { client, ENVIRONMENTS, SSL_MODES, type ConnectionProfile, type ConnectionTestResult } from "@/services/client";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Field, fieldA11y } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Segmented, SegmentedItem } from "@/components/ui/segmented";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Tooltip } from "@/components/ui/tooltip";
import postgresLogo from "@/assets/postgresql.svg";
import { cn } from "@/lib/utils";
import {
  CONNECTION_FIELDS,
  DEFAULT_VALUES,
  connectionSchema,
  isLocalHost,
  parseConnectionString,
  profileToValues,
  type ConnectionFormValues,
} from "./schema";
import { ENVIRONMENT_META, SSL_MODE_META } from "./presentation";
import { ResultPanel, StatusLine, type Phase } from "./ConnectionStatus";

type Props = {
  /** Profile being edited, or null for a new connection. */
  profile: ConnectionProfile | null;
  onSaved: (profile: ConnectionProfile) => void;
};

export function ConnectionForm({ profile, onSaved }: Props) {
  const {
    register,
    control,
    handleSubmit,
    watch,
    setValue,
    reset,
    formState: { errors },
  } = useForm<ConnectionFormValues>({
    resolver: zodResolver(connectionSchema),
    defaultValues: profile ? profileToValues(profile) : DEFAULT_VALUES,
    // Validate when the user tests or saves, then live as they correct fields; a fresh form
    // never shows errors just because focus moved.
    mode: "onSubmit",
    reValidateMode: "onChange",
  });

  const [phase, setPhase] = useState<Phase>({ kind: "idle" });
  const [tab, setTab] = useState("details");
  const [showPassword, setShowPassword] = useState(false);
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [importNotes, setImportNotes] = useState<string[]>([]);
  const abortRef = useRef<AbortController | null>(null);

  // Cancel any in-flight request if the form goes away.
  useEffect(() => () => abortRef.current?.abort(), []);

  // A test result only describes the values it was run with; drop it once they change.
  useEffect(() => {
    const sub = watch((_values, { name }) => {
      if (name && CONNECTION_FIELDS.has(name)) {
        setPhase((p) => (p.kind === "result" || p.kind === "cancelled" ? { kind: "idle" } : p));
      }
    });
    return () => sub.unsubscribe();
  }, [watch]);

  const busy = phase.kind === "busy";
  const sslMode = watch("sslMode");
  const host = watch("host");
  const environment = watch("environment");
  const sslMeta = SSL_MODE_META[sslMode];

  function run(action: "test" | "save") {
    return handleSubmit(
      async (values) => {
        abortRef.current?.abort();
        const controller = new AbortController();
        abortRef.current = controller;
        setPhase({ kind: "busy", action });
        try {
          let result: ConnectionTestResult;
          if (action === "test") {
            result = await client.connections.test(values, controller.signal, profile?.id);
          } else {
            const saved = await client.connections.save(values, controller.signal, profile?.id);
            result = saved.test;
            if (saved.profile) {
              reset(profileToValues(saved.profile));
              onSaved(saved.profile);
            }
          }
          setPhase({ kind: "result", action, result });
        } catch (error) {
          if (error instanceof DOMException && error.name === "AbortError") {
            setPhase({ kind: "cancelled" });
          } else {
            setPhase({
              kind: "result",
              action,
              result: {
                ok: false,
                category: "connection_failed",
                message: "Something went wrong while contacting the backend.",
                hint: "Try again. If it keeps happening, check the application logs.",
                simulated: client.simulated,
              },
            });
          }
        } finally {
          if (abortRef.current === controller) abortRef.current = null;
        }
      },
      // Validation failed: make sure the user can see the fields that need attention.
      () => setTab("details"),
    )();
  }

  function applyConnectionString(raw: string): string | null {
    const parsed = parseConnectionString(raw);
    if (!parsed.ok) return parsed.error;
    for (const [key, value] of Object.entries(parsed.values)) {
      setValue(key as keyof ConnectionFormValues, value as never, {
        shouldDirty: true,
        shouldValidate: true,
      });
    }
    if (!watch("name") && parsed.values.database && parsed.values.host) {
      setValue("name", `${parsed.values.database} on ${parsed.values.host}`, { shouldDirty: true });
    }
    setImportNotes(parsed.notes);
    setTab("details");
    return null;
  }

  const showProdWarning = environment === "production" && sslMeta.level !== "secure";
  const showLocalHint = isLocalHost(host) && sslMode.startsWith("verify");

  return (
    <form
      noValidate
      className="flex h-full min-h-0 flex-col"
      onSubmit={(e) => {
        e.preventDefault();
        run("save");
      }}
      onKeyDown={(e) => {
        if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
          e.preventDefault();
          if (!busy) run("test");
        }
      }}
    >
      <div className="min-h-0 flex-1 overflow-y-auto [scrollbar-gutter:stable]">
        <div className="mx-auto w-full max-w-[760px] px-10 pt-10 pb-12">
          <header className="flex items-start justify-between gap-6">
            <div>
              <p className="text-[13px] font-semibold text-primary-text">
                {profile ? "Edit Connection" : "New Connection"}
              </p>
              <h1 className="mt-2 text-[22px] font-semibold tracking-tight">
                {profile ? profile.name : "Connect to PostgreSQL"}
              </h1>
              <p className="mt-1 text-muted-foreground">
                Enter your server details. Test the connection, then save it to start building reports.
              </p>
            </div>
            <span className="mt-1 inline-flex h-6 shrink-0 items-center gap-1.5 border border-border bg-surface-2 px-2 text-xs text-muted-foreground">
              <img src={postgresLogo} alt="" aria-hidden className="size-4" />
              PostgreSQL
            </span>
          </header>

          <Tabs value={tab} onValueChange={setTab} className="mt-7">
            <TabsList>
              <TabsTrigger value="details">
                <SlidersHorizontal /> Connection details
              </TabsTrigger>
              <TabsTrigger value="string">
                <Link2 /> Paste connection string
              </TabsTrigger>
            </TabsList>

            {/* forceMount keeps the fields registered and focusable while the other tab is open. */}
            <TabsContent value="details" forceMount className="data-[state=inactive]:hidden">
              <fieldset disabled={busy} className="min-w-0">
                {importNotes.length > 0 && (
                  <Callout tone="info" className="mt-6" onDismiss={() => setImportNotes([])}>
                    <p className="font-medium text-foreground">Filled in from your connection string.</p>
                    {importNotes.map((n) => (
                      <p key={n}>{n}</p>
                    ))}
                  </Callout>
                )}

                <Section title="General" description="How this connection appears across Intalix.">
                  <Field label="Connection name" htmlFor="name" error={errors.name?.message}>
                    <Input
                      placeholder="e.g. Finance warehouse"
                      autoFocus={!profile}
                      {...fieldA11y("name", errors.name?.message)}
                      {...register("name")}
                    />
                  </Field>
                  <div className="flex flex-col gap-1.5">
                    <Label id="environment-label">Environment</Label>
                    <Controller
                      control={control}
                      name="environment"
                      render={({ field }) => (
                        <Segmented
                          aria-labelledby="environment-label"
                          value={field.value}
                          onValueChange={field.onChange}
                        >
                          {ENVIRONMENTS.map((env) => (
                            <SegmentedItem
                              key={env}
                              value={env}
                              style={{ "--seg-accent": ENVIRONMENT_META[env].color } as CSSProperties}
                            >
                              <span className="size-1.5" style={{ background: ENVIRONMENT_META[env].color }} />
                              {ENVIRONMENT_META[env].label}
                            </SegmentedItem>
                          ))}
                        </Segmented>
                      )}
                    />
                    <p className="text-xs text-subtle-foreground">
                      {environment === "production"
                        ? "Production connections carry a red marker everywhere they are used."
                        : "Shown as a coloured marker so you always know where queries run."}
                    </p>
                  </div>
                </Section>

                <Section title="Server" description="Where your PostgreSQL database is running.">
                  <div className="grid grid-cols-[1fr_112px] gap-3">
                    <Field label="Host" htmlFor="host" error={errors.host?.message}>
                      <Input
                        placeholder="db.company.internal"
                        className="font-mono text-[12.5px]"
                        {...fieldA11y("host", errors.host?.message)}
                        {...register("host")}
                      />
                    </Field>
                    <Field label="Port" htmlFor="port" error={errors.port?.message}>
                      <Input
                        type="number"
                        inputMode="numeric"
                        className="tabular font-mono text-[12.5px]"
                        {...fieldA11y("port", errors.port?.message)}
                        {...register("port", { valueAsNumber: true })}
                      />
                    </Field>
                  </div>
                  <Field label="Database" htmlFor="database" error={errors.database?.message}>
                    <Input
                      placeholder="analytics"
                      className="font-mono text-[12.5px]"
                      {...fieldA11y("database", errors.database?.message)}
                      {...register("database")}
                    />
                  </Field>
                </Section>

                <Section
                  title="Authentication"
                  description="Use a read-only database user that can only SELECT what reports need."
                >
                  <Field label="Username" htmlFor="username" error={errors.username?.message}>
                    <Input
                      placeholder="report_reader"
                      className="font-mono text-[12.5px]"
                      {...fieldA11y("username", errors.username?.message)}
                      {...register("username")}
                    />
                  </Field>
                  <Field label="Password" htmlFor="password" error={errors.password?.message}>
                    <div className="relative">
                      <Input
                        type={showPassword ? "text" : "password"}
                        autoComplete="new-password"
                        placeholder={
                          profile?.hasStoredPassword ? "Saved securely. Leave blank to keep it." : "••••••••"
                        }
                        className="pr-9"
                        {...fieldA11y("password", errors.password?.message)}
                        {...register("password")}
                      />
                      <Tooltip content={showPassword ? "Hide password" : "Show password"}>
                        <button
                          type="button"
                          onClick={() => setShowPassword((s) => !s)}
                          aria-label={showPassword ? "Hide password" : "Show password"}
                          className="absolute inset-y-0 right-0 flex w-8 items-center justify-center text-subtle-foreground outline-none hover:text-foreground focus-visible:text-foreground"
                        >
                          {showPassword ? <EyeOff className="size-3.5" /> : <Eye className="size-3.5" />}
                        </button>
                      </Tooltip>
                    </div>
                  </Field>
                  <Controller
                    control={control}
                    name="savePassword"
                    render={({ field }) => (
                      <label className="flex cursor-pointer items-start gap-2.5">
                        <Checkbox
                          checked={field.value}
                          onCheckedChange={(v) => field.onChange(v === true)}
                          className="mt-0.5"
                        />
                        <span>
                          <span className="block text-[13px]">Remember password</span>
                          <span className="block text-xs text-subtle-foreground">
                            Stored in Windows Credential Manager, never in Intalix files, logs or exports.
                          </span>
                        </span>
                      </label>
                    )}
                  />
                </Section>

                <Section title="Encryption" description="How the connection to the server is secured with TLS.">
                  <Field label="SSL mode" htmlFor="sslMode">
                    <Controller
                      control={control}
                      name="sslMode"
                      render={({ field }) => (
                        <Select value={field.value} onValueChange={field.onChange}>
                          <SelectTrigger id="sslMode">
                            <SelectValue />
                          </SelectTrigger>
                          <SelectContent>
                            {SSL_MODES.map((m) => (
                              <SelectItem key={m} value={m}>
                                <span className="flex items-center gap-2">
                                  <SslDot level={SSL_MODE_META[m].level} />
                                  {SSL_MODE_META[m].label}
                                  <span className="text-subtle-foreground">· {SSL_MODE_META[m].summary}</span>
                                </span>
                              </SelectItem>
                            ))}
                          </SelectContent>
                        </Select>
                      )}
                    />
                  </Field>
                  <Callout
                    tone={sslMeta.level === "secure" ? "success" : sslMeta.level === "weak" ? "warning" : "danger"}
                  >
                    <p className="font-medium text-foreground">{sslMeta.summary}</p>
                    <p>{sslMeta.detail}</p>
                    {showLocalHint && <p>For a local database without TLS, choose Disable.</p>}
                  </Callout>
                  {showProdWarning && (
                    <Callout tone="danger">
                      <p className="font-medium text-foreground">Not recommended for production</p>
                      <p>Production data should use Verify full so the server's identity is checked.</p>
                    </Callout>
                  )}
                  {(sslMode === "verify-ca" || sslMode === "verify-full") && (
                    <Field
                      label="CA certificate"
                      htmlFor="caCertPath"
                      optional
                      error={errors.caCertPath?.message}
                      hint="Needed when your server uses a private or company certificate authority."
                    >
                      <div className="flex gap-2">
                        <Input
                          placeholder="C:\certs\company-root-ca.pem"
                          className="font-mono text-[12.5px]"
                          {...fieldA11y("caCertPath", errors.caCertPath?.message)}
                          {...register("caCertPath")}
                        />
                        {/* Disabled buttons don't receive hover, so the tooltip sits on a wrapper. */}
                        <Tooltip content="File picker is coming soon. Type the path for now.">
                          <span tabIndex={0} className="inline-flex outline-none">
                            <Button disabled>
                              <FolderOpen /> Browse
                            </Button>
                          </span>
                        </Tooltip>
                      </div>
                    </Field>
                  )}
                </Section>

                <div className="border-t border-border pt-5">
                  <button
                    type="button"
                    onClick={() => setShowAdvanced((s) => !s)}
                    aria-expanded={showAdvanced}
                    aria-controls="advanced-options"
                    className="flex items-center gap-1.5 text-[13px] font-medium text-muted-foreground outline-none hover:text-foreground focus-visible:text-foreground"
                  >
                    <ChevronDown className={cn("size-3.5 transition-transform", !showAdvanced && "-rotate-90")} />
                    Advanced options
                    {(errors.connectTimeoutSecs ||
                      errors.queryTimeoutSecs ||
                      errors.maxConnections ||
                      errors.applicationName) && <span className="size-1.5 bg-danger" aria-label="has errors" />}
                  </button>
                  {/* Hidden rather than unmounted so validation errors here still surface. */}
                  <div id="advanced-options" hidden={!showAdvanced} className="mt-5 grid grid-cols-2 gap-x-3 gap-y-4">
                    <Field
                      label="Connect timeout (seconds)"
                      htmlFor="connectTimeoutSecs"
                      error={errors.connectTimeoutSecs?.message}
                    >
                      <Input
                        type="number"
                        className="tabular font-mono text-[12.5px]"
                        {...fieldA11y("connectTimeoutSecs", errors.connectTimeoutSecs?.message)}
                        {...register("connectTimeoutSecs", { valueAsNumber: true })}
                      />
                    </Field>
                    <Field
                      label="Query timeout (seconds)"
                      htmlFor="queryTimeoutSecs"
                      error={errors.queryTimeoutSecs?.message}
                    >
                      <Input
                        type="number"
                        className="tabular font-mono text-[12.5px]"
                        {...fieldA11y("queryTimeoutSecs", errors.queryTimeoutSecs?.message)}
                        {...register("queryTimeoutSecs", { valueAsNumber: true })}
                      />
                    </Field>
                    <Field
                      label="Max connections"
                      htmlFor="maxConnections"
                      error={errors.maxConnections?.message}
                      hint="Upper limit Intalix opens to this server at once."
                    >
                      <Input
                        type="number"
                        className="tabular font-mono text-[12.5px]"
                        {...fieldA11y("maxConnections", errors.maxConnections?.message)}
                        {...register("maxConnections", { valueAsNumber: true })}
                      />
                    </Field>
                    <Field
                      label="Application name"
                      htmlFor="applicationName"
                      error={errors.applicationName?.message}
                      hint="Shown to DBAs in pg_stat_activity."
                    >
                      <Input
                        {...fieldA11y("applicationName", errors.applicationName?.message)}
                        {...register("applicationName")}
                      />
                    </Field>
                  </div>
                </div>
              </fieldset>
            </TabsContent>

            <TabsContent value="string">
              <ConnectionStringImport onApply={applyConnectionString} />
            </TabsContent>
          </Tabs>
        </div>
      </div>

      <ResultPanel phase={phase} />

      {/* Same column as the form above: 760px wide with 40px inner padding, plus the 10px
          the form reserves for its scrollbar, so the buttons line up with the fields. */}
      <footer className="shrink-0 border-t border-border bg-surface pr-[10px]">
        <div className="mx-auto flex h-14 w-full max-w-[760px] items-center justify-end gap-4 px-10">
          {/* The result bar above already shows the outcome; this copy is only announced to
              screen readers, so status changes are still heard. */}
          <div className="sr-only">
            <StatusLine phase={phase} />
          </div>
          <div className="flex items-center gap-2">
            {busy && (
              <Button variant="ghost" onClick={() => abortRef.current?.abort()}>
                Cancel
              </Button>
            )}
            <Tooltip content="Check the details without saving" shortcut="Ctrl+Enter" side="top">
              <Button onClick={() => run("test")} disabled={busy}>
                {busy && phase.action === "test" ? <Loader2 className="animate-spin" /> : <Zap />}
                Test Connection
              </Button>
            </Tooltip>
            <Tooltip content="Tests the connection, then saves it and opens the workspace" shortcut="Enter" side="top">
              <Button type="submit" variant="primary" disabled={busy}>
                {busy && phase.action === "save" ? <Loader2 className="animate-spin" /> : null}
                {profile ? "Save & Reconnect" : "Save & Connect"}
                {!(busy && phase.action === "save") && <ArrowRight />}
              </Button>
            </Tooltip>
          </div>
        </div>
      </footer>
    </form>
  );
}

function Section({ title, description, children }: { title: string; description: string; children: ReactNode }) {
  return (
    <section className="grid grid-cols-[180px_1fr] gap-10 border-t border-border py-7 first-of-type:border-t-0">
      <div>
        <h2 className="text-[13px] font-semibold">{title}</h2>
        <p className="mt-1 text-xs leading-relaxed text-subtle-foreground">{description}</p>
      </div>
      <div className="flex min-w-0 flex-col gap-4">{children}</div>
    </section>
  );
}

const CALLOUT_TONES = {
  info: { bar: "var(--muted-foreground)", bg: "bg-surface-2", Icon: Info, icon: "text-muted-foreground" },
  success: { bar: "var(--success)", bg: "bg-success-soft", Icon: Info, icon: "text-success" },
  warning: { bar: "var(--warning)", bg: "bg-warning-soft", Icon: AlertTriangle, icon: "text-warning" },
  danger: { bar: "var(--danger)", bg: "bg-danger-soft", Icon: AlertTriangle, icon: "text-danger" },
} as const;

function Callout({
  tone,
  className,
  onDismiss,
  children,
}: {
  tone: keyof typeof CALLOUT_TONES;
  className?: string;
  onDismiss?: () => void;
  children: ReactNode;
}) {
  const t = CALLOUT_TONES[tone];
  return (
    <div
      className={cn("flex gap-2.5 border border-border px-3 py-2.5 text-xs text-muted-foreground", t.bg, className)}
      style={{ boxShadow: `inset 2px 0 0 ${t.bar}` }}
    >
      <t.Icon className={cn("mt-px size-3.5 shrink-0", t.icon)} />
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">{children}</div>
      {onDismiss && (
        <button type="button" onClick={onDismiss} className="self-start text-subtle-foreground hover:text-foreground">
          Dismiss
        </button>
      )}
    </div>
  );
}

function SslDot({ level }: { level: "insecure" | "weak" | "secure" }) {
  const color = level === "secure" ? "var(--success)" : level === "weak" ? "var(--warning)" : "var(--danger)";
  return <span className="size-1.5 shrink-0" style={{ background: color }} />;
}

function ConnectionStringImport({ onApply }: { onApply: (raw: string) => string | null }) {
  const [value, setValue] = useState("");
  const [error, setError] = useState<string | null>(null);
  return (
    <div className="mt-6 flex flex-col gap-4">
      <Field
        label="Connection string"
        htmlFor="connectionString"
        error={error ?? undefined}
        hint="We split it into the fields on the Connection details tab. The string itself is not saved."
      >
        <textarea
          rows={4}
          value={value}
          onChange={(e) => {
            setValue(e.target.value);
            setError(null);
          }}
          spellCheck={false}
          autoComplete="off"
          placeholder="postgresql://report_reader@db.company.internal:5432/analytics?sslmode=verify-full"
          {...fieldA11y("connectionString", error ?? undefined)}
          className="w-full resize-none border border-border bg-surface-2 px-2.5 py-2 font-mono text-[12.5px] leading-relaxed outline-none placeholder:text-subtle-foreground hover:border-border-strong focus-visible:border-primary focus-visible:shadow-[0_0_0_1px_var(--primary)] aria-invalid:border-danger"
        />
      </Field>
      <div>
        <Button disabled={!value.trim()} onClick={() => setError(onApply(value))}>
          Fill in details <ArrowRight />
        </Button>
      </div>
    </div>
  );
}
