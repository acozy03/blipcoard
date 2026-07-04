import { StrictMode } from "react";
import * as React from "react";
import { createRoot } from "react-dom/client";
import {
  AlertTriangle,
  CheckCircle2,
  ClipboardCopy,
  Code2,
  FileQuestion,
  FileText,
  Image,
  Inbox,
  Keyboard,
  Loader2,
  RefreshCw,
  Share2,
  Shield,
  UploadCloud,
  UserRound
} from "lucide-react";
import {
  activateWorkspace,
  currentWorkspace,
  exportPayload,
  getBlip,
  getPayloadPreview,
  hostedJoinWorkspace,
  hostedPublishBlip,
  hostedSetStickyShare,
  hostedStatus,
  listAuditEvents,
  listWorkspaceBlips,
  listWorkspaces,
  registerGlobalShortcuts,
  routeLatestInboxBlip,
  setStickyCapture,
  type AuditEventSummary,
  type BlipDetail,
  type BlipSummary,
  type HostedStatusResponse,
  type PayloadSummary,
  type PayloadBytesResponse,
  type ShortcutRegistration,
  type WorkspaceSummary
} from "./daemon";
import "./styles.css";

const DETAIL_TEXT_LIMIT = 12000;
const AUTO_REFRESH_INTERVAL_MS = 1500;

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

type AuditState =
  | { status: "loading" }
  | { status: "ready"; events: AuditEventSummary[] }
  | { status: "error"; message: string };

type ShortcutState =
  | { status: "loading" }
  | { status: "ready"; shortcuts: ShortcutRegistration[] }
  | { status: "error"; message: string };

type SendState =
  | { status: "idle" }
  | { status: "sending"; workspace: string }
  | { status: "sent"; message: string }
  | { status: "error"; message: string };

type HostedState =
  | { status: "loading" }
  | { status: "ready"; hosted: HostedStatusResponse }
  | { status: "error"; message: string };

type HostedActionState =
  | { status: "idle" }
  | { status: "joining" }
  | { status: "publishing" }
  | { status: "updating" }
  | { status: "done"; message: string }
  | { status: "error"; message: string };

function App() {
  const [workspaceState, setWorkspaceState] = React.useState<WorkspaceState>({
    status: "loading"
  });
  const [selectedWorkspace, setSelectedWorkspace] = React.useState<string | null>(null);
  const selectedWorkspaceRef = React.useRef<string | null>(null);
  const selectedBlipIdRef = React.useRef<string | null>(null);
  const blipRequestRef = React.useRef(0);
  const detailRequestRef = React.useRef(0);
  const [blipState, setBlipState] = React.useState<BlipState>({ status: "idle" });
  const [selectedBlipId, setSelectedBlipId] = React.useState<string | null>(null);
  const [detailState, setDetailState] = React.useState<DetailState>({ status: "idle" });
  const [auditState, setAuditState] = React.useState<AuditState>({ status: "loading" });
  const [shortcutState, setShortcutState] = React.useState<ShortcutState>({ status: "loading" });
  const [sendState, setSendState] = React.useState<SendState>({ status: "idle" });
  const [hostedState, setHostedState] = React.useState<HostedState>({ status: "loading" });
  const [hostedActionState, setHostedActionState] = React.useState<HostedActionState>({
    status: "idle"
  });
  const [joinServiceUrl, setJoinServiceUrl] = React.useState("http://127.0.0.1:8732");
  const [joinCode, setJoinCode] = React.useState("");
  const [joinDisplayName, setJoinDisplayName] = React.useState("");

  React.useEffect(() => {
    selectedWorkspaceRef.current = selectedWorkspace;
  }, [selectedWorkspace]);

  React.useEffect(() => {
    selectedBlipIdRef.current = selectedBlipId;
  }, [selectedBlipId]);

  const loadDetail = React.useCallback((blipId: string) => {
    const requestId = detailRequestRef.current + 1;
    detailRequestRef.current = requestId;
    selectedBlipIdRef.current = blipId;
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

  const loadBlips = React.useCallback((workspace: string, options: { silent?: boolean } = {}) => {
    const requestId = blipRequestRef.current + 1;
    blipRequestRef.current = requestId;
    if (!options.silent) {
      setBlipState({ status: "loading", workspace });
    }
    listWorkspaceBlips(workspace)
      .then((response) => {
        if (selectedWorkspaceRef.current !== workspace) {
          return;
        }

        if (blipRequestRef.current === requestId) {
          setBlipState({ status: "ready", workspace: response.workspace, blips: response.blips });
          const currentSelection = selectedBlipIdRef.current;
          const selectedStillExists =
            currentSelection &&
            response.blips.some((blip) => blip.id === currentSelection);

          if (selectedStillExists) {
            return;
          }

          const firstBlip = response.blips[0] ?? null;

          if (firstBlip) {
            loadDetail(firstBlip.id);
          } else {
            selectedBlipIdRef.current = null;
            setSelectedBlipId(null);
            setDetailState({ status: "idle" });
          }
        }
      })
      .catch((error: unknown) => {
        if (selectedWorkspaceRef.current !== workspace) {
          return;
        }

        if (blipRequestRef.current !== requestId) {
          return;
        }

        if (options.silent) {
          return;
        }

        const message = error instanceof Error ? error.message : "Unable to load workspace";
        setBlipState({ status: "error", workspace, message });
        selectedBlipIdRef.current = null;
        setSelectedBlipId(null);
        setDetailState({ status: "idle" });
      });
  }, [loadDetail]);

  const loadAudit = React.useCallback(() => {
    setAuditState({ status: "loading" });
    listAuditEvents()
      .then((response) => setAuditState({ status: "ready", events: response.events }))
      .catch((error: unknown) => {
        const message = error instanceof Error ? error.message : "Unable to load audit events";
        setAuditState({ status: "error", message });
      });
  }, []);

  const loadShortcuts = React.useCallback(() => {
    setShortcutState({ status: "loading" });
    registerGlobalShortcuts()
      .then((response) =>
        setShortcutState({ status: "ready", shortcuts: response.shortcuts })
      )
      .catch(() => {
        setShortcutState({
          status: "ready",
          shortcuts: [
            {
              id: "route-latest-to-active",
              label: "Route latest to active",
              accelerator: "Ctrl+Alt+B",
              action: "route_latest_inbox_to_active_workspace",
              state: "unsupported",
              message: "Global shortcuts require desktop support from the running app"
            }
          ]
        });
      });
  }, []);

  const loadHosted = React.useCallback(() => {
    hostedStatus()
      .then((hosted) => setHostedState({ status: "ready", hosted }))
      .catch((error: unknown) => {
        const message = error instanceof Error ? error.message : "Unable to load hosted status";
        setHostedState({ status: "error", message });
      });
  }, []);

  const refresh = React.useCallback(() => {
    setWorkspaceState({ status: "loading" });
    loadAudit();
    loadHosted();
    loadShortcuts();
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

        selectedWorkspaceRef.current = nextSelection;
        setWorkspaceState({ status: "ready", workspaces, activeWorkspace });
        setSelectedWorkspace(nextSelection);

        if (nextSelection) {
          loadBlips(nextSelection);
        } else {
          setBlipState({ status: "idle" });
          selectedBlipIdRef.current = null;
          setSelectedBlipId(null);
          setDetailState({ status: "idle" });
        }
      })
      .catch((error: unknown) => {
        const message = error instanceof Error ? error.message : "Unable to load workspaces";
        setWorkspaceState({ status: "error", message });
        setBlipState({ status: "idle" });
        selectedBlipIdRef.current = null;
        setSelectedBlipId(null);
        setDetailState({ status: "idle" });
      });
  }, [loadAudit, loadBlips, loadHosted, loadShortcuts]);

  React.useEffect(() => {
    refresh();
  }, [refresh]);

  React.useEffect(() => {
    if (!selectedWorkspace) {
      return;
    }

    const intervalId = window.setInterval(() => {
      const workspace = selectedWorkspaceRef.current;
      if (workspace) {
        loadBlips(workspace, { silent: true });
      }
    }, AUTO_REFRESH_INTERVAL_MS);

    return () => window.clearInterval(intervalId);
  }, [loadBlips, selectedWorkspace]);

  const selectWorkspace = (workspace: string) => {
    selectedWorkspaceRef.current = workspace;
    setSelectedWorkspace(workspace);
    selectedBlipIdRef.current = null;
    setSelectedBlipId(null);
    setDetailState({ status: "idle" });
    loadBlips(workspace);
  };

  const setActiveWorkspace = () => {
    if (!selectedWorkspace || workspaceState.status !== "ready") {
      return;
    }

    activateWorkspace(selectedWorkspace)
      .then((response) => {
        setWorkspaceState({
          ...workspaceState,
          activeWorkspace: response.active_workspace
        });
        loadAudit();
      })
      .catch((error: unknown) => {
        const message = error instanceof Error ? error.message : "Unable to activate workspace";
        setWorkspaceState({ status: "error", message });
      });
  };

  const toggleStickyCapture = () => {
    if (!selectedSummary || workspaceState.status !== "ready") {
      return;
    }

    setStickyCapture(selectedSummary.name, !selectedSummary.sticky_capture)
      .then((updatedWorkspace) => {
        setWorkspaceState({
          ...workspaceState,
          workspaces: workspaceState.workspaces.map((workspace) => ({
            ...workspace,
            sticky_capture:
              updatedWorkspace.sticky_capture && workspace.name !== updatedWorkspace.name
                ? false
                : workspace.name === updatedWorkspace.name
                  ? updatedWorkspace.sticky_capture
                  : workspace.sticky_capture
          }))
        });
        loadAudit();
      })
      .catch((error: unknown) => {
        const message = error instanceof Error ? error.message : "Unable to update sticky mode";
        setWorkspaceState({ status: "error", message });
      });
  };

  const sendLatestToSelectedWorkspace = () => {
    if (!selectedSummary || selectedSummary.name === "inbox") {
      return;
    }

    setSendState({ status: "sending", workspace: selectedSummary.name });
    routeLatestInboxBlip(selectedSummary.name)
      .then((routed) => {
        setSendState({
          status: "sent",
          message: `Sent ${routed.id} to ${routed.to_workspace}`
        });
        loadBlips(selectedSummary.name);
        loadAudit();
      })
      .catch((error: unknown) => {
        const message = error instanceof Error ? error.message : "Unable to send latest blip";
        setSendState({ status: "error", message });
      });
  };

  const joinHostedWorkspace = () => {
    setHostedActionState({ status: "joining" });
    hostedJoinWorkspace({
      service_url: joinServiceUrl,
      join_code: joinCode,
      display_name: joinDisplayName
    })
      .then((hosted) => {
        setHostedState({ status: "ready", hosted });
        setHostedActionState({
          status: "done",
          message: `Joined ${hosted.workspace_name ?? "hosted workspace"}`
        });
        loadAudit();
        refresh();
      })
      .catch((error: unknown) => {
        const message = error instanceof Error ? error.message : "Unable to join hosted workspace";
        setHostedActionState({ status: "error", message });
      });
  };

  const publishSelectedBlip = () => {
    const blipId = selectedBlipIdRef.current;
    if (!blipId) {
      return;
    }

    setHostedActionState({ status: "publishing" });
    hostedPublishBlip(blipId)
      .then((published) => {
        setHostedActionState({
          status: "done",
          message: `Published ${published.local_blip_id}`
        });
        loadAudit();
      })
      .catch((error: unknown) => {
        const message = error instanceof Error ? error.message : "Unable to publish blip";
        setHostedActionState({ status: "error", message });
      });
  };

  const toggleHostedStickyShare = () => {
    if (hostedState.status !== "ready") {
      return;
    }

    setHostedActionState({ status: "updating" });
    hostedSetStickyShare(!hostedState.hosted.sticky_share_enabled)
      .then((hosted) => {
        setHostedState({ status: "ready", hosted });
        setHostedActionState({
          status: "done",
          message: hosted.sticky_share_enabled ? "Sticky share enabled" : "Sticky share disabled"
        });
        loadAudit();
        refresh();
      })
      .catch((error: unknown) => {
        const message = error instanceof Error ? error.message : "Unable to update sticky share";
        setHostedActionState({ status: "error", message });
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
          <img src="/img/blipcoard-logo.png" alt="" aria-hidden="true" />
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
              onSendLatest={sendLatestToSelectedWorkspace}
              onToggleSticky={toggleStickyCapture}
              sendState={sendState}
            />
          ) : null}
          <HostedPanel
            actionState={hostedActionState}
            joinCode={joinCode}
            joinDisplayName={joinDisplayName}
            joinServiceUrl={joinServiceUrl}
            selectedBlipId={selectedBlipId}
            state={hostedState}
            onJoin={joinHostedWorkspace}
            onPublish={publishSelectedBlip}
            onRefresh={loadHosted}
            onServiceUrlChange={setJoinServiceUrl}
            onJoinCodeChange={setJoinCode}
            onDisplayNameChange={setJoinDisplayName}
            onToggleStickyShare={toggleHostedStickyShare}
          />
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
          <ShortcutPanel state={shortcutState} />
          <AuditPanel state={auditState} />
        </section>
      </section>
    </main>
  );
}

function HostedPanel({
  actionState,
  joinCode,
  joinDisplayName,
  joinServiceUrl,
  selectedBlipId,
  state,
  onJoin,
  onPublish,
  onRefresh,
  onServiceUrlChange,
  onJoinCodeChange,
  onDisplayNameChange,
  onToggleStickyShare
}: {
  actionState: HostedActionState;
  joinCode: string;
  joinDisplayName: string;
  joinServiceUrl: string;
  selectedBlipId: string | null;
  state: HostedState;
  onJoin: () => void;
  onPublish: () => void;
  onRefresh: () => void;
  onServiceUrlChange: (value: string) => void;
  onJoinCodeChange: (value: string) => void;
  onDisplayNameChange: (value: string) => void;
  onToggleStickyShare: () => void;
}) {
  const hosted = state.status === "ready" ? state.hosted : null;
  const connected = hosted?.connected ?? false;
  const busy =
    actionState.status === "joining" ||
    actionState.status === "publishing" ||
    actionState.status === "updating";
  const canJoin = joinServiceUrl.trim() !== "" && joinCode.trim() !== "" && joinDisplayName.trim() !== "";

  return (
    <section className="hosted-panel">
      <div className="hosted-title">
        <Share2 aria-hidden="true" size={18} />
        <div>
          <h2>Hosted share</h2>
          <p>{hostedStatusText(state)}</p>
        </div>
      </div>
      <div className="hosted-controls">
        <label>
          <span>Relay</span>
          <input
            value={joinServiceUrl}
            onChange={(event) => onServiceUrlChange(event.currentTarget.value)}
            placeholder="http://127.0.0.1:8732"
          />
        </label>
        <label>
          <span>Code</span>
          <input
            value={joinCode}
            onChange={(event) => onJoinCodeChange(event.currentTarget.value)}
            placeholder="BLIP-0000-0000-0000"
          />
        </label>
        <label>
          <span>Name</span>
          <input
            value={joinDisplayName}
            onChange={(event) => onDisplayNameChange(event.currentTarget.value)}
            placeholder="Adrian"
          />
        </label>
      </div>
      <div className="hosted-actions">
        <button className="text-button" type="button" onClick={onJoin} disabled={busy || !canJoin}>
          {actionState.status === "joining" ? "Joining" : connected ? "Rejoin" : "Join"}
        </button>
        <button
          className="text-button"
          type="button"
          onClick={onPublish}
          disabled={busy || !connected || !selectedBlipId}
        >
          <UploadCloud aria-hidden="true" size={15} />
          {actionState.status === "publishing" ? "Publishing" : "Publish blip"}
        </button>
        <button
          className="text-button"
          type="button"
          onClick={onToggleStickyShare}
          disabled={busy || !connected}
        >
          {hosted?.sticky_share_enabled ? "Share sticky on" : "Share sticky off"}
        </button>
        <button className="icon-button" type="button" onClick={onRefresh} aria-label="Refresh hosted">
          <RefreshCw aria-hidden="true" size={16} />
        </button>
      </div>
      {actionState.status === "done" || actionState.status === "error" ? (
        <p className={actionState.status === "error" ? "hosted-message error" : "hosted-message"}>
          {actionState.message}
        </p>
      ) : null}
    </section>
  );
}

function hostedStatusText(state: HostedState) {
  if (state.status === "loading") {
    return "Loading hosted status";
  }
  if (state.status === "error") {
    return state.message;
  }
  if (!state.hosted.connected) {
    return "Local-only";
  }

  const name = state.hosted.workspace_name ?? state.hosted.workspace_id ?? "workspace";
  const role = state.hosted.member_role ?? "member";
  return `${name} - ${role}`;
}

function ShortcutPanel({ state }: { state: ShortcutState }) {
  if (state.status === "loading") {
    return (
      <section className="shortcut-panel">
        <ShortcutHeader />
        <LoadingState label="Loading shortcuts" />
      </section>
    );
  }

  if (state.status === "error") {
    return (
      <section className="shortcut-panel">
        <ShortcutHeader />
        <ErrorState message={state.message} />
      </section>
    );
  }

  return (
    <section className="shortcut-panel">
      <ShortcutHeader />
      <div className="shortcut-list">
        {state.shortcuts.map((shortcut) => (
          <article className="shortcut-row" key={shortcut.id}>
            <div className="shortcut-title">
              <Keyboard aria-hidden="true" size={17} />
              <div>
                <h3>{shortcut.label}</h3>
                <p>{formatShortcutAction(shortcut.action)}</p>
              </div>
            </div>
            <kbd>{shortcut.accelerator}</kbd>
            <span className={`shortcut-state ${shortcut.state}`}>{shortcut.state}</span>
            {shortcut.message ? <p className="shortcut-message">{shortcut.message}</p> : null}
          </article>
        ))}
      </div>
    </section>
  );
}

function ShortcutHeader() {
  return (
    <header className="shortcut-header">
      <h2>Shortcuts</h2>
      <p>Routing actions</p>
    </header>
  );
}

function AuditPanel({ state }: { state: AuditState }) {
  if (state.status === "loading") {
    return (
      <section className="audit-panel">
        <AuditHeader />
        <LoadingState label="Loading audit" />
      </section>
    );
  }

  if (state.status === "error") {
    return (
      <section className="audit-panel">
        <AuditHeader />
        <ErrorState message={state.message} />
      </section>
    );
  }

  return (
    <section className="audit-panel">
      <AuditHeader />
      {state.events.length === 0 ? (
        <EmptyState title="No audit events" />
      ) : (
        <div className="audit-list">
          {state.events.map((event) => (
            <article className="audit-row" key={event.id}>
              <div className="audit-main">
                <h3>{formatAuditEvent(event.event_type)}</h3>
                <p>{formatAuditTarget(event)}</p>
              </div>
              <div className="audit-meta">
                <span>{formatAuditActor(event)}</span>
                <time dateTime={event.created_at}>{formatTimestamp(event.created_at)}</time>
              </div>
              {event.details_json ? <code>{formatAuditDetails(event.details_json)}</code> : null}
            </article>
          ))}
        </div>
      )}
    </section>
  );
}

function AuditHeader() {
  return (
    <header className="audit-header">
      <h2>Audit</h2>
      <p>Recent events</p>
    </header>
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
            {workspace.sticky_capture ? <span className="sticky-marker">Sticky</span> : null}
            {workspace.hosted_share_enabled ? (
              <span className="share-marker">
                <Share2 aria-hidden="true" size={13} />
                Shared
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
  onActivate,
  onSendLatest,
  onToggleSticky,
  sendState
}: {
  activeWorkspace: string | null;
  workspace: WorkspaceSummary;
  onActivate: () => void;
  onSendLatest: () => void;
  onToggleSticky: () => void;
  sendState: SendState;
}) {
  const isActive = workspace.name === activeWorkspace;
  const sendDisabled = workspace.name === "inbox" || sendState.status === "sending";

  return (
    <div className="workspace-header">
      <div className="workspace-title">
        <Inbox aria-hidden="true" size={20} />
        <div>
          <h2>{workspace.name}</h2>
          <p>{workspaceStatusText(workspace, sendState)}</p>
        </div>
      </div>
      <div className="workspace-actions">
        <button
          className="text-button"
          type="button"
          onClick={onSendLatest}
          disabled={sendDisabled}
        >
          {sendState.status === "sending" ? "Sending" : "Send latest"}
        </button>
        <button className="text-button" type="button" onClick={onToggleSticky}>
          {workspace.sticky_capture ? "Sticky on" : "Sticky off"}
        </button>
        <button className="text-button" type="button" onClick={onActivate} disabled={isActive}>
          {isActive ? "Active" : "Set active"}
        </button>
      </div>
    </div>
  );
}

function workspaceStatusText(workspace: WorkspaceSummary, sendState: SendState) {
  if (sendState.status === "sent" || sendState.status === "error") {
    return sendState.message;
  }

  return workspaceAccessText(workspace);
}

function workspaceAccessText(workspace: WorkspaceSummary) {
  const access = workspace.agent_access ? "Agent-readable" : "Human-only";
  if (workspace.hosted_share_enabled) {
    return `${access} / hosted sticky`;
  }

  return access;
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
          <PreviewKindBadge kind={classifyPayload(blip)} />
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

      <SafePayloadPreview blip={blip} />

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

type PayloadKind = "text" | "image" | "file_list" | "rich_text" | "unknown";

type PayloadLike = {
  content_type?: string;
  content?: string;
  is_redacted?: boolean;
  tags?: string[];
  payloads?: PayloadSummary[];
};

function SafePayloadPreview({ blip }: { blip: BlipDetail }) {
  const payload = primaryPayloadSummary(blip);
  const kind = classifyPayload(blip);
  const previewText = payload?.preview_text ?? blip.content;
  const { text: safeContent, truncated: contentWasTruncated } = boundedText(previewText);

  if (blip.is_redacted) {
    return (
      <PayloadPlaceholder
        detail="The detail payload is redacted. Content is intentionally unavailable in this view."
        kind={kind}
        title="Redacted payload"
      />
    );
  }

  if (payload?.preview_state === "missing_blob") {
    return (
      <PayloadPlaceholder
        detail={safeContent || "The payload metadata is available, but the backing blob is missing."}
        kind={kind}
        title="Missing payload blob"
      />
    );
  }

  if (payload?.preview_state === "unavailable") {
    return (
      <PayloadPlaceholder
        detail={safeContent || "A safe preview could not be generated for this payload."}
        kind={kind}
        title="Preview unavailable"
      />
    );
  }

  if (kind === "image") {
    if (payload?.preview_state === "available") {
      return <ImagePayloadPreview payload={payload} summary={safeContent} />;
    }

    return (
      <PayloadPlaceholder
        detail={safeContent || "Image metadata is unavailable."}
        kind={kind}
        title="Image preview unavailable"
      />
    );
  }

  if (kind === "file_list") {
    return (
      <PayloadPlaceholder
        detail={safeContent || "File-list metadata is unavailable."}
        kind={kind}
        title="File list captured"
      />
    );
  }

  if (kind === "unknown") {
    return (
      <PayloadPlaceholder
        detail={safeContent || "No readable metadata was provided for this payload."}
        kind={kind}
        title="Unknown payload"
      />
    );
  }

  if (!safeContent) {
    return (
      <PayloadPlaceholder
        detail="No displayable text was provided for this payload."
        kind={kind}
        title="Missing preview text"
      />
    );
  }

  return (
    <div className="safe-preview">
      {kind === "rich_text" || payload?.preview_state === "text_fallback" ? (
        <div className="safe-preview-note">
          <Code2 aria-hidden="true" size={16} />
          <span>Rich payload shown as escaped plain text</span>
        </div>
      ) : null}
      <pre className="detail-content">{safeContent}</pre>
      {contentWasTruncated ? (
        <p className="preview-footnote">Preview truncated at {DETAIL_TEXT_LIMIT} characters.</p>
      ) : null}
    </div>
  );
}

function ImagePayloadPreview({ payload, summary }: { payload: PayloadSummary; summary: string }) {
  const [state, setState] = React.useState<
    | { status: "loading" }
    | { status: "ready"; objectUrl: string; blob: Blob; mimeType: string | null }
    | { status: "error"; message: string }
  >({ status: "loading" });
  const [copyState, setCopyState] = React.useState<"idle" | "copying" | "copied" | "error">(
    "idle"
  );

  React.useEffect(() => {
    let cancelled = false;
    let objectUrl: string | null = null;

    setState({ status: "loading" });
    getDisplayImagePayload(payload.id)
      .then((response) => {
        if (cancelled) {
          return;
        }

        const blob = createPayloadBlob(response);
        objectUrl = URL.createObjectURL(blob);
        setState({
          status: "ready",
          objectUrl,
          blob,
          mimeType: response.mime_type
        });
      })
      .catch((error: unknown) => {
        if (cancelled) {
          return;
        }

        const message =
          error instanceof Error ? error.message : "Unable to load image preview";
        setState({ status: "error", message });
      });

    return () => {
      cancelled = true;
      if (objectUrl) {
        URL.revokeObjectURL(objectUrl);
      }
    };
  }, [payload.id]);

  if (state.status === "loading") {
    return (
      <PayloadPlaceholder detail="Loading image preview." kind="image" title="Image preview" />
    );
  }

  if (state.status === "error") {
    return (
      <PayloadPlaceholder
        detail={state.message}
        kind="image"
        title="Image preview unavailable"
      />
    );
  }

  const copyImage = async () => {
    if (state.status !== "ready") {
      return;
    }

    setCopyState("copying");
    try {
      await writeImageBlobToClipboard(state.blob, state.mimeType);
      setCopyState("copied");
      window.setTimeout(() => setCopyState("idle"), 1400);
    } catch {
      setCopyState("error");
      window.setTimeout(() => setCopyState("idle"), 2200);
    }
  };

  const interceptCopy = (event: React.ClipboardEvent<HTMLDivElement>) => {
    event.preventDefault();
    void copyImage();
  };

  return (
    <div
      className="image-preview-panel"
      onCopy={interceptCopy}
      tabIndex={0}
      aria-label="Image payload preview"
    >
      <div className="image-preview-toolbar">
        <button
          className="secondary-button"
          type="button"
          onClick={() => void copyImage()}
          disabled={copyState === "copying"}
        >
          <ClipboardCopy aria-hidden="true" size={16} />
          {copyState === "copied"
            ? "Copied"
            : copyState === "error"
              ? "Copy failed"
              : copyState === "copying"
                ? "Copying"
                : "Copy image"}
        </button>
      </div>
      <img
        alt={summary || "Clipboard image preview"}
        src={state.objectUrl}
      />
      <p>{summary || state.mimeType || payload.mime_type || "Image preview"}</p>
    </div>
  );
}

function createPayloadBlob(response: PayloadBytesResponse) {
  const bytes = Uint8Array.from(response.bytes);
  return new Blob([bytes], {
    type: response.mime_type ?? "application/octet-stream"
  });
}

async function writeImageBlobToClipboard(blob: Blob, mimeType: string | null) {
  const clipboardItem = globalThis.ClipboardItem;
  if (!navigator.clipboard?.write || !clipboardItem) {
    throw new Error("image clipboard write is unavailable");
  }

  const type = mimeType && mimeType.startsWith("image/") ? mimeType : blob.type || "image/png";
  const clipboardBlob = blob.type === type ? blob : blob.slice(0, blob.size, type);
  await navigator.clipboard.write([new clipboardItem({ [type]: clipboardBlob })]);
}

async function getDisplayImagePayload(payloadId: string) {
  try {
    return await exportPayload(payloadId);
  } catch {
    return getPayloadPreview(payloadId);
  }
}

function PayloadPlaceholder({
  detail,
  kind,
  title
}: {
  detail: string;
  kind: PayloadKind;
  title: string;
}) {
  const Icon = payloadIcon(kind);

  return (
    <div className={`payload-placeholder ${kind}`}>
      <Icon aria-hidden="true" size={24} />
      <div>
        <h3>{title}</h3>
        <p>{detail}</p>
      </div>
    </div>
  );
}

function PreviewKindBadge({ kind }: { kind: PayloadKind }) {
  const Icon = payloadIcon(kind);

  return (
    <span className={`preview-kind ${kind}`} title={payloadKindLabel(kind)}>
      <Icon aria-hidden="true" size={14} />
      <span>{payloadKindLabel(kind)}</span>
    </span>
  );
}

function classifyPayload(payload: PayloadLike): PayloadKind {
  const primaryPayload = primaryPayloadSummary(payload);
  if (primaryPayload) {
    return normalizePayloadKind(primaryPayload.payload_kind);
  }

  const tags = payload.tags ?? [];
  const normalizedTags = tags.map((tag) => tag.toLowerCase());
  const contentType = payload.content_type?.toLowerCase() ?? "";
  const content = payload.content?.toLowerCase() ?? "";

  if (hasTag(normalizedTags, "clipboard:image") || content.startsWith("image clipboard payload")) {
    return "image";
  }

  if (
    hasTag(normalizedTags, "clipboard:file-list") ||
    hasTag(normalizedTags, "clipboard:file") ||
    content.startsWith("file-list clipboard payload")
  ) {
    return "file_list";
  }

  if (
    hasTag(normalizedTags, "clipboard:html") ||
    hasTag(normalizedTags, "clipboard:rtf") ||
    contentType === "html" ||
    contentType === "rtf" ||
    contentType === "text/html" ||
    contentType === "text/rtf"
  ) {
    return "rich_text";
  }

  if (hasTag(normalizedTags, "clipboard:unknown") || content.startsWith("unknown clipboard payload")) {
    return "unknown";
  }

  return "text";
}

function primaryPayloadSummary(payload: PayloadLike): PayloadSummary | undefined {
  return payload.payloads?.find((candidate) => candidate.payload_kind !== "text") ?? payload.payloads?.[0];
}

function normalizePayloadKind(payloadKind: string): PayloadKind {
  switch (payloadKind) {
    case "image":
      return "image";
    case "file_list":
      return "file_list";
    case "html":
    case "rtf":
      return "rich_text";
    case "unknown":
      return "unknown";
    default:
      return "text";
  }
}

function hasTag(tags: string[], expected: string) {
  return tags.some((tag) => tag === expected);
}

function payloadIcon(kind: PayloadKind) {
  switch (kind) {
    case "image":
      return Image;
    case "file_list":
      return FileText;
    case "rich_text":
      return Code2;
    case "unknown":
      return FileQuestion;
    default:
      return FileText;
  }
}

function payloadKindLabel(kind: PayloadKind) {
  switch (kind) {
    case "image":
      return "Image";
    case "file_list":
      return "Files";
    case "rich_text":
      return "Rich";
    case "unknown":
      return "Unknown";
    default:
      return "Text";
  }
}

function boundedText(value: string) {
  const trimmed = value.trim();
  return {
    text: trimmed.slice(0, DETAIL_TEXT_LIMIT),
    truncated: trimmed.length > DETAIL_TEXT_LIMIT
  };
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

function formatAuditEvent(value: string) {
  return formatMetadata(value).replace(/\b\w/g, (letter) => letter.toUpperCase());
}

function formatAuditActor(event: AuditEventSummary) {
  return event.actor_id ? `${event.actor_type}:${event.actor_id}` : event.actor_type;
}

function formatAuditTarget(event: AuditEventSummary) {
  if (event.target_blip_id && event.target_workspace) {
    return `${event.target_blip_id} / ${event.target_workspace}`;
  }

  return event.target_blip_id ?? event.target_workspace ?? "No target";
}

function formatAuditDetails(detailsJson: string) {
  try {
    const parsed = JSON.parse(detailsJson) as unknown;
    return JSON.stringify(parsed);
  } catch {
    return detailsJson;
  }
}

function formatShortcutAction(value: ShortcutRegistration["action"]) {
  return formatMetadata(value);
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
