import { useEffect, useRef } from "react";
import { Copy, CopyPlus, Plus, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { copyToClipboard } from "@/lib/clipboard";
import { Tooltip } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import { QueryIcon, RELATION_META } from "./object-icons";
import { hasUnsavedWork, type Tab } from "./tabs-state";

type Props = {
  tabs: Tab[];
  activeId: string | null;
  onActivate: (id: string) => void;
  onClose: (id: string) => void;
  /** Closes several tabs at once; the caller confirms if any hold unsaved work. */
  onCloseMany: (ids: string[]) => void;
  onDuplicate: (id: string) => void;
  onNewQuery: () => void;
};

export function TabBar({ tabs, activeId, onActivate, onClose, onCloseMany, onDuplicate, onNewQuery }: Props) {
  const listRef = useRef<HTMLDivElement>(null);

  // Keep the active tab scrolled into view when there are many.
  useEffect(() => {
    if (!activeId) return;
    listRef.current
      ?.querySelector<HTMLElement>(`[data-tab-id="${CSS.escape(activeId)}"]`)
      ?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }, [activeId]);

  return (
    <div className="flex h-9 shrink-0 items-stretch border-b border-border bg-surface">
      <div
        ref={listRef}
        role="tablist"
        aria-label="Open tabs"
        className="flex min-w-0 [scrollbar-width:none] items-stretch overflow-x-auto [&::-webkit-scrollbar]:hidden"
        onKeyDown={(e) => {
          if (e.key !== "ArrowRight" && e.key !== "ArrowLeft") return;
          const i = tabs.findIndex((t) => t.id === activeId);
          const next = tabs[(i + (e.key === "ArrowRight" ? 1 : -1) + tabs.length) % tabs.length];
          if (next) {
            e.preventDefault();
            onActivate(next.id);
            listRef.current?.querySelector<HTMLElement>(`[data-tab-id="${CSS.escape(next.id)}"]`)?.focus();
          }
        }}
      >
        {tabs.map((tab) => {
          const active = tab.id === activeId;
          const dirty = hasUnsavedWork(tab);
          const Icon = tab.kind === "query" ? QueryIcon : RELATION_META[tab.relationKind].icon;
          const label = tab.kind === "query" ? tab.title : tab.name;
          const index = tabs.indexOf(tab);
          return (
            <ContextMenu key={tab.id}>
              <ContextMenuTrigger asChild>
                <div
                  data-tab-id={tab.id}
                  role="tab"
                  aria-selected={active}
                  tabIndex={active ? 0 : -1}
                  title={tab.kind === "relation" ? `${tab.schema}.${tab.name}` : tab.title}
                  onClick={() => onActivate(tab.id)}
                  onAuxClick={(e) => {
                    if (e.button === 1) {
                      e.preventDefault();
                      onClose(tab.id);
                    }
                  }}
                  className={cn(
                    "group relative flex max-w-[220px] min-w-[120px] shrink-0 cursor-pointer items-center gap-2 border-r border-border pr-1.5 pl-3 text-[12.5px] outline-none select-none",
                    active
                      ? "bg-background text-foreground shadow-[inset_0_2px_0_var(--primary)]"
                      : "text-muted-foreground hover:bg-surface-2 hover:text-foreground",
                    "focus-visible:shadow-[inset_0_0_0_1px_var(--primary)]",
                  )}
                >
                  {/* Hides the bar's bottom border under the active tab so it joins the content. */}
                  {active && <span className="absolute inset-x-0 -bottom-px h-px bg-background" />}
                  <Icon
                    className="size-3.5 shrink-0"
                    style={{
                      color: tab.kind === "query" ? "var(--primary-text)" : RELATION_META[tab.relationKind].color,
                    }}
                  />
                  <span className="min-w-0 flex-1 truncate">
                    {tab.kind === "relation" && <span className="text-subtle-foreground">{tab.schema}.</span>}
                    {label}
                  </span>
                  <Tooltip
                    content={dirty ? "Close (unsaved changes)" : "Close"}
                    shortcut={active ? "Ctrl+W" : undefined}
                  >
                    <button
                      type="button"
                      tabIndex={-1}
                      aria-label={`Close ${label}`}
                      onClick={(e) => {
                        e.stopPropagation();
                        onClose(tab.id);
                      }}
                      className={cn(
                        "flex size-5 shrink-0 items-center justify-center text-subtle-foreground hover:bg-surface-2 hover:text-foreground",
                        !active && !dirty && "invisible group-hover:visible",
                      )}
                    >
                      {dirty ? (
                        <>
                          <span className="size-1.5 bg-primary group-hover:hidden" aria-label="Unsaved changes" />
                          <X className="hidden size-3.5 group-hover:block" />
                        </>
                      ) : (
                        <X className="size-3.5" />
                      )}
                    </button>
                  </Tooltip>
                </div>
              </ContextMenuTrigger>
              <ContextMenuContent>
                <ContextMenuItem onSelect={() => onClose(tab.id)} shortcut="Ctrl+W">
                  <X /> Close
                </ContextMenuItem>
                <ContextMenuItem
                  disabled={tabs.length < 2}
                  onSelect={() => onCloseMany(tabs.filter((t) => t.id !== tab.id).map((t) => t.id))}
                >
                  Close others
                </ContextMenuItem>
                <ContextMenuItem
                  disabled={index === tabs.length - 1}
                  onSelect={() => onCloseMany(tabs.slice(index + 1).map((t) => t.id))}
                >
                  Close tabs to the right
                </ContextMenuItem>
                <ContextMenuItem
                  disabled={index === 0}
                  onSelect={() => onCloseMany(tabs.slice(0, index).map((t) => t.id))}
                >
                  Close tabs to the left
                </ContextMenuItem>
                <ContextMenuItem onSelect={() => onCloseMany(tabs.map((t) => t.id))}>Close all</ContextMenuItem>
                <ContextMenuSeparator />
                {tab.kind === "query" ? (
                  <>
                    <ContextMenuItem onSelect={() => onDuplicate(tab.id)}>
                      <CopyPlus /> Duplicate query
                    </ContextMenuItem>
                    <ContextMenuItem disabled={!tab.sql.trim()} onSelect={() => void copyToClipboard(tab.sql)}>
                      <Copy /> Copy SQL
                    </ContextMenuItem>
                  </>
                ) : (
                  <>
                    <ContextMenuItem onSelect={() => void copyToClipboard(tab.name)}>
                      <Copy /> Copy name
                    </ContextMenuItem>
                    <ContextMenuItem onSelect={() => void copyToClipboard(`${tab.schema}.${tab.name}`)}>
                      <Copy /> Copy qualified name
                    </ContextMenuItem>
                  </>
                )}
              </ContextMenuContent>
            </ContextMenu>
          );
        })}
      </div>
      <Tooltip content="New query" shortcut="Ctrl+T">
        <Button
          variant="ghost"
          size="icon"
          aria-label="New query tab"
          onClick={onNewQuery}
          className="h-full w-9 shrink-0"
        >
          <Plus />
        </Button>
      </Tooltip>
      <div className="flex-1" />
    </div>
  );
}
