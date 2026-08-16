import { StrictMode } from "react";
import * as React from "react";
import { createRoot } from "react-dom/client";
import {
  AlertTriangle,
  CheckCircle2,
  ChevronLeft,
  ChevronRight,
  Code2,
  Copy,
  FileQuestion,
  FileText,
  Image,
  Inbox,
  Keyboard,
  Loader2,
  Plus,
  RefreshCw,
  RotateCcw,
  Share2,
  Shield,
  UploadCloud,
  UserRound
} from "lucide-react";
import {
  activateWorkspace,
  createWorkspace,
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
  recopyBlip,
  routeLatestInboxBlip,
  setAgentAccess,
  setStickyCapture,
  type AuditEventSummary,
  type BlipDetail,
  type BlipListFilters,
  type BlipSummary,
  type BlipTypeFilter,
  type HostedStatusResponse,
  type PayloadSummary,
  type PayloadBytesResponse,
  type ShortcutRegistration,
  type WorkspaceSummary
} from "./daemon";
import "./styles.css";

const DETAIL_TEXT_LIMIT = 12000;
const AUTO_REFRESH_INTERVAL_MS = 1500;
const MIN_REFRESH_FEEDBACK_MS = 600;
const BLIPS_PER_PAGE = 8;
const ALL_WORKSPACES_FILTER = "";
const ALL_BLIP_TYPES: BlipTypeFilter[] = ["text", "image", "file_list", "rich_text", "unknown"];

type BlipDatePreset = "any" | "today" | "last_7_days" | "last_30_days" | "custom";

type BlipFilterState = {
  workspace: string;
  datePreset: BlipDatePreset;
  customFrom: string;
  customTo: string;
  blipTypes: BlipTypeFilter[];
};

const DEFAULT_BLIP_FILTER_STATE: BlipFilterState = {
  workspace: ALL_WORKSPACES_FILTER,
  datePreset: "any",
  customFrom: "",
  customTo: "",
  blipTypes: ALL_BLIP_TYPES
};

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
  | {
      status: "ready";
      workspace: string;
      blips: BlipSummary[];
      page: number;
      totalPages: number;
      pendingPage: number | null;
      pageError: string | null;
    }
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
  | { status: "sent"; workspace: string; message: string }
  | { status: "error"; workspace: string; message: string };

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

type AgentAccessState =
  | { status: "idle" }
  | { status: "confirming"; workspace: string }
  | { status: "updating"; workspace: string; enabled: boolean }
  | { status: "done"; workspace: string; message: string }
  | { status: "error"; workspace: string; message: string };

type RefreshState =
  | { status: "idle" }
  | { status: "refreshing" }
  | { status: "error"; message: string };

type WorkspaceCreateState =
  | { status: "idle" }
  | { status: "creating" }
  | { status: "error"; message: string };

function App() {
  const [workspaceState, setWorkspaceState] = React.useState<WorkspaceState>({
    status: "loading"
  });
  const [selectedWorkspace, setSelectedWorkspace] = React.useState<string | null>(null);
  const selectedWorkspaceRef = React.useRef<string | null>(null);
  const blipFiltersRef = React.useRef<BlipFilterState>(DEFAULT_BLIP_FILTER_STATE);
  const selectedBlipIdRef = React.useRef<string | null>(null);
  const blipRequestRef = React.useRef(0);
  const blipRequestInFlightRef = React.useRef(false);
  const blipPageRef = React.useRef(0);
  const detailRequestRef = React.useRef(0);
  const agentAccessRequestRef = React.useRef(0);
  const hasLoadedWorkspacesRef = React.useRef(false);
  const [blipState, setBlipState] = React.useState<BlipState>({ status: "idle" });
  const [blipFilters, setBlipFilters] = React.useState<BlipFilterState>(
    DEFAULT_BLIP_FILTER_STATE
  );
  const [selectedBlipId, setSelectedBlipId] = React.useState<string | null>(null);
  const [detailState, setDetailState] = React.useState<DetailState>({ status: "idle" });
  const [auditState, setAuditState] = React.useState<AuditState>({ status: "loading" });
  const [shortcutState, setShortcutState] = React.useState<ShortcutState>({ status: "loading" });
  const [sendState, setSendState] = React.useState<SendState>({ status: "idle" });
  const [hostedState, setHostedState] = React.useState<HostedState>({ status: "loading" });
  const [hostedActionState, setHostedActionState] = React.useState<HostedActionState>({
    status: "idle"
  });
  const [agentAccessState, setAgentAccessState] = React.useState<AgentAccessState>({
    status: "idle"
  });
  const [refreshState, setRefreshState] = React.useState<RefreshState>({ status: "idle" });
  const [workspaceCreateOpen, setWorkspaceCreateOpen] = React.useState(false);
  const [workspaceCreateState, setWorkspaceCreateState] =
    React.useState<WorkspaceCreateState>({ status: "idle" });
  const [newWorkspaceName, setNewWorkspaceName] = React.useState("");
  const [newWorkspaceAgentAccess, setNewWorkspaceAgentAccess] = React.useState(false);
  const [joinServiceUrl, setJoinServiceUrl] = React.useState("http://127.0.0.1:8732");
  const [joinCode, setJoinCode] = React.useState("");
  const [joinDisplayName, setJoinDisplayName] = React.useState("");

  React.useEffect(() => {
    selectedWorkspaceRef.current = selectedWorkspace;
  }, [selectedWorkspace]);

  React.useEffect(() => {
    selectedBlipIdRef.current = selectedBlipId;
  }, [selectedBlipId]);

  const loadDetail = React.useCallback((blipId: string, options: { silent?: boolean } = {}) => {
    const requestId = detailRequestRef.current + 1;
    detailRequestRef.current = requestId;
    selectedBlipIdRef.current = blipId;
    setSelectedBlipId(blipId);
    if (!options.silent) {
      setDetailState({ status: "loading", blipId });
    }
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

        const message = errorMessage(error, "Unable to load blip");
        setDetailState({ status: "error", blipId, message });
      });
  }, []);

  const loadBlips = React.useCallback(
    function loadBlips(
      workspace: string,
      options: {
        silent?: boolean;
        page?: number;
        pagination?: boolean;
        preserve?: boolean;
        previousPage?: number;
        filters?: BlipFilterState;
      } = {}
    ): Promise<void> {
      const filterState = options.filters ?? blipFiltersRef.current;
      if (!blipFilterIsValid(filterState)) {
        return Promise.resolve();
      }
      const requestId = blipRequestRef.current + 1;
      const page = options.page ?? blipPageRef.current;
      const requestWorkspace =
        filterState.workspace === ALL_WORKSPACES_FILTER ? workspace : filterState.workspace;
      blipRequestRef.current = requestId;
      blipRequestInFlightRef.current = true;
      if (!options.silent && !options.preserve) {
        setBlipState({ status: "loading", workspace });
      }
      return listWorkspaceBlips(
        requestWorkspace,
        BLIPS_PER_PAGE,
        page * BLIPS_PER_PAGE,
        daemonBlipListFilters(filterState)
      )
        .then((response) => {
          if (selectedWorkspaceRef.current !== workspace) {
            return;
          }

          if (blipRequestRef.current === requestId) {
            const blips = response.blips.slice(0, BLIPS_PER_PAGE);
            const totalPages = Math.max(1, Math.ceil(response.total / BLIPS_PER_PAGE));
            if (page >= totalPages && page > 0) {
              const clampedPage = totalPages - 1;
              blipPageRef.current = clampedPage;
              return loadBlips(workspace, {
                page: clampedPage,
                silent: true,
                pagination: true,
                previousPage: page,
                filters: filterState
              });
            }
            blipPageRef.current = page;
            setBlipState({
              status: "ready",
              workspace: response.workspace,
              blips,
              page,
              totalPages,
              pendingPage: null,
              pageError: null
            });
            const currentSelection = selectedBlipIdRef.current;
            const selectedStillExists =
              currentSelection && blips.some((blip) => blip.id === currentSelection);

            if (selectedStillExists) {
              return;
            }

            const firstBlip = blips[0] ?? null;

            if (firstBlip) {
              loadDetail(firstBlip.id, { silent: options.silent || options.preserve });
            } else {
              detailRequestRef.current += 1;
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

          if (options.pagination) {
            const message = errorMessage(error, "Unable to load blip page");
            blipPageRef.current = options.previousPage ?? 0;
            setBlipState((current) =>
              current.status === "ready"
                ? { ...current, pendingPage: null, pageError: message }
                : current
            );
            return;
          }

          if (options.preserve) {
            const message = errorMessage(error, "Unable to apply blip filters");
            setBlipState((current) =>
              current.status === "ready"
                ? { ...current, pendingPage: null, pageError: message }
                : { status: "error", workspace, message }
            );
            return;
          }

          if (options.silent) {
            return;
          }

          const message = errorMessage(error, "Unable to load workspace");
          setBlipState({ status: "error", workspace, message });
          detailRequestRef.current += 1;
          selectedBlipIdRef.current = null;
          setSelectedBlipId(null);
          setDetailState({ status: "idle" });
        })
        .finally(() => {
          if (blipRequestRef.current === requestId) {
            blipRequestInFlightRef.current = false;
          }
        });
    },
    [loadDetail]
  );

  const loadAudit = React.useCallback(() => {
    listAuditEvents()
      .then((response) => setAuditState({ status: "ready", events: response.events }))
      .catch((error: unknown) => {
        const message = errorMessage(error, "Unable to load audit events");
        setAuditState({ status: "error", message });
      });
  }, []);

  const loadShortcuts = React.useCallback(() => {
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
        const message = errorMessage(error, "Unable to load hosted status");
        setHostedState({ status: "error", message });
      });
  }, []);

  const refresh = React.useCallback(() => {
    const backgroundRefresh = hasLoadedWorkspacesRef.current;
    const startedAt = window.performance.now();
    const finishRefresh = (state: RefreshState) => {
      const remaining = Math.max(
        0,
        MIN_REFRESH_FEEDBACK_MS - (window.performance.now() - startedAt)
      );
      window.setTimeout(() => setRefreshState(state), remaining);
    };
    setRefreshState({ status: "refreshing" });
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
        hasLoadedWorkspacesRef.current = true;
        setWorkspaceState({ status: "ready", workspaces, activeWorkspace });
        setSelectedWorkspace(nextSelection);
        finishRefresh({ status: "idle" });

        if (nextSelection) {
          loadBlips(nextSelection, { silent: backgroundRefresh });
        } else {
          setBlipState({ status: "idle" });
          selectedBlipIdRef.current = null;
          setSelectedBlipId(null);
          setDetailState({ status: "idle" });
        }
      })
      .catch((error: unknown) => {
        const message = errorMessage(error, "Unable to load workspaces");
        finishRefresh({ status: "error", message });
        if (!hasLoadedWorkspacesRef.current) {
          setWorkspaceState({ status: "error", message });
          setBlipState({ status: "idle" });
          selectedBlipIdRef.current = null;
          setSelectedBlipId(null);
          setDetailState({ status: "idle" });
        }
      });
  }, [loadAudit, loadBlips, loadHosted, loadShortcuts]);

  React.useEffect(() => {
    refresh();
  }, [refresh]);

  React.useEffect(() => {
    if (!selectedWorkspace) {
      return;
    }

    let timeoutId: number;
    let cancelled = false;
    const scheduleRefresh = () => {
      if (cancelled) {
        return;
      }
      timeoutId = window.setTimeout(() => {
        if (cancelled) {
          return;
        }
        if (blipRequestInFlightRef.current) {
          scheduleRefresh();
          return;
        }
        const workspace = selectedWorkspaceRef.current;
        if (workspace) {
          void loadBlips(workspace, { silent: true }).finally(scheduleRefresh);
        } else {
          scheduleRefresh();
        }
      }, AUTO_REFRESH_INTERVAL_MS);
    };
    scheduleRefresh();

    return () => {
      cancelled = true;
      window.clearTimeout(timeoutId);
    };
  }, [loadBlips, selectedWorkspace]);

  const selectWorkspace = (workspace: string) => {
    agentAccessRequestRef.current += 1;
    blipPageRef.current = 0;
    selectedWorkspaceRef.current = workspace;
    setSelectedWorkspace(workspace);
    detailRequestRef.current += 1;
    selectedBlipIdRef.current = null;
    setSelectedBlipId(null);
    setDetailState({ status: "idle" });
    setAgentAccessState({ status: "idle" });
    setSendState({ status: "idle" });
    loadBlips(workspace, { page: 0 });
  };

  const selectBlipPage = (page: number) => {
    if (!selectedWorkspace || page < 0) {
      return;
    }
    const previousPage = blipState.status === "ready" ? blipState.page : 0;
    blipPageRef.current = page;
    setBlipState((current) =>
      current.status === "ready"
        ? { ...current, pendingPage: page, pageError: null }
        : current
    );
    loadBlips(selectedWorkspace, {
      page,
      silent: true,
      pagination: true,
      previousPage
    });
  };

  const updateBlipFilters = (filters: BlipFilterState) => {
    if (!selectedWorkspace) {
      return;
    }
    blipFiltersRef.current = filters;
    setBlipFilters(filters);
    blipPageRef.current = 0;
    if (blipFilterIsValid(filters)) {
      setBlipState((current) =>
        current.status === "ready"
          ? { ...current, pendingPage: 0, pageError: null }
          : current
      );
      loadBlips(selectedWorkspace, { page: 0, filters, preserve: true });
    }
  };

  const refreshAfterRecopy = () => {
    if (!selectedWorkspace) {
      return;
    }
    blipPageRef.current = 0;
    loadBlips(selectedWorkspace, { page: 0, silent: true });
  };

  const closeWorkspaceCreate = () => {
    if (workspaceCreateState.status === "creating") {
      return;
    }
    setWorkspaceCreateOpen(false);
    setWorkspaceCreateState({ status: "idle" });
    setNewWorkspaceName("");
    setNewWorkspaceAgentAccess(false);
  };

  const submitWorkspaceCreate = () => {
    const name = newWorkspaceName.trim();
    if (!name || workspaceCreateState.status === "creating") {
      return;
    }

    setWorkspaceCreateState({ status: "creating" });
    createWorkspace({
      name,
      description: null,
      color: null,
      agent_access: newWorkspaceAgentAccess
    })
      .then((workspace) => {
        setWorkspaceState((current) =>
          current.status === "ready"
            ? {
                ...current,
                workspaces: current.workspaces.some((candidate) => candidate.name === workspace.name)
                  ? current.workspaces
                  : [...current.workspaces, workspace]
              }
            : current
        );
        setWorkspaceCreateOpen(false);
        setWorkspaceCreateState({ status: "idle" });
        setNewWorkspaceName("");
        setNewWorkspaceAgentAccess(false);
        selectWorkspace(workspace.name);
      })
      .catch((error: unknown) => {
        setWorkspaceCreateState({
          status: "error",
          message: errorMessage(error, "Unable to create workspace")
        });
      });
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
        const message = errorMessage(error, "Unable to activate workspace");
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
        const message = errorMessage(error, "Unable to update sticky mode");
        setWorkspaceState({ status: "error", message });
      });
  };

  const updateAgentAccess = (enabled: boolean) => {
    if (!selectedSummary || workspaceState.status !== "ready") {
      return;
    }

    const workspace = selectedSummary.name;
    const requestId = agentAccessRequestRef.current + 1;
    agentAccessRequestRef.current = requestId;
    setAgentAccessState({ status: "updating", workspace, enabled });
    setAgentAccess(workspace, enabled)
      .then((updatedWorkspace) => {
        setWorkspaceState((current) =>
          current.status === "ready"
            ? {
                ...current,
                workspaces: current.workspaces.map((candidate) =>
                  candidate.name === updatedWorkspace.name ? updatedWorkspace : candidate
                )
              }
            : current
        );
        loadAudit();
        if (agentAccessRequestRef.current !== requestId) {
          return;
        }
        setAgentAccessState({
          status: "done",
          workspace,
          message: enabled ? "Agent access enabled" : "Agent access disabled"
        });
      })
      .catch((error: unknown) => {
        if (agentAccessRequestRef.current !== requestId) {
          return;
        }
        setAgentAccessState({
          status: "error",
          workspace,
          message: errorMessage(error, "Unable to update agent access")
        });
      });
  };

  const requestAgentAccessChange = () => {
    if (!selectedSummary) {
      return;
    }

    if (
      agentAccessState.status === "confirming" &&
      agentAccessState.workspace === selectedSummary.name
    ) {
      setAgentAccessState({ status: "idle" });
      return;
    }

    if (selectedSummary.agent_access) {
      updateAgentAccess(false);
    } else {
      setAgentAccessState({ status: "confirming", workspace: selectedSummary.name });
    }
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
          workspace: selectedSummary.name,
          message: `Sent ${routed.id} to ${routed.to_workspace}`
        });
        blipPageRef.current = 0;
        loadBlips(selectedSummary.name, { page: 0 });
        loadAudit();
      })
      .catch((error: unknown) => {
        const message = errorMessage(error, "Unable to send latest blip");
        setSendState({ status: "error", workspace: selectedSummary.name, message });
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
        const message = errorMessage(error, "Unable to join hosted workspace");
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
        const message = errorMessage(error, "Unable to publish blip");
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
        const message = errorMessage(error, "Unable to update sticky share");
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
          <button
            className={refreshState.status === "error" ? "icon-button refresh-error" : "icon-button"}
            type="button"
            onClick={refresh}
            disabled={refreshState.status === "refreshing"}
            aria-busy={refreshState.status === "refreshing"}
            aria-label={
              refreshState.status === "error"
                ? `Refresh failed: ${refreshState.message}`
                : refreshState.status === "refreshing"
                  ? "Refreshing"
                  : "Refresh"
            }
            title={refreshState.status === "error" ? refreshState.message : "Refresh"}
          >
            <RefreshCw
              className={refreshState.status === "refreshing" ? "spin" : undefined}
              aria-hidden="true"
              size={18}
            />
          </button>
        </div>
      </header>

      <section className="workspace-layout">
        <aside className="workspace-sidebar">
          <WorkspaceCreateControl
            agentAccess={newWorkspaceAgentAccess}
            name={newWorkspaceName}
            open={workspaceCreateOpen}
            state={workspaceCreateState}
            onAgentAccessChange={setNewWorkspaceAgentAccess}
            onCancel={closeWorkspaceCreate}
            onNameChange={setNewWorkspaceName}
            onOpen={() => {
              setWorkspaceCreateOpen(true);
              setWorkspaceCreateState({ status: "idle" });
            }}
            onSubmit={submitWorkspaceCreate}
          />
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
          <div className="workspace-overview">
            {selectedSummary ? (
              <WorkspaceHeader
                activeWorkspace={
                  workspaceState.status === "ready" ? workspaceState.activeWorkspace : null
                }
                agentAccessState={agentAccessState}
                workspace={selectedSummary}
                onActivate={setActiveWorkspace}
                onCancelAgentAccess={() => setAgentAccessState({ status: "idle" })}
                onConfirmAgentAccess={() => updateAgentAccess(true)}
                onSendLatest={sendLatestToSelectedWorkspace}
                onToggleAgentAccess={requestAgentAccessChange}
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
                filters={blipFilters}
                selectedBlipId={selectedBlipId}
                state={blipState}
                workspaces={workspaceState.status === "ready" ? workspaceState.workspaces : []}
                onFiltersChange={updateBlipFilters}
                onPageChange={selectBlipPage}
                onSelectBlip={loadDetail}
              />
              <BlipDetailPanel
                activeWorkspace={
                  workspaceState.status === "ready" ? workspaceState.activeWorkspace : null
                }
                onRecopied={refreshAfterRecopy}
                state={detailState}
              />
            </div>
          </div>
          <ShortcutPanel state={shortcutState} />
          <AuditPanel state={auditState} />
        </section>
      </section>
    </main>
  );
}

function WorkspaceCreateControl({
  agentAccess,
  name,
  open,
  state,
  onAgentAccessChange,
  onCancel,
  onNameChange,
  onOpen,
  onSubmit
}: {
  agentAccess: boolean;
  name: string;
  open: boolean;
  state: WorkspaceCreateState;
  onAgentAccessChange: (enabled: boolean) => void;
  onCancel: () => void;
  onNameChange: (name: string) => void;
  onOpen: () => void;
  onSubmit: () => void;
}) {
  if (!open) {
    return (
      <button className="workspace-create-trigger" type="button" onClick={onOpen}>
        <Plus aria-hidden="true" size={16} />
        New workspace
      </button>
    );
  }

  const creating = state.status === "creating";
  return (
    <form
      className="workspace-create-form"
      onSubmit={(event) => {
        event.preventDefault();
        onSubmit();
      }}
    >
      <div className="workspace-create-heading">
        <div>
          <h2>New workspace</h2>
          <p>Create a destination for routed blips.</p>
        </div>
        <button
          className="workspace-create-cancel"
          type="button"
          onClick={onCancel}
          disabled={creating}
        >
          Cancel
        </button>
      </div>
      <label className="workspace-create-name">
        <span>Name</span>
        <input
          autoFocus
          value={name}
          onChange={(event) => onNameChange(event.currentTarget.value)}
          placeholder="release-notes"
          disabled={creating}
        />
      </label>
      <label className="workspace-create-access">
        <input
          type="checkbox"
          checked={agentAccess}
          onChange={(event) => onAgentAccessChange(event.currentTarget.checked)}
          disabled={creating}
        />
        <span>Allow agent access</span>
      </label>
      {state.status === "error" ? (
        <p className="workspace-create-error" role="alert">
          {state.message}
        </p>
      ) : null}
      <button
        className="text-button workspace-create-submit"
        type="submit"
        disabled={creating || name.trim() === ""}
      >
        {creating ? (
          <Loader2 className="spin" aria-hidden="true" size={15} />
        ) : (
          <Plus aria-hidden="true" size={15} />
        )}
        {creating ? "Creating" : "Create workspace"}
      </button>
    </form>
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
            <span className="workspace-badges">
              {isActive ? (
                <span className="active-marker">
                  <CheckCircle2 aria-hidden="true" size={14} />
                  Active
                </span>
              ) : null}
              {workspace.hosted_share_enabled ? (
                <span className="share-marker">
                  <Share2 aria-hidden="true" size={13} />
                  Shared
                </span>
              ) : null}
            </span>
            {workspace.sticky_capture ? (
              <span className="sticky-marker workspace-sticky">Sticky</span>
            ) : null}
          </button>
        );
      })}
    </div>
  );
}

function WorkspaceHeader({
  activeWorkspace,
  agentAccessState,
  workspace,
  onActivate,
  onCancelAgentAccess,
  onConfirmAgentAccess,
  onSendLatest,
  onToggleAgentAccess,
  onToggleSticky,
  sendState
}: {
  activeWorkspace: string | null;
  agentAccessState: AgentAccessState;
  workspace: WorkspaceSummary;
  onActivate: () => void;
  onCancelAgentAccess: () => void;
  onConfirmAgentAccess: () => void;
  onSendLatest: () => void;
  onToggleAgentAccess: () => void;
  onToggleSticky: () => void;
  sendState: SendState;
}) {
  const isActive = workspace.name === activeWorkspace;
  const sendDisabled = workspace.name === "inbox" || sendState.status === "sending";
  const sendStateApplies =
    sendState.status !== "idle" && sendState.workspace === workspace.name;
  const sendBusy = sendStateApplies && sendState.status === "sending";
  const accessStateApplies =
    agentAccessState.status !== "idle" && agentAccessState.workspace === workspace.name;
  const accessBusy = accessStateApplies && agentAccessState.status === "updating";
  const accessBusyLabel =
    accessStateApplies && agentAccessState.status === "updating"
      ? agentAccessState.enabled
        ? "Enabling"
        : "Disabling"
      : null;
  const confirmationTitleId = React.useId();
  const confirmationDescriptionId = React.useId();
  const accessToggleButtonRef = React.useRef<HTMLButtonElement>(null);
  const enableAccessButtonRef = React.useRef<HTMLButtonElement>(null);

  const cancelAgentAccess = () => {
    onCancelAgentAccess();
    window.requestAnimationFrame(() => accessToggleButtonRef.current?.focus());
  };

  React.useEffect(() => {
    if (accessStateApplies && agentAccessState.status === "confirming") {
      enableAccessButtonRef.current?.focus();
    }
  }, [accessStateApplies, agentAccessState.status]);

  return (
    <section className="workspace-header">
      <div className="workspace-header-main">
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
            aria-busy={sendBusy}
            title={
              workspace.name === "inbox"
                ? "Select another workspace to send the latest inbox blip"
                : "Move the latest inbox blip to this workspace"
            }
          >
            {sendBusy ? <Loader2 className="spin" aria-hidden="true" size={15} /> : null}
            {workspace.name === "inbox" ? "Choose destination" : sendBusy ? "Sending" : "Send latest"}
          </button>
          <button
            ref={accessToggleButtonRef}
            className="text-button agent-access-button"
            type="button"
            aria-pressed={workspace.agent_access}
            aria-expanded={accessStateApplies && agentAccessState.status === "confirming"}
            onClick={onToggleAgentAccess}
            disabled={accessBusy}
          >
            <Shield aria-hidden="true" size={15} />
            {accessBusyLabel
              ? accessBusyLabel
              : workspace.agent_access
                ? "Agent access on"
                : "Agent access off"}
          </button>
          <button className="text-button" type="button" onClick={onToggleSticky}>
            {workspace.sticky_capture ? "Sticky on" : "Sticky off"}
          </button>
          <button
            className={isActive ? "text-button active-state" : "text-button"}
            type="button"
            onClick={onActivate}
            disabled={isActive}
          >
            {isActive ? "Active" : "Set active"}
          </button>
        </div>
      </div>
      {accessStateApplies && agentAccessState.status === "confirming" ? (
        <div
          className="agent-access-confirmation"
          role="alertdialog"
          aria-labelledby={confirmationTitleId}
          aria-describedby={confirmationDescriptionId}
          onKeyDown={(event) => {
            if (event.key === "Escape") {
              cancelAgentAccess();
            }
          }}
        >
          <AlertTriangle aria-hidden="true" size={18} />
          <div>
            <h3 id={confirmationTitleId}>Enable agent access?</h3>
            <p id={confirmationDescriptionId}>
              Agents will be able to read blip text in <strong>{workspace.name}</strong>.{" "}
              {workspace.agent_raw_payload_access
                ? "This workspace already allows raw payload access, so agents will also be able to export image and file bytes."
                : "Raw image and file bytes remain disabled unless separately allowed."}
            </p>
          </div>
          <div className="agent-access-confirmation-actions">
            <button className="text-button" type="button" onClick={cancelAgentAccess}>
              Cancel
            </button>
            <button
              ref={enableAccessButtonRef}
              className="text-button primary"
              type="button"
              onClick={onConfirmAgentAccess}
            >
              Enable access
            </button>
          </div>
        </div>
      ) : null}
      {accessStateApplies &&
      (agentAccessState.status === "done" || agentAccessState.status === "error") ? (
        <p
          className={
            agentAccessState.status === "error"
              ? "workspace-action-message error"
              : "workspace-action-message"
          }
          role={agentAccessState.status === "error" ? "alert" : "status"}
        >
          {agentAccessState.message}
        </p>
      ) : null}
      {sendStateApplies && (sendState.status === "sent" || sendState.status === "error") ? (
        <p
          className={
            sendState.status === "error"
              ? "workspace-action-message error"
              : "workspace-action-message"
          }
          role={sendState.status === "error" ? "alert" : "status"}
        >
          {sendState.status === "sent" ? (
            <CheckCircle2 aria-hidden="true" size={14} />
          ) : (
            <AlertTriangle aria-hidden="true" size={14} />
          )}
          {sendState.message}
        </p>
      ) : null}
    </section>
  );
}

function workspaceStatusText(workspace: WorkspaceSummary, sendState: SendState) {
  if (
    (sendState.status === "sent" || sendState.status === "error") &&
    sendState.workspace === workspace.name
  ) {
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

function daemonBlipListFilters(filters: BlipFilterState): BlipListFilters {
  const { createdAtFrom, createdAtBefore } = blipDateBounds(filters);
  return {
    all_workspaces: filters.workspace === ALL_WORKSPACES_FILTER,
    created_at_from: createdAtFrom,
    created_at_before: createdAtBefore,
    blip_types: filters.blipTypes
  };
}

function blipFilterIsValid(filters: BlipFilterState) {
  return !(
    filters.datePreset === "custom" &&
    filters.customFrom !== "" &&
    filters.customTo !== "" &&
    filters.customFrom > filters.customTo
  );
}

function blipDateBounds(filters: BlipFilterState) {
  if (filters.datePreset === "any") {
    return { createdAtFrom: null, createdAtBefore: null };
  }

  if (filters.datePreset === "custom") {
    return {
      createdAtFrom: filters.customFrom ? localDateBoundary(filters.customFrom, 0) : null,
      createdAtBefore: filters.customTo ? localDateBoundary(filters.customTo, 1) : null
    };
  }

  const days = filters.datePreset === "today" ? 1 : filters.datePreset === "last_7_days" ? 7 : 30;
  const now = new Date();
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const from = new Date(today.getFullYear(), today.getMonth(), today.getDate() - (days - 1));
  const before = new Date(today.getFullYear(), today.getMonth(), today.getDate() + 1);
  return { createdAtFrom: from.toISOString(), createdAtBefore: before.toISOString() };
}

function localDateBoundary(value: string, dayOffset: number) {
  const [year, month, day] = value.split("-").map(Number);
  return new Date(year, month - 1, day + dayOffset).toISOString();
}

function errorMessage(error: unknown, fallback: string) {
  if (error instanceof Error) {
    return error.message;
  }
  if (typeof error === "string") {
    return error;
  }

  return fallback;
}

function BlipPanel({
  filters,
  selectedBlipId,
  state,
  workspaces,
  onFiltersChange,
  onPageChange,
  onSelectBlip
}: {
  filters: BlipFilterState;
  selectedBlipId: string | null;
  state: BlipState;
  workspaces: WorkspaceSummary[];
  onFiltersChange: (filters: BlipFilterState) => void;
  onPageChange: (page: number) => void;
  onSelectBlip: (blipId: string) => void;
}) {
  let content: React.ReactNode;
  if (state.status === "idle") {
    content = <EmptyState title="No workspace selected" />;
  } else if (state.status === "loading") {
    content = <LoadingState label="Loading blips" />;
  } else if (state.status === "error") {
    content = <ErrorState message={state.message} />;
  } else if (state.blips.length === 0) {
    content = <EmptyState title="No blips match these filters" />;
  } else {
    const paginationBusy = state.pendingPage !== null;
    const pages = paginationItems(state.page + 1, state.totalPages);
    content = (
      <>
        <BlipList
          blips={state.blips}
          selectedBlipId={selectedBlipId}
          showWorkspace={filters.workspace === ALL_WORKSPACES_FILTER}
          onSelect={onSelectBlip}
        />
        <nav className="blip-pagination" aria-label="Blip pages">
          <button
            className="pagination-control pagination-arrow"
            type="button"
            onClick={() => onPageChange(state.page - 1)}
            disabled={state.page === 0 || paginationBusy}
            aria-label="Previous page"
          >
            <ChevronLeft aria-hidden="true" size={15} />
          </button>
          <div className="pagination-pages">
            {pages.map((page, index) =>
              page === "ellipsis" ? (
                <span className="pagination-ellipsis" aria-hidden="true" key={`ellipsis-${index}`}>
                  ...
                </span>
              ) : (
                <button
                  className={
                    page === state.page + 1 ? "pagination-control active" : "pagination-control"
                  }
                  type="button"
                  key={page}
                  onClick={() => onPageChange(page - 1)}
                  disabled={paginationBusy || page === state.page + 1}
                  aria-current={page === state.page + 1 ? "page" : undefined}
                  aria-label={`Page ${page}`}
                >
                  {page}
                </button>
              )
            )}
          </div>
          <button
            className="pagination-control pagination-arrow"
            type="button"
            onClick={() => onPageChange(state.page + 1)}
            disabled={state.page + 1 >= state.totalPages || paginationBusy}
            aria-label="Next page"
          >
            <ChevronRight aria-hidden="true" size={15} />
          </button>
        </nav>
        {state.pageError ? (
          <p className="blip-pagination-error" role="alert">
            {state.pageError}
          </p>
        ) : null}
      </>
    );
  }

  return (
    <div className="blip-browser">
      <BlipFilterBar
        filters={filters}
        workspaces={workspaces}
        onChange={onFiltersChange}
      />
      {content}
    </div>
  );
}

function BlipFilterBar({
  filters,
  workspaces,
  onChange
}: {
  filters: BlipFilterState;
  workspaces: WorkspaceSummary[];
  onChange: (filters: BlipFilterState) => void;
}) {
  const rangeErrorId = React.useId();
  const customRangeInvalid =
    filters.datePreset === "custom" &&
    filters.customFrom !== "" &&
    filters.customTo !== "" &&
    filters.customFrom > filters.customTo;
  const updateType = (blipType: BlipTypeFilter, checked: boolean) => {
    const blipTypes = checked
      ? ALL_BLIP_TYPES.filter(
          (candidate) => candidate === blipType || filters.blipTypes.includes(candidate)
        )
      : filters.blipTypes.filter((candidate) => candidate !== blipType);
    if (blipTypes.length > 0) {
      onChange({ ...filters, blipTypes });
    }
  };

  return (
    <section className="blip-filters" aria-label="Filter blips">
      <div className="blip-filter-heading">
        <strong>Filter blips</strong>
        <button
          className="blip-filter-reset"
          type="button"
          onClick={() =>
            onChange({ ...DEFAULT_BLIP_FILTER_STATE, blipTypes: [...ALL_BLIP_TYPES] })
          }
        >
          <RotateCcw aria-hidden="true" size={13} />
          Reset
        </button>
      </div>
      <div className="blip-filter-fields">
        <label className="blip-filter-field">
          <span>Workspace</span>
          <select
            value={filters.workspace}
            onChange={(event) => onChange({ ...filters, workspace: event.currentTarget.value })}
          >
            <option value={ALL_WORKSPACES_FILTER}>All workspaces</option>
            {workspaces.map((workspace) => (
              <option value={workspace.name} key={workspace.name}>
                {workspace.name}
              </option>
            ))}
          </select>
        </label>
        <label className="blip-filter-field">
          <span>Date</span>
          <select
            value={filters.datePreset}
            onChange={(event) =>
              onChange({ ...filters, datePreset: event.currentTarget.value as BlipDatePreset })
            }
          >
            <option value="any">Any time</option>
            <option value="today">Today</option>
            <option value="last_7_days">Last 7 days</option>
            <option value="last_30_days">Last 30 days</option>
            <option value="custom">Custom range</option>
          </select>
        </label>
        {filters.datePreset === "custom" ? (
          <div className="blip-filter-dates">
            <label className="blip-filter-field">
              <span>From</span>
              <input
                type="date"
                value={filters.customFrom}
                aria-invalid={customRangeInvalid}
                aria-describedby={customRangeInvalid ? rangeErrorId : undefined}
                onChange={(event) => onChange({ ...filters, customFrom: event.currentTarget.value })}
              />
            </label>
            <label className="blip-filter-field">
              <span>To</span>
              <input
                type="date"
                value={filters.customTo}
                min={filters.customFrom || undefined}
                aria-invalid={customRangeInvalid}
                aria-describedby={customRangeInvalid ? rangeErrorId : undefined}
                onChange={(event) => onChange({ ...filters, customTo: event.currentTarget.value })}
              />
            </label>
          </div>
        ) : null}
      </div>
      <fieldset className="blip-type-filters">
        <legend>Blip type</legend>
        <div>
          {ALL_BLIP_TYPES.map((blipType) => (
            <label className="blip-type-filter" key={blipType}>
              <input
                type="checkbox"
                checked={filters.blipTypes.includes(blipType)}
                disabled={filters.blipTypes.length === 1 && filters.blipTypes[0] === blipType}
                onChange={(event) => updateType(blipType, event.currentTarget.checked)}
              />
              <span>{payloadKindLabel(blipType)}</span>
            </label>
          ))}
        </div>
      </fieldset>
      {customRangeInvalid ? (
        <p className="blip-filter-error" id={rangeErrorId} role="alert">
          The From date must be before the To date.
        </p>
      ) : null}
    </section>
  );
}

function paginationItems(currentPage: number, totalPages: number): Array<number | "ellipsis"> {
  if (totalPages <= 7) {
    return Array.from({ length: totalPages }, (_, index) => index + 1);
  }
  if (currentPage <= 3) {
    return [1, 2, 3, 4, 5, "ellipsis", totalPages];
  }
  if (currentPage >= totalPages - 2) {
    return [
      1,
      "ellipsis",
      totalPages - 4,
      totalPages - 3,
      totalPages - 2,
      totalPages - 1,
      totalPages
    ];
  }
  return [
    1,
    "ellipsis",
    currentPage - 1,
    currentPage,
    currentPage + 1,
    "ellipsis",
    totalPages
  ];
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
  showWorkspace,
  onSelect
}: {
  blips: BlipSummary[];
  selectedBlipId: string | null;
  showWorkspace: boolean;
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
            <p>{showWorkspace && blip.workspace ? `${blip.workspace} / ` : ""}{blip.id}</p>
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
  onRecopied,
  state
}: {
  activeWorkspace: string | null;
  onRecopied: () => void;
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

      <SafePayloadPreview blip={blip} onRecopied={onRecopied} />

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

type PreviewCopyState = "idle" | "copying" | "copied" | "error";

function SafePayloadPreview({ blip, onRecopied }: { blip: BlipDetail; onRecopied: () => void }) {
  const payload = primaryPayloadSummary(blip);
  const kind = classifyPayload(blip);
  const previewText = payload?.preview_text ?? blip.content;
  const { text: safeContent, truncated: contentWasTruncated } = boundedText(previewText);
  const [copyState, setCopyState] = React.useState<PreviewCopyState>("idle");

  React.useEffect(() => setCopyState("idle"), [blip.id]);

  const copyText = async () => {
    if (!blip.content || copyState === "copying") {
      return;
    }
    setCopyState("copying");
    try {
      const fingerprint = await clipboardTextFingerprint(blip.content);
      await recopyBlip(blip.id, fingerprint);
      await navigator.clipboard.writeText(blip.content);
      setCopyState("copied");
      onRecopied();
      window.setTimeout(() => setCopyState("idle"), 1400);
    } catch {
      setCopyState("error");
      window.setTimeout(() => setCopyState("idle"), 2200);
    }
  };

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
      return (
        <ImagePayloadPreview
          blipId={blip.id}
          onRecopied={onRecopied}
          payload={payload}
          summary={safeContent}
        />
      );
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
        copyState={copyState}
        detail={safeContent || "File-list metadata is unavailable."}
        kind={kind}
        onCopy={() => void copyText()}
        title="File list captured"
      />
    );
  }

  if (kind === "unknown") {
    return (
      <PayloadPlaceholder
        copyState={copyState}
        detail={safeContent || "No readable metadata was provided for this payload."}
        kind={kind}
        onCopy={() => void copyText()}
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
    <div
      className={`safe-preview copyable-preview ${copyState}`}
      role="button"
      tabIndex={0}
      onClick={() => void copyText()}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          void copyText();
        }
      }}
      aria-label="Copy this blip to the clipboard"
    >
      {kind === "rich_text" || payload?.preview_state === "text_fallback" ? (
        <div className="safe-preview-note">
          <Code2 aria-hidden="true" size={16} />
          <span>Rich payload shown as escaped plain text</span>
        </div>
      ) : null}
      <pre className="detail-content"><PreviewCopyStatus state={copyState} />{safeContent}</pre>
      {contentWasTruncated ? (
        <p className="preview-footnote">Preview truncated at {DETAIL_TEXT_LIMIT} characters.</p>
      ) : null}
    </div>
  );
}

function ImagePayloadPreview({
  blipId,
  onRecopied,
  payload,
  summary
}: {
  blipId: string;
  onRecopied: () => void;
  payload: PayloadSummary;
  summary: string;
}) {
  const [state, setState] = React.useState<
    | { status: "loading" }
    | { status: "ready"; objectUrl: string; blob: Blob; mimeType: string | null }
    | { status: "error"; message: string }
  >({ status: "loading" });
  const [copyState, setCopyState] = React.useState<PreviewCopyState>("idle");
  const imageRef = React.useRef<HTMLImageElement>(null);

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

        const message = errorMessage(error, "Unable to load image preview");
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
      const image = imageRef.current;
      if (!image?.naturalWidth || !image.naturalHeight) {
        throw new Error("image dimensions are unavailable");
      }
      await recopyBlip(blipId, `image:${image.naturalWidth}x${image.naturalHeight}`);
      await writeImageBlobToClipboard(state.blob, state.mimeType);
      setCopyState("copied");
      onRecopied();
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
      data-copy-state={copyState}
      onCopy={interceptCopy}
      onClick={() => void copyImage()}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          void copyImage();
        }
      }}
      role="button"
      tabIndex={0}
      aria-label="Copy this image blip to the clipboard"
    >
      <PreviewCopyStatus state={copyState} />
      <div className="image-preview-canvas">
        <img
          ref={imageRef}
          alt={summary || "Clipboard image preview"}
          src={state.objectUrl}
        />
      </div>
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
  copyState,
  detail,
  kind,
  onCopy,
  title
}: {
  copyState?: PreviewCopyState;
  detail: string;
  kind: PayloadKind;
  onCopy?: () => void;
  title: string;
}) {
  const Icon = payloadIcon(kind);
  const interactiveProps = onCopy
    ? {
        role: "button",
        tabIndex: 0,
        onClick: onCopy,
        onKeyDown: (event: React.KeyboardEvent<HTMLDivElement>) => {
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            onCopy();
          }
        },
        "aria-label": "Copy this blip to the clipboard"
      }
    : {};

  return (
    <div
      className={`payload-placeholder ${kind}${onCopy ? ` copyable-preview ${copyState ?? "idle"}` : ""}`}
      {...interactiveProps}
    >
      {onCopy ? (
        <PreviewCopyStatus state={copyState ?? "idle"} />
      ) : null}
      <Icon aria-hidden="true" size={24} />
      <div>
        <h3>{title}</h3>
        <p>{detail}</p>
      </div>
    </div>
  );
}

function previewCopyLabel(state: PreviewCopyState) {
  switch (state) {
    case "copying":
      return "Copying";
    case "copied":
      return "Copied";
    case "error":
      return "Copy failed";
    case "idle":
      return "Click to copy";
  }
}

function PreviewCopyStatus({ state }: { state: PreviewCopyState }) {
  const label = previewCopyLabel(state);
  const Icon =
    state === "copying"
      ? Loader2
      : state === "copied"
        ? CheckCircle2
        : state === "error"
          ? AlertTriangle
          : Copy;
  return (
    <span className="preview-copy-status" role="status" aria-label={label} title={label}>
      <Icon className={state === "copying" ? "spin" : undefined} aria-hidden="true" size={15} />
    </span>
  );
}

async function clipboardTextFingerprint(text: string) {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text));
  return `text:${Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("")}`;
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
