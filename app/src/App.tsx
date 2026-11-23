"use client";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { AnimatePresence, motion } from "motion/react";
import type { FolderConflicts, FolderView, Profile, Role, Snapshot, View, Workspace } from "./types";
import { totalConflicts } from "./lib/conflicts";
import { motionTokens } from "./lib/motion-tokens";
import { useSafeMotion } from "./hooks/useSafeMotion";
import { AddFolder } from "./components/AddFolder";
import { IgnoreEditor } from "./components/IgnoreEditor";
import { VersioningEditor } from "./components/VersioningEditor";
import { Preferences, Sidebar, Topbar, VIEW_KEYS } from "./components/Shell";
import { ComputersView, FoldersView, OverviewView, PendingView, type Actions } from "./components/Views";
import { HelpView } from "./components/Guide";
import { SetupView } from "./components/Setup";
import { RoleContext, ShareEditor } from "./components/Team";
import { WorkspaceContext } from "./components/Invite";
import { ToastProvider, useToast } from "./components/Toasts";

const POLL_MS = 3000;
/** Conflict scans walk the folders on disk: the backend watcher emits
 * `conflicts-changed`; this is only a slow fallback. */
const CONFLICT_POLL_MS = 120000;

function Skeleton() {
  return (
    <div className="view" aria-busy="true" aria-label="Conectando ao Syncthing local">
      <div className="sk sk-title" />
      <div className="stats">
        {[0, 1, 2, 3].map((i) => (
          <div key={i} className="stat"><div className="sk sk-line short" /><div className="sk sk-big" /></div>
        ))}
      </div>
      <div className="panel sk-card"><div className="sk sk-line" /><div className="sk sk-line short" /></div>
      <div className="panel sk-card"><div className="sk sk-line" /><div className="sk sk-bar" /><div className="sk sk-line short" /></div>
    </div>
  );
}

function Problem({ title, message }: { title: string; message: string }) {
  return (
    <div className="view">
      <header className="hero">
        <div className="grow">
          <p className="eyebrow">SERVIÇO</p>
          <h1>{title}</h1>
          <p className="muted">{message}</p>
        </div>
      </header>
    </div>
  );
}

function AppShell() {
  const [snap, setSnap] = useState<Snapshot | null>(null);
  const [profile, setProfile] = useState<Profile | null>(null);
  const [view, setView] = useState<View>("overview");
  const [adding, setAdding] = useState(false);
  const [editing, setEditing] = useState<FolderView | null>(null);
  const [versioning, setVersioning] = useState<FolderView | null>(null);
  const [prefs, setPrefs] = useState(false);
  const [sharing, setSharing] = useState<FolderView | null>(null);
  const [ws, setWs] = useState<Workspace>({ role: "personal" });
  const role = ws.role;
  const [conflicts, setConflicts] = useState<FolderConflicts[]>([]);
  const toast = useToast();
  const m = useSafeMotion(motionTokens.distance.sm);

  // Polls, events and post-action refreshes overlap; only the newest
  // request may update state (an older, slower one must not undo it).
  const snapSeq = useRef(0);
  const conflictSeq = useRef(0);

  const refresh = useCallback(async () => {
    const id = ++snapSeq.current;
    let next: Snapshot;
    try {
      next = await invoke<Snapshot>("get_overview");
    } catch (e) {
      next = { status: "notRunning", message: String(e) };
    }
    if (id === snapSeq.current) setSnap(next);
  }, []);

  const refreshConflicts = useCallback(async () => {
    const id = ++conflictSeq.current;
    try {
      const next = await invoke<FolderConflicts[]>("list_conflicts");
      if (id === conflictSeq.current) setConflicts(next);
    } catch {
      // Not connected yet; the overview already explains why.
    }
  }, []);

  useEffect(() => {
    invoke<Profile>("get_profile").then(setProfile).catch(() => setProfile(null));
  }, []);

  const reloadWs = useCallback(() => {
    invoke<Workspace>("get_workspace").then(setWs).catch(() => undefined);
  }, []);
  useEffect(() => {
    reloadWs();
  }, [reloadWs]);
  // A team computer's join state changes when the central approves it.
  useEffect(() => {
    if (ws.joining) reloadWs();
  }, [snap, ws.joining, reloadWs]);

  const changeRole = useCallback(
    async (r: Role) => {
      try {
        await invoke("set_role", { role: r });
        reloadWs();
      } catch (e) {
        toast("error", `Não foi possível salvar: ${e}`);
      }
    },
    [toast, reloadWs],
  );

  useEffect(() => {
    refreshConflicts();
    const id = setInterval(refreshConflicts, CONFLICT_POLL_MS);
    const unlisten = listen("conflicts-changed", refreshConflicts).catch(() => null);
    return () => {
      clearInterval(id);
      unlisten.then((f) => f?.());
    };
  }, [refreshConflicts]);

  useEffect(() => {
    refresh();
    const id = setInterval(refresh, POLL_MS);
    // If the event bridge is unavailable, polling alone keeps the panel fresh.
    const unlisten = listen("sync-changed", refresh).catch(() => null);
    return () => {
      clearInterval(id);
      unlisten.then((f) => f?.());
    };
  }, [refresh]);

  const overview = snap?.status === "ok" ? snap.overview : null;
  const state = snap ? snap.status : "loading";
  const nConflicts = totalConflicts(conflicts);

  const actions: Actions = useMemo(
    () => ({
      refresh,
      refreshConflicts,
      add: () => setAdding(true),
      ignores: setEditing,
      share: setSharing,
      versions: setVersioning,
      go: setView,
    }),
    [refresh, refreshConflicts],
  );

  const refreshAll = useCallback(async () => {
    await Promise.all([refresh(), refreshConflicts()]);
  }, [refresh, refreshConflicts]);

  // Keyboard: 1–4 switch screens, N adds a folder, R refreshes. Ignored
  // while typing or when a dialog is open.
  const modalOpen = adding || !!editing || !!versioning || prefs || !!sharing;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (modalOpen || e.metaKey || e.ctrlKey || e.altKey) return;
      const el = e.target as HTMLElement | null;
      if (el && (el.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(el.tagName))) return;
      const i = Number(e.key) - 1;
      if (i >= 0 && i < VIEW_KEYS.length) setView(VIEW_KEYS[i]);
      else if (e.key.toLowerCase() === "n" && overview && overview.devices.length > 0) setAdding(true);
      else if (e.key.toLowerCase() === "r") refreshAll();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [modalOpen, overview, refreshAll]);

  // One key per screen: switching views crossfades, data updates don't.
  const screen = overview || view === "help" ? view : state;

  return (
    <WorkspaceContext.Provider value={{ ws, reload: reloadWs }}>
    <RoleContext.Provider value={role}>
    <div className="shell">
      <Sidebar overview={overview} profile={profile} view={view} conflicts={nConflicts} onSelect={setView} onPreferences={() => setPrefs(true)} />
      <div className="main-col">
        <Topbar view={view} onRefresh={refreshAll} />
        <main>
          <AnimatePresence mode="wait" initial={false}>
            <motion.div
              key={screen}
              initial={{ opacity: 0, y: m.reduce ? 0 : motionTokens.distance.xs }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0 }}
              transition={{ duration: motionTokens.duration.fast, ease: motionTokens.easing.smooth }}
            >
              {state === "loading" && view !== "help" && <Skeleton />}
              {snap?.status === "notInstalled" && view !== "help" && <SetupView onReady={refreshAll} />}
              {snap?.status === "notRunning" && view !== "help" && (
                <Problem
                  title="O Syncthing local não está respondendo."
                  message={`${snap.message} — o serviço reinicia sozinho; esta tela se atualiza quando ele voltar.`}
                />
              )}
              {overview && view === "overview" && <OverviewView overview={overview} profile={profile} conflicts={conflicts} actions={actions} />}
              {overview && view === "folders" && <FoldersView overview={overview} actions={actions} />}
              {overview && view === "computers" && <ComputersView overview={overview} profile={profile} actions={actions} />}
              {overview && view === "pending" && <PendingView overview={overview} conflicts={conflicts} actions={actions} />}
              {view === "help" && <HelpView go={setView} />}
            </motion.div>
          </AnimatePresence>
        </main>
      </div>
      <AnimatePresence>
        {adding && overview && (
          <AddFolder
            key="add"
            devices={overview.devices}
            onClose={() => setAdding(false)}
            onDone={(label) => {
              setAdding(false);
              toast("ok", `"${label}" agora é sincronizada. Aceite a pasta em cada computador escolhido.`);
              refresh();
            }}
          />
        )}
        {editing && <IgnoreEditor key="ign" folder={editing} onClose={() => setEditing(null)} />}
        {versioning && <VersioningEditor key="ver" folder={versioning} onClose={() => setVersioning(null)} />}
        {sharing && overview && <ShareEditor key="share" folder={sharing} devices={overview.devices} onClose={() => setSharing(null)} onSaved={refresh} />}
        {prefs && <Preferences key="prefs" role={role} onRole={changeRole} onClose={() => setPrefs(false)} />}
      </AnimatePresence>
    </div>
    </RoleContext.Provider>
    </WorkspaceContext.Provider>
  );
}

export default function App() {
  return (
    <ToastProvider>
      <AppShell />
    </ToastProvider>
  );
}
