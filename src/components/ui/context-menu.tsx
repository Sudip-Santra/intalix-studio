import * as React from "react";
import { ContextMenu as MenuPrimitive } from "radix-ui";
import { ChevronRight } from "lucide-react";
import { cn } from "@/lib/utils";

const ContextMenu = MenuPrimitive.Root;
const ContextMenuTrigger = MenuPrimitive.Trigger;
const ContextMenuSub = MenuPrimitive.Sub;

const contentClass =
  "z-50 min-w-[220px] border border-border-strong bg-surface p-1 text-foreground shadow-[0_12px_32px_-8px_rgb(0_0_0/0.45)]";

function ContextMenuContent({ className, ...props }: React.ComponentProps<typeof MenuPrimitive.Content>) {
  return (
    <MenuPrimitive.Portal>
      <MenuPrimitive.Content collisionPadding={8} className={cn(contentClass, className)} {...props} />
    </MenuPrimitive.Portal>
  );
}

const itemClass = cn(
  "flex h-7 cursor-pointer items-center gap-2 px-2 text-[12.5px] outline-none select-none",
  "data-[disabled]:pointer-events-none data-[disabled]:opacity-45 data-[highlighted]:bg-surface-2",
  "[&_svg]:size-3.5 [&_svg]:shrink-0 [&_svg]:text-muted-foreground",
);

function ContextMenuItem({
  className,
  shortcut,
  children,
  ...props
}: React.ComponentProps<typeof MenuPrimitive.Item> & { shortcut?: string }) {
  return (
    <MenuPrimitive.Item className={cn(itemClass, className)} {...props}>
      {children}
      {shortcut && <span className="ml-auto pl-6 font-mono text-[10.5px] text-subtle-foreground">{shortcut}</span>}
    </MenuPrimitive.Item>
  );
}

function ContextMenuSubTrigger({
  className,
  children,
  ...props
}: React.ComponentProps<typeof MenuPrimitive.SubTrigger>) {
  return (
    <MenuPrimitive.SubTrigger className={cn(itemClass, "data-[state=open]:bg-surface-2", className)} {...props}>
      {children}
      <ChevronRight className="ml-auto" />
    </MenuPrimitive.SubTrigger>
  );
}

function ContextMenuSubContent({ className, ...props }: React.ComponentProps<typeof MenuPrimitive.SubContent>) {
  return (
    <MenuPrimitive.Portal>
      <MenuPrimitive.SubContent sideOffset={2} className={cn(contentClass, className)} {...props} />
    </MenuPrimitive.Portal>
  );
}

function ContextMenuLabel({ className, ...props }: React.ComponentProps<typeof MenuPrimitive.Label>) {
  return (
    <MenuPrimitive.Label
      className={cn("truncate px-2 pt-1 pb-1.5 font-mono text-[11px] text-subtle-foreground", className)}
      {...props}
    />
  );
}

function ContextMenuSeparator({ className, ...props }: React.ComponentProps<typeof MenuPrimitive.Separator>) {
  return <MenuPrimitive.Separator className={cn("my-1 h-px bg-border", className)} {...props} />;
}

export {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
  ContextMenuTrigger,
};
