import { useState } from "react";
import { Check, ChevronsUpDown, LogOut, Settings2 } from "lucide-react";
import { client, type ConnectionProfile } from "@/services/client";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Tooltip } from "@/components/ui/tooltip";
import { ENVIRONMENT_META } from "@/features/connections/presentation";
import { describeTarget } from "@/features/connections/schema";

type Props = {
  current: ConnectionProfile;
  onSwitch: (profile: ConnectionProfile) => void;
  onManage: () => void;
  onDisconnect: () => void;
};

/** The active connection in the status bar. Opens a menu upwards to switch or manage. */
export function ConnectionMenu({ current, onSwitch, onManage, onDisconnect }: Props) {
  const [profiles, setProfiles] = useState<ConnectionProfile[] | null>(null);
  const env = ENVIRONMENT_META[current.environment];

  return (
    <DropdownMenu
      onOpenChange={(open) => {
        if (open)
          client.connections
            .list()
            .then(setProfiles)
            .catch(() => setProfiles([]));
      }}
    >
      <Tooltip content="Switch or manage connections" side="top">
        <DropdownMenuTrigger asChild>
          <button
            type="button"
            aria-label={`Connection: ${current.name}. Switch or manage connections`}
            className="flex h-full min-w-0 items-center gap-2 pr-2 pl-3 outline-none hover:bg-surface-2 hover:text-foreground focus-visible:shadow-[inset_0_0_0_1px_var(--primary)] data-[state=open]:bg-surface-2 data-[state=open]:text-foreground"
          >
            <span className="size-1.5 shrink-0" style={{ background: env.color }} />
            <span className="shrink-0" style={{ color: env.color }}>
              {current.environment === "production" ? "PRODUCTION" : env.label}
            </span>
            <span className="truncate font-mono">{describeTarget(current)}</span>
            <ChevronsUpDown className="size-3 shrink-0" />
          </button>
        </DropdownMenuTrigger>
      </Tooltip>
      <DropdownMenuContent side="top" align="start" sideOffset={6} alignOffset={8} className="w-[320px]">
        <DropdownMenuLabel>Switch Connection</DropdownMenuLabel>
        {profiles === null && <p className="px-2 py-1.5 text-xs text-subtle-foreground">Loading…</p>}
        {profiles?.map((p) => {
          const e = ENVIRONMENT_META[p.environment];
          return (
            <DropdownMenuItem key={p.id} onSelect={() => p.id !== current.id && onSwitch(p)}>
              <span className="size-2 shrink-0" style={{ background: e.color }} />
              <span className="min-w-0 flex-1">
                <span className="block truncate">{p.name}</span>
                <span className="block truncate font-mono text-[10.5px] text-subtle-foreground">
                  {describeTarget(p)}
                </span>
              </span>
              {p.id === current.id && <Check className="text-primary-text!" />}
            </DropdownMenuItem>
          );
        })}
        <DropdownMenuSeparator />
        <DropdownMenuItem onSelect={onManage}>
          <Settings2 /> Manage Connections
        </DropdownMenuItem>
        <DropdownMenuItem onSelect={onDisconnect}>
          <LogOut /> Disconnect
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
