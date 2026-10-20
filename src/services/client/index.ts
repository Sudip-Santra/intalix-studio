import { mockClient } from "./mock-client";
import { tauriClient } from "./tauri-client";
import type { IntalixClient } from "./types";

/** True inside the desktop app; false when the UI runs in a plain browser during development. */
const inDesktopApp = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

// The desktop app always uses the real backend. The simulated client only serves browser
// previews, and every screen labels its data as simulated.
export const client: IntalixClient = inDesktopApp ? tauriClient : mockClient;

export { ClientError } from "./tauri-client";
export * from "./types";
