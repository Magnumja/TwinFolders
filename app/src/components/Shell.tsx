"use client";
import { useEffect, useState, type ReactNode } from "react";
import { motion } from "motion/react";
import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import type { Overview, Profile, View } from "../types";
import { displayName, initial } from "../lib/space";
import { springs } from "../lib/motion-tokens";
import { useSafeMotion } from "../hooks/useSafeMotion";
import { linkState } from "./Views";
import { Modal } from "./Modal";
import { IconFolder, IconGear, IconGrid, IconHelp, IconInbox, IconMonitor, IconRefresh, IconShield, SwapText } from "./ui";

export const VIEW_KEYS: View[] = ["overview", "folders", "computers", "pending", "help"];

export const VIEW_TITLE: Record<View, string> = {
  overview: "Visão geral",
  folders: "Pastas",
  computers: "Computadores",
  pending: "Pendências",
  help: "Como funciona",
};

function NavItem({
  view,
  current,
  icon,
  count,
  alert,
  onSelect,
}: {
  view: View;
  current: View;
  icon: ReactNode;
  count?: number;
  alert?: boolean;
  onSelect: (v: View) => void;
}) {
  const active = view === current;
  return (
    <li>
      <button
        className={active ? "nav-item active" : "nav-item"}
        aria-current={active ? "page" : undefined}
        aria-keyshortcuts={String(VIEW_KEYS.indexOf(view) + 1)}
        title={`${VIEW_TITLE[view]} (${VIEW_KEYS.indexOf(view) + 1})`}
        onClick={() => onSelect(view)}
      >
        {active && <motion.span layoutId="nav-active" className="nav-active" transition={springs.snappy} />}
        <span className="nav-icon">{icon}</span>
        <span className="nav-label">{VIEW_TITLE[view]}</span>
        {count !== undefined && count > 0 && <span className={alert ? "nav-count alert" : "nav-count"}>{count}</span>}
      </button>
    </li>
  );
}

export function Sidebar({
  overview,
  profile,
  view,
  conflicts,
  onSelect,
  onPreferences,
}: {
  overview: Overview | null;
  profile: Profile | null;
  view: View;
  conflicts: number;
  onSelect: (v: View) => void;
  onPreferences: () => void;
}) {
  const link = overview ? linkState(overview.devices) : "none";
  const linkText = { flow: "Trocando arquivos", on: "Conectados", off: "Sem conexão agora", none: "Ainda não pareado" }[link];
  const pending = overview ? overview.pendingDevices.length + overview.pendingFolders.length + conflicts : 0;
  const user = profile?.user ?? "";
  return (
    <aside className="sidebar">
      <div className="side-brand">
        <img src="/logo-mark.svg" alt="" width={34} height={22} />
        <span>
          <strong>TwinFolders</strong>
          <span className={`brand-status brand-status-${link}`}>
            <span className="space-link" aria-hidden="true" /> {linkText}
          </span>
        </span>
      </div>

      <nav aria-label="Seções">
        <ul className="nav">
          <NavItem view="overview" current={view} icon={<IconGrid />} onSelect={onSelect} />
          <NavItem view="folders" current={view} icon={<IconFolder />} count={overview?.folders.length} onSelect={onSelect} />
          <NavItem view="computers" current={view} icon={<IconMonitor />} count={overview?.devices.length} onSelect={onSelect} />
          <NavItem view="pending" current={view} icon={<IconInbox />} count={pending} alert onSelect={onSelect} />
          <NavItem view="help" current={view} icon={<IconHelp />} onSelect={onSelect} />
        </ul>
      </nav>

      <div className="side-foot">
        <div className="privacy">
          <IconShield size={20} />
          <strong>Só entre seus computadores.</strong>
          <span>Seus arquivos viajam direto. Sem nuvem de terceiros.</span>
        </div>
        <button className="nav-item" onClick={onPreferences}>
          <span className="nav-icon"><IconGear /></span>
          <span className="nav-label">Preferências</span>
        </button>
        <div className="me">
          <span className="avatar">{initial(user)}</span>
          <span className="grow">
            <strong className="ellipsis">{displayName(user)}</strong>
            <span className="ellipsis">em {overview?.myName || "este computador"}</span>
          </span>
          {profile && <small className="muted">v{profile.version}</small>}
        </div>
      </div>
    </aside>
  );
}

export function Topbar({ view, profile, onRefresh }: { view: View; profile: Profile | null; onRefresh: () => Promise<void> }) {
  const [spinning, setSpinning] = useState(false);
  const m = useSafeMotion();
  const refresh = async () => {
    setSpinning(true);
    try {
      await onRefresh();
    } finally {
      setTimeout(() => setSpinning(false), 400);
    }
  };
  return (
    <div className="topbar">
      <nav className="crumbs" aria-label="Você está em">
        <span className="muted">Meu espaço</span>
        <span className="crumb-sep" aria-hidden="true">/</span>
        <SwapText swapKey={view}>{VIEW_TITLE[view]}</SwapText>
      </nav>
      <span className="spacer" />
      <button className="icon-btn round" onClick={refresh} aria-label="Atualizar agora" title="Atualizar agora">
        <motion.span
          style={{ display: "grid" }}
          animate={spinning && !m.reduce ? { rotate: 360 } : { rotate: 0 }}
          transition={spinning ? { duration: 0.6, ease: "linear" } : { duration: 0 }}
        >
          <IconRefresh />
        </motion.span>
      </button>
      <span className="avatar small" title={displayName(profile?.user ?? "")}>{initial(profile?.user ?? "")}</span>
    </div>
  );
}

export function Preferences({ onClose }: { onClose: () => void }) {
  const [autostart, setAutostart] = useState<boolean | null>(null);
  useEffect(() => {
    isEnabled().then(setAutostart).catch(() => setAutostart(null));
  }, []);
  const toggle = async () => {
    try {
      if (autostart) await disable();
      else await enable();
    } catch {
      // Re-read below so the checkbox stays truthful even if the OS refused.
    }
    setAutostart(await isEnabled().catch(() => autostart));
  };
  return (
    <Modal title="Preferências" onClose={onClose}>
      <div className="form">
        <label className="pref-row">
          <span className="grow">
            <strong>Abrir ao entrar no computador</strong>
            <span className="muted small block">O ícone fica na barra de menus. A sincronização roda em segundo plano mesmo com o app fechado.</span>
          </span>
          <input type="checkbox" className="switch" checked={!!autostart} disabled={autostart === null} onChange={toggle} />
        </label>
        <div className="actions">
          <button className="btn" onClick={onClose}>Fechar</button>
        </div>
      </div>
    </Modal>
  );
}
