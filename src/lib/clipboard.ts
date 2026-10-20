/** Copies text to the clipboard. Resolves false instead of throwing when access is denied. */
export async function copyToClipboard(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}

const isEditable = (el: EventTarget | null) =>
  el instanceof HTMLElement &&
  (el.isContentEditable || el.closest("input, textarea, [contenteditable='true']") !== null);

/**
 * Replaces the webview's browser menu (Back, Reload, Print, Inspect) with nothing, so only
 * the app's own context menus appear. Text fields keep the native Cut/Copy/Paste menu.
 * In development, Shift+right-click still opens the browser menu for Inspect.
 */
export function suppressBrowserContextMenu(): () => void {
  const handler = (e: MouseEvent) => {
    if (isEditable(e.target)) return;
    if (import.meta.env.DEV && e.shiftKey) return;
    e.preventDefault();
  };
  document.addEventListener("contextmenu", handler);
  return () => document.removeEventListener("contextmenu", handler);
}
