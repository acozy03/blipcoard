import { StrictMode } from "react";
import * as React from "react";
import { createRoot } from "react-dom/client";
import {
  AlertTriangle,
  CheckCircle2,
  Inbox,
  Loader2,
  RefreshCw,
  Shield,
  UserRound,
  Workflow
} from "lucide-react";
import {
  activateWorkspace,
  currentWorkspace,
  listWorkspaceBlips,
  listWorkspaces,
  type BlipSummary,
  type WorkspaceSummary
} from "./daemon";
import "./styles.css";

type WorkspaceState =
  | { status: "loading" }
  | {
      status: "ready";
      workspaces: WorkspaceSummary[];
      activeWorkspace: string | null;
    }
  | { status: "error"; message: string };

type BlipState =
  | { status: "idle" }
  | { status: "loading"; workspace: string }
  | { status: "ready"; workspace: string; blips: BlipSummary[] }
  | { status: "error"; workspace: string; message: string };

function App() {
  const [workspaceState, setWorkspaceState] = React.useState<WorkspaceState>({
    status: "loading"
  });
  const [selectedWorkspace, setSelectedWorkspace] = React.useState<string | null>(null);
  const selectedWorkspaceRef = React.useRef<string | null>(null);
  const blipRequestRef = React.useRef(0);
  const [blipState, setBlipState] = React.useState<BlipState>({ status: "idle" });

  React.useEffect(() => {
    selectedWorkspaceRef.current = selectedWorkspace;
  }, [selectedWorkspace]);

  const loadBlips = React.useCallback((workspace: string) => {
    const requestId = blipRequestRef.current + 1;
    blipRequestRef.current = requestId;
    setBlipState({ status: "loading", workspace });
    listWorkspaceBlips(workspace)
      .then((response) => {
        if (blipRequestRef.current === requestId) {
          setBlipState({ status: "ready", workspace: response.workspace, blips: response.blips });
        }
      })
      .catch((error: unknown) => {
        if (blipRequestRef.current !== requestId) {
          return;
        }

        const message = error instanceof Error ? error.message : "Unable to load workspace";
        setBlipState({ status: "error", workspace, message });
      });
  }, []);

  const refresh = React.useCallback(() => {
    setWorkspaceState({ status: "loading" });
    Promise.all([listWorkspaces(), currentWorkspace()])
      .then(([workspaceResponse, currentResponse]) => {
        const workspaces = workspaceResponse.workspaces;
        const activeWorkspace = currentResponse.active_workspace;
        const currentSelection = selectedWorkspaceRef.current;
        const existingSelection =
          currentSelection && workspaces.some((workspace) => workspace.name === currentSelection)
            ? currentSelection
            : null;
        const nextSelection = existingSelection ?? activeWorkspace ?? workspaces[0]?.name ?? null;

        setWorkspaceState({ status: "ready", workspaces, activeWorkspace });
        setSelectedWorkspace(nextSelection);

        if (nextSelection) {
          loadBlips(nextSelection);
        } else {
          setBlipState({ status: "idle" });
        }
      })
      .catch((error: unknown) => {
        const message = error instanceof Error ? error.message : "Unable to load workspaces";
        setWorkspaceState({ status: "error", message });
        setBlipState({ status: "idle" });
      });
  }, [loadBlips]);

  React.useEffect(() => {
    refresh();
  }, [refresh]);

  const selectWorkspace = (workspace: string) => {
    setSelectedWorkspace(workspace);
    loadBlips(workspace);
  };

  const setActiveWorkspace = () => {
    if (!selectedWorkspace || workspaceState.status !== "ready") {
      return;
    }

    activateWorkspace(selectedWorkspace)
      .then((response) =>
        setWorkspaceState({
          ...workspaceState,
          activeWorkspace: response.active_workspace
        })
      )
      .catch((error: unknown) => {
        const message = error instanceof Error ? error.message : "Unable to activate workspace";
        setWorkspaceState({ status: "error", message });
      });
  };

  const selectedSummary =
    workspaceState.status === "ready"
      ? workspaceState.workspaces.find((workspace) => workspace.name === selectedWorkspace)
      : null;
  const statusLabel = workspaceStatusLabel(workspaceState, selectedWorkspace);
  const activeWorkspace =
    workspaceState.status === "ready" ? workspaceState.activeWorkspace : null;

  return (
    <main className="app-shell">
      <header className="topbar">
        <div className="brand-lockup">
          <Workflow aria-hidden="true" size={22} />
          <div>
            <h1>Workspaces</h1>
            <p>{statusLabel}</p>
          </div>
        </div>
        <div className="topbar-actions">
          <ActiveWorkspaceBadge state={workspaceState.status} workspace={activeWorkspace} />
          <button className="icon-button" type="button" onClick={refresh} aria-label="Refresh">
            <RefreshCw aria-hidden="true" size={18} />
          </button>
        </div>
      </header>

      <section className="workspace-layout" aria-live="polite">
        <aside className="workspace-sidebar">
          {workspaceState.status === "loading" ? <LoadingState label="Loading workspaces" /> : null}
          {workspaceState.status === "error" ? (
            <ErrorState message={workspaceState.message} />
          ) : null}
          {workspaceState.status === "ready" ? (
            <WorkspaceList
              activeWorkspace={workspaceState.activeWorkspace}
              selectedWorkspace={selectedWorkspace}
              workspaces={workspaceState.workspaces}
              onSelect={selectWorkspace}
            />
          ) : null}
        </aside>

        <section className="workspace-content">
          {selectedSummary ? (
            <WorkspaceHeader
              activeWorkspace={workspaceState.status === "ready" ? workspaceState.activeWorkspace : null}
              workspace={selectedSummary}
              onActivate={setActiveWorkspace}
            />
          ) : null}
          <BlipPanel state={blipState} />
        </section>
      </section>
    </main>
  );
}

function ActiveWorkspaceBadge({
  state,
  workspace
}: {
  state: WorkspaceState["status"];
  workspace: string | null;
}) {
  const label =
    state === "loading" ? "Loading" : state === "error" ? "Unavailable" : (workspace ?? "None");

  return (
    <div className="active-badge" aria-live="polite">
      <CheckCircle2 aria-hidden="true" size={15} />
      <span>Active</span>
      <strong>{label}</strong>
    </div>
  );
}

function WorkspaceList({
  activeWorkspace,
  selectedWorkspace,
  workspaces,
  onSelect
}: {
  activeWorkspace: string | null;
  selectedWorkspace: string | null;
  workspaces: WorkspaceSummary[];
  onSelect: (workspace: string) => void;
}) {
  if (workspaces.length === 0) {
    return <EmptyState title="No workspaces" />;
  }

  return (
    <div className="workspace-list">
      {workspaces.map((workspace) => {
        const isActive = workspace.name === activeWorkspace;
        const isSelected = workspace.name === selectedWorkspace;

        return (
          <button
            className={isSelected ? "workspace-item selected" : "workspace-item"}
            key={workspace.name}
            type="button"
            onClick={() => onSelect(workspace.name)}
          >
            <span className="workspace-name">{workspace.name}</span>
            <span className="workspace-meta">
              {workspace.agent_access ? (
                <>
                  <Shield aria-hidden="true" size={14} />
                  Agent-readable
                </>
              ) : (
                <>
                  <UserRound aria-hidden="true" size={14} />
                  Human-only
                </>
              )}
            </span>
            {isActive ? (
              <span className="active-marker">
                <CheckCircle2 aria-hidden="true" size={14} />
                Active
              </span>
            ) : null}
          </button>
        );
      })}
    </div>
  );
}

function WorkspaceHeader({
  activeWorkspace,
  workspace,
  onActivate
}: {
  activeWorkspace: string | null;
  workspace: WorkspaceSummary;
  onActivate: () => void;
}) {
  const isActive = workspace.name === activeWorkspace;

  return (
    <div className="workspace-header">
      <div className="workspace-title">
        <Inbox aria-hidden="true" size={20} />
        <div>
          <h2>{workspace.name}</h2>
          <p>{workspace.agent_access ? "Agent-readable" : "Human-only"}</p>
        </div>
      </div>
      <button className="text-button" type="button" onClick={onActivate} disabled={isActive}>
        {isActive ? "Active" : "Set active"}
      </button>
    </div>
  );
}

function BlipPanel({ state }: { state: BlipState }) {
  if (state.status === "idle") {
    return <EmptyState title="No workspace selected" />;
  }

  if (state.status === "loading") {
    return <LoadingState label="Loading blips" />;
  }

  if (state.status === "error") {
    return <ErrorState message={state.message} />;
  }

  if (state.blips.length === 0) {
    return <EmptyState title="No blips in workspace" />;
  }

  return <BlipList blips={state.blips} />;
}

function LoadingState({ label }: { label: string }) {
  return (
    <div className="state-panel">
      <Loader2 className="spin" aria-hidden="true" size={22} />
      <span>{label}</span>
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

function EmptyState({ title }: { title: string }) {
  return (
    <div className="empty-state">
      <Inbox aria-hidden="true" size={24} />
      <h2>{title}</h2>
    </div>
  );
}

function BlipList({ blips }: { blips: BlipSummary[] }) {
  return (
    <div className="blip-list">
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

function workspaceStatusLabel(state: WorkspaceState, selectedWorkspace: string | null) {
  if (state.status === "loading") {
    return "Loading";
  }

  if (state.status === "error") {
    return "Unavailable";
  }

  const count = state.workspaces.length === 1 ? "1 workspace" : `${state.workspaces.length} workspaces`;
  return selectedWorkspace ? `${selectedWorkspace} / ${count}` : count;
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
