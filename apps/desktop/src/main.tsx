import { StrictMode } from "react";
import * as React from "react";
import { createRoot } from "react-dom/client";
import { AlertTriangle, Inbox, Loader2, RefreshCw } from "lucide-react";
import { listInboxBlips, type InboxBlip } from "./daemon";
import "./styles.css";

type LoadState =
  | { status: "loading" }
  | { status: "ready"; blips: InboxBlip[] }
  | { status: "error"; message: string };

function App() {
  const [state, setState] = React.useState<LoadState>({ status: "loading" });

  const refresh = React.useCallback(() => {
    setState({ status: "loading" });
    listInboxBlips()
      .then((response) => setState({ status: "ready", blips: response.blips }))
      .catch((error: unknown) => {
        const message = error instanceof Error ? error.message : "Unable to load inbox";
        setState({ status: "error", message });
      });
  }, []);

  React.useEffect(() => {
    refresh();
  }, [refresh]);

  const statusLabel =
    state.status === "ready"
      ? state.blips.length === 1
        ? "1 blip"
        : `${state.blips.length} blips`
      : state.status === "loading"
        ? "Loading"
        : "Unavailable";

  return (
    <main className="app-shell">
      <header className="topbar">
        <div className="brand-lockup">
          <Inbox aria-hidden="true" size={22} />
          <div>
            <h1>Inbox</h1>
            <p>{statusLabel}</p>
          </div>
        </div>
        <button className="icon-button" type="button" onClick={refresh} aria-label="Refresh inbox">
          <RefreshCw aria-hidden="true" size={18} />
        </button>
      </header>

      <section className="content-band" aria-live="polite">
        {state.status === "loading" ? <LoadingState /> : null}
        {state.status === "error" ? <ErrorState message={state.message} /> : null}
        {state.status === "ready" && state.blips.length === 0 ? <EmptyState /> : null}
        {state.status === "ready" && state.blips.length > 0 ? (
          <InboxList blips={state.blips} />
        ) : null}
      </section>
    </main>
  );
}

function LoadingState() {
  return (
    <div className="state-panel">
      <Loader2 className="spin" aria-hidden="true" size={22} />
      <span>Loading inbox</span>
    </div>
  );
}

function ErrorState({ message }: { message: string }) {
  return (
    <div className="state-panel error">
      <AlertTriangle aria-hidden="true" size={22} />
      <span>{message}</span>
    </div>
  );
}

function EmptyState() {
  return (
    <div className="empty-state">
      <Inbox aria-hidden="true" size={24} />
      <h2>No inbox blips</h2>
    </div>
  );
}

function InboxList({ blips }: { blips: InboxBlip[] }) {
  return (
    <div className="inbox-list">
      {blips.map((blip) => (
        <article className="blip-row" key={blip.id}>
          <div className="blip-copy">
            <h2>{blip.preview || "Untitled blip"}</h2>
            <p>{blip.id}</p>
          </div>
          <span className="byte-count">{formatBytes(blip.size_bytes)}</span>
        </article>
      ))}
    </div>
  );
}

function formatBytes(sizeBytes: number) {
  if (sizeBytes < 1024) {
    return `${sizeBytes} B`;
  }

  return `${(sizeBytes / 1024).toFixed(1)} KB`;
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>
);
