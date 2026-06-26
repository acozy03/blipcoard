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
  getBlip,
  listWorkspaceBlips,
  listWorkspaces,
  type BlipDetail,
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

type DetailState =
  | { status: "idle" }
  | { status: "loading"; blipId: string }
  | { status: "ready"; blip: BlipDetail }
  | { status: "error"; blipId: string; message: string };

function App() {
  const [workspaceState, setWorkspaceState] = React.useState<WorkspaceState>({
    status: "loading"
  });
  const [selectedWorkspace, setSelectedWorkspace] = React.useState<string | null>(null);
  const selectedWorkspaceRef = React.useRef<string | null>(null);
  const blipRequestRef = React.useRef(0);
  const detailRequestRef = React.useRef(0);
  const [blipState, setBlipState] = React.useState<BlipState>({ status: "idle" });
  const [selectedBlipId, setSelectedBlipId] = React.useState<string | null>(null);
  const [detailState, setDetailState] = React.useState<DetailState>({ status: "idle" });

  React.useEffect(() => {
    selectedWorkspaceRef.current = selectedWorkspace;
  }, [selectedWorkspace]);

  const loadDetail = React.useCallback((blipId: string) => {
    const requestId = detailRequestRef.current + 1;
    detailRequestRef.current = requestId;
    setSelectedBlipId(blipId);
    setDetailState({ status: "loading", blipId });
    getBlip(blipId)
      .then((blip) => {
        if (detailRequestRef.current === requestId) {
          setDetailState({ status: "ready", blip });
        }
      })
      .catch((error: unknown) => {
        if (detailRequestRef.current !== requestId) {
          return;
        }

        const message = error instanceof Error ? error.message : "Unable to load blip";
        setDetailState({ status: "error", blipId, message });
      });
  }, []);

  const loadBlips = React.useCallback((workspace: string) => {
    const requestId = blipRequestRef.current + 1;
    blipRequestRef.current = requestId;
    setBlipState({ status: "loading", workspace });
    listWorkspaceBlips(workspace)
      .then((response) => {
        if (blipRequestRef.current === requestId) {
          setBlipState({ status: "ready", workspace: response.workspace, blips: response.blips });
          const firstBlip = response.blips[0] ?? null;

          if (firstBlip) {
            loadDetail(firstBlip.id);
          } else {
            setSelectedBlipId(null);
            setDetailState({ status: "idle" });
          }
        }
      })
      .catch((error: unknown) => {
        if (blipRequestRef.current !== requestId) {
          return;
        }

        const message = error instanceof Error ? error.message : "Unable to load workspace";
        setBlipState({ status: "error", workspace, message });
        setSelectedBlipId(null);
        setDetailState({ status: "idle" });
      });
  }, [loadDetail]);

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
          setSelectedBlipId(null);
          setDetailState({ status: "idle" });
        }
      })
      .catch((error: unknown) => {
        const message = error instanceof Error ? error.message : "Unable to load workspaces";
        setWorkspaceState({ status: "error", message });
        setBlipState({ status: "idle" });
        setSelectedBlipId(null);
        setDetailState({ status: "idle" });
      });
  }, [loadBlips]);

  React.useEffect(() => {
    refresh();
  }, [refresh]);

  const selectWorkspace = (workspace: string) => {
    setSelectedWorkspace(workspace);
    setSelectedBlipId(null);
    setDetailState({ status: "idle" });
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
              activeWorkspace={
                workspaceState.status === "ready" ? workspaceState.activeWorkspace : null
              }
              workspace={selectedSummary}
              onActivate={setActiveWorkspace}
            />
          ) : null}
          <div className="workspace-main">
            <BlipPanel
              selectedBlipId={selectedBlipId}
              state={blipState}
              onSelectBlip={loadDetail}
            />
            <BlipDetailPanel
              activeWorkspace={
                workspaceState.status === "ready" ? workspaceState.activeWorkspace : null
              }
              state={detailState}
            />
          </div>
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

function BlipPanel({
  selectedBlipId,
  state,
  onSelectBlip
}: {
  selectedBlipId: string | null;
  state: BlipState;
  onSelectBlip: (blipId: string) => void;
}) {
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

  return <BlipList blips={state.blips} selectedBlipId={selectedBlipId} onSelect={onSelectBlip} />;
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

function BlipList({
  blips,
  selectedBlipId,
  onSelect
}: {
  blips: BlipSummary[];
  selectedBlipId: string | null;
  onSelect: (blipId: string) => void;
}) {
  return (
    <div className="blip-list">
      {blips.map((blip) => (
        <button
          className={blip.id === selectedBlipId ? "blip-row selected" : "blip-row"}
          key={blip.id}
          type="button"
          onClick={() => onSelect(blip.id)}
        >
          <div className="blip-copy">
            <h2>{blip.preview || "Untitled blip"}</h2>
            <p>{blip.id}</p>
          </div>
          <span className="byte-count">{formatBytes(blip.size_bytes)}</span>
        </button>
      ))}
    </div>
  );
}

function BlipDetailPanel({
  activeWorkspace,
  state
}: {
  activeWorkspace: string | null;
  state: DetailState;
}) {
  if (state.status === "idle") {
    return <EmptyState title="No blip selected" />;
  }

  if (state.status === "loading") {
    return <LoadingState label="Loading detail" />;
  }

  if (state.status === "error") {
    return <ErrorState message={state.message} />;
  }

  const blip = state.blip;

  return (
    <article className="detail-panel">
      <header className="detail-header">
        <div>
          <p>Blip detail</p>
          <h2>{blip.id}</h2>
        </div>
        <span className={blip.is_redacted ? "redaction-badge redacted" : "redaction-badge"}>
          {blip.is_redacted ? "Redacted" : "Unredacted"}
        </span>
      </header>

      <pre className="detail-content">{blip.content}</pre>

      <dl className="metadata-grid">
        <div>
          <dt>Workspace</dt>
          <dd>{blip.workspace}</dd>
        </div>
        <div>
          <dt>Active workspace</dt>
          <dd>{activeWorkspace ?? "None"}</dd>
        </div>
        <div>
          <dt>Content type</dt>
          <dd>{formatMetadata(blip.content_type)}</dd>
        </div>
        <div>
          <dt>Source</dt>
          <dd>{blip.source_app ?? "Unknown"}</dd>
        </div>
        <div>
          <dt>Language</dt>
          <dd>{blip.language ?? "None"}</dd>
        </div>
        <div>
          <dt>Size</dt>
          <dd>{formatBytes(blip.size_bytes)}</dd>
        </div>
        <div>
          <dt>Tokens</dt>
          <dd>{blip.token_estimate ?? "Unknown"}</dd>
        </div>
        <div>
          <dt>Created</dt>
          <dd>{formatTimestamp(blip.created_at)}</dd>
        </div>
        <div className="metadata-wide">
          <dt>Tags</dt>
          <dd>{blip.tags.length > 0 ? blip.tags.join(", ") : "None"}</dd>
        </div>
      </dl>
    </article>
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

function formatMetadata(value: string) {
  return value.replaceAll("_", " ");
}

function formatTimestamp(value: string) {
  const date = new Date(value);

  if (Number.isNaN(date.getTime())) {
    return value;
  }

  return date.toLocaleString();
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
