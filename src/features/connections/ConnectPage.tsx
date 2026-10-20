import { useEffect, useRef, useState, type ReactNode } from "react";
import { CloudOff, FileOutput, KeyRound, LogIn, MoreVertical, Pencil, Plus, ShieldCheck, Trash2 } from "lucide-react";
import { client, type ConnectionProfile } from "@/services/client";
import { Button } from "@/components/ui/button";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Tooltip } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import { ColorThemeButton, SimulatedBadge, ThemeToggle } from "@/app/AppChrome";
import { ConnectionForm } from "./ConnectionForm";
import { ENVIRONMENT_META } from "./presentation";
import { describeTarget } from "./schema";

type ConnectPageProps = {
  /** Opens the workspace for a connection. */
  onConnected: (profile: ConnectionProfile) => void;
};

export function ConnectPage({ onConnected }: ConnectPageProps) {
  const [profiles, setProfiles] = useState<ConnectionProfile[]>([]);
  const [selected, setSelected] = useState<ConnectionProfile | null>(null);
  // Changes only when the user picks a different profile or starts a new one, so saving
  // does not remount the form and wipe its result.
  const [formKey, setFormKey] = useState("new");

  useEffect(() => {
    let active = true;
    client.connections.list().then((list) => active && setProfiles(list));
    return () => {
      active = false;
    };
  }, []);

  function open(profile: ConnectionProfile | null) {
    setSelected(profile);
    setFormKey(profile?.id ?? `new-${Date.now()}`);
  }

  const navigateTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => () => clearTimeout(navigateTimer.current ?? undefined), []);

  const [pendingDelete, setPendingDelete] = useState<ConnectionProfile | null>(null);
  const [deleteError, setDeleteError] = useState<string | null>(null);

  async function confirmDelete(profile: ConnectionProfile) {
    setPendingDelete(null);
    setDeleteError(null);
    try {
      await client.connections.remove(profile.id);
      setProfiles((list) => list.filter((p) => p.id !== profile.id));
      if (selected?.id === profile.id) open(null);
    } catch (e) {
      setDeleteError(e instanceof Error ? e.message : "The connection could not be deleted.");
    }
  }

  function handleSaved(profile: ConnectionProfile) {
    setSelected(profile);
    setProfiles((list) => [profile, ...list.filter((p) => p.id !== profile.id)]);
    // Leave the success state on screen for a moment before moving on.
    navigateTimer.current = setTimeout(() => onConnected(profile), 700);
  }

  return (
    <div className="flex h-full min-h-0">
      <aside className="relative hidden w-[400px] shrink-0 flex-col overflow-hidden border-r border-border bg-surface-sunken lg:flex xl:w-[440px]">
        <div className="bg-grid pointer-events-none absolute inset-0 [mask-image:linear-gradient(to_bottom,black,transparent_75%)]" />
        <div className="pointer-events-none absolute -top-40 -left-24 size-[420px] bg-[radial-gradient(closest-side,var(--primary-soft),transparent)]" />

        <div className="relative flex min-h-0 flex-1 flex-col px-10 pt-12 pb-8">
          <p className="flex items-center gap-2 text-[13px] font-medium text-muted-foreground">
            <span className="h-px w-6 bg-primary" />
            Database Reporting & Analytics
          </p>
          <h2 className="mt-5 text-[28px] leading-[1.15] font-semibold tracking-tight">
            Turn your database into reports people <span className="text-primary-text">trust.</span>
          </h2>
          <p className="mt-3 max-w-[340px] text-muted-foreground">
            Connect once, then write a query and export it as a clean PDF, Excel or CSV report.
          </p>

          <ul className="mt-9 flex flex-col gap-5">
            <Feature icon={ShieldCheck} title="Read-only by design">
              Reports run in read-only transactions with time limits.
            </Feature>
            <Feature icon={KeyRound} title="Passwords stay in Windows">
              Kept in Credential Manager, never in files, logs or exports.
            </Feature>
            <Feature icon={FileOutput} title="One query, three formats">
              PDF, Excel and CSV from the same report definition.
            </Feature>
            <Feature icon={CloudOff} title="Your data stays with you">
              Your data stays on this computer. No cloud, no tracking.
            </Feature>
          </ul>

          <SavedConnections
            profiles={profiles}
            selectedId={selected?.id ?? null}
            error={deleteError}
            onNew={() => open(null)}
            onEdit={open}
            onConnect={onConnected}
            onDelete={setPendingDelete}
          />
        </div>
      </aside>

      <main className="relative min-w-0 flex-1 bg-background">
        <div className="absolute top-3 right-5 z-20 flex items-center gap-2">
          <SimulatedBadge />
          <ColorThemeButton side="bottom" />
          <ThemeToggle side="bottom" />
        </div>
        <ConnectionForm key={formKey} profile={selected} onSaved={handleSaved} />
      </main>

      <ConfirmDialog
        open={pendingDelete !== null}
        title={`Delete "${pendingDelete?.name ?? ""}"?`}
        description="This permanently removes the saved connection and its stored password from this computer. Your database itself is not affected."
        confirmLabel="Delete connection"
        tone="danger"
        onConfirm={() => pendingDelete && void confirmDelete(pendingDelete)}
        onCancel={() => setPendingDelete(null)}
      />
    </div>
  );
}

function Feature({ icon: Icon, title, children }: { icon: typeof ShieldCheck; title: string; children: ReactNode }) {
  return (
    <li className="flex gap-3.5">
      <span className="flex size-8 shrink-0 items-center justify-center border border-border bg-surface">
        <Icon className="size-4 text-primary-text" />
      </span>
      <div>
        <p className="text-[13px] font-medium">{title}</p>
        <p className="text-xs text-muted-foreground">{children}</p>
      </div>
    </li>
  );
}

function SavedConnections({
  profiles,
  selectedId,
  error,
  onNew,
  onEdit,
  onConnect,
  onDelete,
}: {
  profiles: ConnectionProfile[];
  selectedId: string | null;
  error: string | null;
  onNew: () => void;
  onEdit: (p: ConnectionProfile) => void;
  onConnect: (p: ConnectionProfile) => void;
  onDelete: (p: ConnectionProfile) => void;
}) {
  return (
    <section className="mt-auto flex min-h-0 flex-col pt-10" aria-label="Saved connections">
      <div className="flex items-center justify-between pb-2.5">
        <h3 className="text-[13px] font-semibold text-foreground">Saved Connections</h3>
        {profiles.length > 0 && (
          <Button variant="ghost" size="sm" onClick={onNew} className="-mr-2.5">
            <Plus /> New
          </Button>
        )}
      </div>

      {error && (
        <p role="alert" className="mb-2 border border-border bg-danger-soft px-3 py-2 text-xs text-danger">
          {error}
        </p>
      )}

      {profiles.length === 0 ? (
        <div className="border border-dashed border-border-strong px-4 py-5 text-xs text-subtle-foreground">
          No saved connections yet. Your first one will appear here.
        </div>
      ) : (
        <ul className="min-h-0 overflow-y-auto border border-border bg-surface">
          {profiles.map((p) => {
            const env = ENVIRONMENT_META[p.environment];
            const active = p.id === selectedId;
            return (
              <li
                key={p.id}
                className={cn("group relative flex border-b border-border last:border-b-0", active && "bg-surface-2")}
              >
                {/* Environment edge, layered above the row so hover backgrounds never cover it. */}
                <span
                  aria-hidden
                  className="pointer-events-none absolute inset-y-0 left-0 z-10 w-0.5"
                  style={{ background: env.color }}
                />
                <button
                  type="button"
                  onClick={() => onConnect(p)}
                  title={`Open ${p.name} (${env.label})`}
                  className="flex min-w-0 flex-1 items-center gap-3 py-2.5 pr-2 pl-3 text-left transition-colors outline-none hover:bg-surface-2 focus-visible:bg-surface-2"
                >
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-[13px] font-medium">{p.name}</p>
                    <p className="truncate font-mono text-[11px] text-subtle-foreground">{describeTarget(p)}</p>
                  </div>
                </button>
                <DropdownMenu>
                  <Tooltip content="More actions">
                    <DropdownMenuTrigger asChild>
                      <button
                        type="button"
                        aria-label={`Actions for ${p.name}`}
                        className="flex w-9 shrink-0 items-center justify-center text-muted-foreground outline-none hover:bg-surface-2 hover:text-foreground focus-visible:bg-surface-2 data-[state=open]:bg-surface-2 data-[state=open]:text-foreground"
                      >
                        <MoreVertical className="size-4" />
                      </button>
                    </DropdownMenuTrigger>
                  </Tooltip>
                  <DropdownMenuContent align="end" className="min-w-[180px]">
                    <DropdownMenuItem onSelect={() => onConnect(p)}>
                      <LogIn /> Enter Workspace
                    </DropdownMenuItem>
                    <DropdownMenuItem onSelect={() => onEdit(p)}>
                      <Pencil /> Edit Connection
                    </DropdownMenuItem>
                    <DropdownMenuSeparator />
                    <DropdownMenuItem
                      onSelect={() => onDelete(p)}
                      className="text-danger data-[highlighted]:bg-danger-soft data-[highlighted]:text-danger [&_svg]:text-danger!"
                    >
                      <Trash2 /> Delete Connection
                    </DropdownMenuItem>
                  </DropdownMenuContent>
                </DropdownMenu>
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}
