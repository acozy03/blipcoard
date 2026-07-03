import { StrictMode } from "react";
import * as React from "react";
import { createRoot } from "react-dom/client";
import {
  AlertTriangle,
  ClipboardCopy,
  Download,
  LogOut,
  RefreshCw,
  Save,
  Search,
  Share2,
  Tag,
  UsersRound
} from "lucide-react";
import {
  clearStoredSession,
  getBlip,
  heartbeat,
  joinWorkspace,
  listBlips,
  listEvents,
  listPresence,
  loadStoredSession,
  recordAccess,
  storeSession,
  updateTags,
  type HostedBlipSummary,
  type HostedSession,
  type MemberPresenceSummary
} from "./api";
import "./styles.css";

const POLL_INTERVAL_MS = 2000;
const PRESENCE_INTERVAL_MS = 10000;

type LoadState =
  | { status: "idle" }
  | { status: "loading" }
  | { status: "ready"; blips: HostedBlipSummary[] }
  | { status: "error"; message: string };

type DetailState =
  | { status: "idle" }
  | { status: "loading"; blipId: string }
  | { status: "ready"; blip: HostedBlipSummary }
  | { status: "error"; message: string };

function App() {
  const [session, setSession] = React.useState<HostedSession | null>(() => loadStoredSession());
  const [loadState, setLoadState] = React.useState<LoadState>({ status: "idle" });
  const [detailState, setDetailState] = React.useState<DetailState>({ status: "idle" });
  const [selectedBlipId, setSelectedBlipId] = React.useState<string | null>(null);
  const [presence, setPresence] = React.useState<MemberPresenceSummary[]>([]);
  const [notice, setNotice] = React.useState<string | null>(null);
  const sessionRef = React.useRef(session);

  React.useEffect(() => {
    sessionRef.current = session;
  }, [session]);

  const loadDetail = React.useCallback(async (blipId: string) => {
    const active = sessionRef.current;
    if (!active) {
      return;
    }

    setSelectedBlipId(blipId);
    setDetailState({ status: "loading", blipId });
    try {
      const blip = await getBlip(active, blipId);
      setDetailState({ status: "ready", blip });
    } catch (error) {
      const message = error instanceof Error ? error.message : "Unable to load blip";
      setDetailState({ status: "error", message });
    }
  }, []);

  const loadWorkspace = React.useCallback(
    async (options: { silent?: boolean } = {}) => {
      const active = sessionRef.current;
      if (!active) {
        return;
      }

      if (!options.silent) {
        setLoadState({ status: "loading" });
      }

      try {
        const blips = await listBlips(active);
        setLoadState({ status: "ready", blips });
        setPresence(await listPresence(active));

        const current = selectedBlipId ?? blips[0]?.id ?? null;
        if (current && blips.some((blip) => blip.id === current)) {
          await loadDetail(current);
        } else {
          setSelectedBlipId(null);
          setDetailState({ status: "idle" });
        }
      } catch (error) {
        const message = error instanceof Error ? error.message : "Unable to load workspace";
        setLoadState({ status: "error", message });
      }
    },
    [loadDetail, selectedBlipId]
  );

  React.useEffect(() => {
    if (!session) {
      setLoadState({ status: "idle" });
      setDetailState({ status: "idle" });
      return;
    }

    void loadWorkspace();
  }, [session, loadWorkspace]);

  React.useEffect(() => {
    if (!session) {
      return undefined;
    }

    let stopped = false;
    const tick = async () => {
      const active = sessionRef.current;
      if (!active || stopped) {
        return;
      }

      try {
        const events = await listEvents(active, active.lastSequence);
        if (events.length === 0) {
          return;
        }

        const lastSequence = Math.max(...events.map((event) => event.sequence), active.lastSequence);
        const nextSession = { ...active, lastSequence };
        setSession(nextSession);
        storeSession(nextSession);

        if (
          events.some((event) =>
            ["blip_published", "blip_tags_updated"].includes(event.event_type)
          )
        ) {
          await loadWorkspace({ silent: true });
        }
      } catch {
        return;
      }
    };

    const interval = window.setInterval(() => void tick(), POLL_INTERVAL_MS);
    return () => {
      stopped = true;
      window.clearInterval(interval);
    };
  }, [session, loadWorkspace]);

  React.useEffect(() => {
    if (!session) {
      return undefined;
    }

    const tick = async () => {
      const active = sessionRef.current;
      if (!active) {
        return;
      }

      try {
        setPresence(await heartbeat(active));
      } catch {
        return;
      }
    };

    void tick();
    const interval = window.setInterval(() => void tick(), PRESENCE_INTERVAL_MS);
    return () => window.clearInterval(interval);
  }, [session]);

  const onJoined = React.useCallback((joined: HostedSession) => {
    storeSession(joined);
    setSession(joined);
    setNotice("Joined hosted workspace");
  }, []);

  const signOut = React.useCallback(() => {
    clearStoredSession();
    setSession(null);
    setPresence([]);
    setSelectedBlipId(null);
    setNotice(null);
  }, []);

  const canEditTags = session?.member.role === "owner" || session?.member.role === "editor";

  return (
    <main className="app-shell">
      <header className="topbar">
        <div className="brand-lockup">
          <Share2 size={28} aria-hidden="true" />
          <div>
            <h1>Hosted workspace</h1>
            <p>{session ? session.workspace.name : "Join by invite code"}</p>
          </div>
        </div>
        {session ? (
          <div className="topbar-actions">
            <span className="status-pill">{session.member.role}</span>
            <button className="icon-button" type="button" onClick={() => void loadWorkspace()}>
              <RefreshCw size={18} aria-hidden="true" />
              <span className="sr-only">Refresh</span>
            </button>
            <button className="icon-button" type="button" onClick={signOut}>
              <LogOut size={18} aria-hidden="true" />
              <span className="sr-only">Leave workspace</span>
            </button>
          </div>
        ) : null}
      </header>

      {!session ? (
        <JoinPanel onJoined={onJoined} />
      ) : (
        <section className="workspace-grid">
          <aside className="side-panel">
            <PresencePanel members={presence} currentMemberId={session.member.id} />
          </aside>

          <section className="blip-list-panel">
            <div className="panel-heading">
              <div>
                <h2>Blips</h2>
                <p>{loadState.status === "ready" ? `${loadState.blips.length} shared` : "Loading"}</p>
              </div>
              <Search size={18} aria-hidden="true" />
            </div>
            <BlipList
              loadState={loadState}
              selectedBlipId={selectedBlipId}
              onSelect={(blipId) => void loadDetail(blipId)}
            />
          </section>

          <section className="detail-panel">
            <DetailPanel
              detailState={detailState}
              canEditTags={canEditTags}
              session={session}
              onChanged={(blip) => {
                setDetailState({ status: "ready", blip });
                void loadWorkspace({ silent: true });
              }}
              onNotice={setNotice}
            />
          </section>
        </section>
      )}

      {notice ? <div className="toast">{notice}</div> : null}
    </main>
  );
}

function JoinPanel({ onJoined }: { onJoined: (session: HostedSession) => void }) {
  const [serviceUrl, setServiceUrl] = React.useState("http://127.0.0.1:8732");
  const [code, setCode] = React.useState("");
  const [displayName, setDisplayName] = React.useState("");
  const [error, setError] = React.useState<string | null>(null);
  const [joining, setJoining] = React.useState(false);

  async function submit(event: React.FormEvent) {
    event.preventDefault();
    setError(null);
    setJoining(true);

    try {
      const joined = await joinWorkspace(serviceUrl, code, displayName || "Web member");
      onJoined({
        serviceUrl,
        workspace: joined.workspace,
        member: joined.member,
        session: joined.session,
        lastSequence: joined.workspace.last_sequence
      });
    } catch (error) {
      setError(error instanceof Error ? error.message : "Unable to join workspace");
    } finally {
      setJoining(false);
    }
  }

  return (
    <section className="join-shell">
      <form className="join-panel" onSubmit={(event) => void submit(event)}>
        <div className="panel-heading">
          <div>
            <h2>Join workspace</h2>
            <p>Use the relay URL and invite code from the workspace owner.</p>
          </div>
          <Share2 size={20} aria-hidden="true" />
        </div>
        <label>
          Relay URL
          <input value={serviceUrl} onChange={(event) => setServiceUrl(event.target.value)} />
        </label>
        <label>
          Code
          <input
            value={code}
            onChange={(event) => setCode(event.target.value)}
            placeholder="BLIP-0000-0000-0000"
          />
        </label>
        <label>
          Name
          <input value={displayName} onChange={(event) => setDisplayName(event.target.value)} />
        </label>
        {error ? <InlineError message={error} /> : null}
        <button className="primary-button" type="submit" disabled={joining || !code.trim()}>
          {joining ? "Joining" : "Join"}
        </button>
      </form>
    </section>
  );
}

function PresencePanel({
  members,
  currentMemberId
}: {
  members: MemberPresenceSummary[];
  currentMemberId: string;
}) {
  return (
    <div className="presence-panel">
      <div className="panel-heading">
        <div>
          <h2>Presence</h2>
          <p>{members.length} active</p>
        </div>
        <UsersRound size={18} aria-hidden="true" />
      </div>
      <div className="presence-list">
        {members.map((member) => (
          <div className="presence-row" key={member.member_id}>
            <div>
              <strong>{member.display_name}</strong>
              <span>{member.member_id === currentMemberId ? "you" : member.role}</span>
            </div>
            <time>{relativeTime(member.last_seen_at)}</time>
          </div>
        ))}
        {members.length === 0 ? <p className="muted">No active members yet.</p> : null}
      </div>
    </div>
  );
}

function BlipList({
  loadState,
  selectedBlipId,
  onSelect
}: {
  loadState: LoadState;
  selectedBlipId: string | null;
  onSelect: (blipId: string) => void;
}) {
  if (loadState.status === "loading" || loadState.status === "idle") {
    return <div className="empty-state">Loading blips</div>;
  }
  if (loadState.status === "error") {
    return <InlineError message={loadState.message} />;
  }
  if (loadState.blips.length === 0) {
    return <div className="empty-state">No shared blips yet</div>;
  }

  return (
    <div className="blip-list">
      {loadState.blips.map((blip) => (
        <button
          className={blip.id === selectedBlipId ? "blip-row selected" : "blip-row"}
          key={blip.id}
          type="button"
          onClick={() => onSelect(blip.id)}
        >
          <span>{blip.preview || blip.content_type}</span>
          <small>
            {blip.content_type} / {formatBytes(blip.size_bytes)}
          </small>
        </button>
      ))}
    </div>
  );
}

function DetailPanel({
  detailState,
  canEditTags,
  session,
  onChanged,
  onNotice
}: {
  detailState: DetailState;
  canEditTags: boolean;
  session: HostedSession;
  onChanged: (blip: HostedBlipSummary) => void;
  onNotice: (message: string) => void;
}) {
  const [tagText, setTagText] = React.useState("");
  const [savingTags, setSavingTags] = React.useState(false);

  React.useEffect(() => {
    if (detailState.status === "ready") {
      setTagText(detailState.blip.tags.join(", "));
    }
  }, [detailState]);

  if (detailState.status === "idle") {
    return <div className="empty-state">Select a blip</div>;
  }
  if (detailState.status === "loading") {
    return <div className="empty-state">Loading detail</div>;
  }
  if (detailState.status === "error") {
    return <InlineError message={detailState.message} />;
  }

  const blip = detailState.blip;
  const blocked = blip.is_redacted;

  async function copyBlip() {
    if (blocked) {
      return;
    }
    await window.navigator.clipboard.writeText(blip.content);
    await recordAccess(session, blip.id, "copy");
    onNotice("Copied blip content");
  }

  async function exportBlip() {
    if (blocked) {
      return;
    }
    const blob = new Blob([blip.content], { type: "text/plain;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = `${blip.local_blip_id || blip.id}.txt`;
    link.click();
    URL.revokeObjectURL(url);
    await recordAccess(session, blip.id, "export");
    onNotice("Exported blip content");
  }

  async function saveTags() {
    setSavingTags(true);
    try {
      const tags = tagText
        .split(",")
        .map((tag) => tag.trim())
        .filter(Boolean);
      onChanged(await updateTags(session, blip.id, tags));
      onNotice("Updated tags");
    } finally {
      setSavingTags(false);
    }
  }

  return (
    <article className="detail-content">
      <div className="detail-header">
        <div>
          <h2>{blip.preview || blip.local_blip_id}</h2>
          <p>
            {blip.content_type} / {formatBytes(blip.size_bytes)} / sequence {blip.sequence}
          </p>
        </div>
        <div className="detail-actions">
          <button className="icon-button" type="button" onClick={() => void copyBlip()} disabled={blocked}>
            <ClipboardCopy size={18} aria-hidden="true" />
            <span className="sr-only">Copy blip</span>
          </button>
          <button className="icon-button" type="button" onClick={() => void exportBlip()} disabled={blocked}>
            <Download size={18} aria-hidden="true" />
            <span className="sr-only">Export blip</span>
          </button>
        </div>
      </div>

      <section className="payload-box">
        {blocked ? "Payload is redacted for this workspace." : blip.content}
      </section>

      <section className="tag-editor">
        <label>
          <span>
            <Tag size={16} aria-hidden="true" />
            Tags
          </span>
          <input
            value={tagText}
            onChange={(event) => setTagText(event.target.value)}
            disabled={!canEditTags}
            placeholder="bug, api, handoff"
          />
        </label>
        {canEditTags ? (
          <button className="secondary-button" type="button" onClick={() => void saveTags()} disabled={savingTags}>
            <Save size={16} aria-hidden="true" />
            {savingTags ? "Saving" : "Save"}
          </button>
        ) : null}
      </section>
    </article>
  );
}

function InlineError({ message }: { message: string }) {
  return (
    <div className="inline-error">
      <AlertTriangle size={18} aria-hidden="true" />
      <span>{message}</span>
    </div>
  );
}

function formatBytes(size: number) {
  if (size < 1024) {
    return `${size} B`;
  }
  return `${(size / 1024).toFixed(1)} KB`;
}

function relativeTime(value: string) {
  const seconds = Math.max(0, Math.round((Date.now() - new Date(value).getTime()) / 1000));
  if (seconds < 60) {
    return `${seconds}s`;
  }
  return `${Math.round(seconds / 60)}m`;
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>
);
