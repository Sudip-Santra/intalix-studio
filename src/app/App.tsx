import { useState } from "react";
import { client, type ConnectionProfile } from "@/services/client";
import { TooltipProvider } from "@/components/ui/tooltip";
import { ConnectPage } from "@/features/connections/ConnectPage";
import { WorkspacePage } from "@/features/workspace/WorkspacePage";
import { useThemeSync } from "./theme";

type Route = { kind: "connect" } | { kind: "workspace"; profile: ConnectionProfile };

export function App() {
  useThemeSync();
  const [route, setRoute] = useState<Route>({ kind: "connect" });
  const connection = route.kind === "workspace" ? route.profile : null;

  return (
    <TooltipProvider delayDuration={150} skipDelayDuration={300}>
      <div className="flex h-full min-w-0 flex-col">
        {/* Production is impossible to miss: a red rule across the whole window. */}
        {connection?.environment === "production" && <div className="h-0.5 shrink-0 bg-env-production" />}
        <div className="min-h-0 flex-1">
          {route.kind === "connect" ? (
            <ConnectPage onConnected={(profile) => setRoute({ kind: "workspace", profile })} />
          ) : (
            // Keyed by connection so switching starts a clean workspace.
            <WorkspacePage
              key={route.profile.id}
              profile={route.profile}
              onSwitch={(profile) => setRoute({ kind: "workspace", profile })}
              onManageConnections={() => setRoute({ kind: "connect" })}
              onDisconnect={() => {
                // Close the server connections; failure here only means they were already gone.
                client.connections.disconnect(route.profile.id).catch(() => undefined);
                setRoute({ kind: "connect" });
              }}
            />
          )}
        </div>
      </div>
    </TooltipProvider>
  );
}
