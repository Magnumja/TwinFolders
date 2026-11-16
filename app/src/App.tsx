"use client";
import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { ask } from "@tauri-apps/plugin-dialog";
import { AnimatePresence, motion } from "motion/react";
import type { DeviceView, FolderView, Overview, Snapshot } from "./types";
import { formatBytes, formatRate, relativeTime, shortId } from "./lib/format";
import { describeHealth, overallStatus, type Tone } from "./lib/health";
import { motionTokens, springs } from "./lib/motion-tokens";
import { staggerContainer, useSafeMotion } from "./hooks/useSafeMotion";
import { AddFolder } from "./components/AddFolder";
import { IgnoreEditor } from "./components/IgnoreEditor";
import { Pending } from "./components/Pending";
import { ToastProvider, useToast } from "./components/Toasts";

const POLL_MS = 3000;

function Dot({ tone }: { tone: Tone }) {
  return <span className={`dot dot-${tone}`} aria-hidden="true" />;
}

/** Crossfades when the *meaning* (tone) changes, not on every % tick. */
function SwapText({ swapKey, className, children }: { swapKey: string; className?: string; children: React.ReactNode }) {
  const m = useSafeMotion(motionTokens.distance.xs);
  return (
    <AnimatePresence mode="wait" initial={false}>
      <motion.span
        key={swapKey}
        className={className}
        initial={m.initial}
        animate={m.animate}
        exit={m.exit}
        transition={{ duration: motionTokens.duration.fast, ease: motionTokens.easing.smooth }}
      >
        {children}
      </motion.span>
    </AnimatePresence>
  );
}

function PressButton(props: React.ComponentProps<typeof motion.button>) {
  const m = useSafeMotion();
  return <motion.button whileTap={m.press} transition={springs.snappy} {...props} />;
}

function Header({ overview, onAdd }: { overview: Overview | null; onAdd: () => void }) {
  const [copied, setCopied] = useState(false);
  const status = overview ? overallStatus(overview) : null;
  const copy = async () => {
    if (!overview) return;
    await navigator.clipboard.writeText(overview.myId);
    setCopied(true);
    setTimeout(() => setCopied(false), 1600);
  };
  return (
    <header className="header">
      <div className="brand">
        <img src="/icon.png" alt="" width={28} height={28} />
        <h1>TwinFolders</h1>
      </div>
      {status && (
        <div className={`pill pill-${status.tone}`} role="status" aria-live="polite">
          <Dot tone={status.tone} />
          <SwapText swapKey={status.label}>{status.label}</SwapText>
        </div>
      )}
      {overview && (
        <PressButton className="btn btn-primary" onClick={onAdd}>
          + Adicionar pasta
        </PressButton>
      )}
      {overview && (
        <PressButton className="device-id" onClick={copy} title="Copiar o ID deste computador (use no outro para parear)">
          <span className="muted">Este computador</span>
          <code>{shortId(overview.myId)}</code>
          <SwapText swapKey={copied ? "c" : "n"} className={copied ? "copy copied" : "copy"}>
            {copied ? "copiado ✓" : "copiar ID"}
          </SwapText>
        </PressButton>
      )}
    </header>
  );
}

/** Card/row entrance used inside a stagger container; exit when removed. */
function useItemVariants() {
  const m = useSafeMotion(motionTokens.distance.md);
  return {
    variants: {
      hidden: m.initial,
      visible: { ...m.animate, transition: m.reduce ? m.fade : springs.snappy },
    },
    exit: { ...m.exit, transition: { duration: motionTokens.duration.fast } },
  };
}

function DeviceCard({ d }: { d: DeviceView }) {
  const item = useItemVariants();
  const tone: Tone = d.paused ? "muted" : d.connected ? "ok" : "wait";
  const state = d.paused ? "Pausado" : d.connected ? "Online" : "Offline";
  return (
    <motion.article className="card device" variants={item.variants} exit={item.exit}>
      <div className="row">
        <Dot tone={tone} />
        <strong className="grow ellipsis">{d.name || shortId(d.id)}</strong>
        <SwapText swapKey={tone} className={`tag tag-${tone}`}>{state}</SwapText>
      </div>
      {d.connected ? (
        <div className="rates">
          <span title="Recebendo"><span className="muted">↓</span> {formatRate(d.inRate)}</span>
          <span title="Enviando"><span className="muted">↑</span> {formatRate(d.outRate)}</span>
        </div>
      ) : (
        <p className="muted small">Visto pela última vez: {relativeTime(d.lastSeen)}</p>
      )}
      <p className="muted small mono">{shortId(d.id)}</p>
    </motion.article>
  );
}

/** Progress via scaleX (transform only, rule 4) on a non-overshooting spring. */
function Progress({ percent, tone }: { percent: number; tone: Tone }) {
  const m = useSafeMotion();
  return (
    <div className={`bar bar-${tone}`} role="progressbar" aria-valuenow={Math.round(percent)} aria-valuemin={0} aria-valuemax={100}>
      <motion.span
        initial={false}
        animate={{ scaleX: Math.max(0, Math.min(100, percent)) / 100 }}
        transition={m.reduce ? { duration: 0 } : springs.progress}
      />
    </div>
  );
}

function FolderRow({ f, onIgnores, onChange }: { f: FolderView; onIgnores: () => void; onChange: () => void }) {
  const item = useItemVariants();
  const toast = useToast();
  const h = describeHealth(f.health);
  const percent =
    f.health.kind === "syncing" ? f.health.percent : f.health.kind === "upToDate" || f.health.kind === "waitingForPeer" ? 100 : 0;

  const remove = async () => {
    const ok = await ask(
      `Parar de sincronizar "${f.label}"?\n\nOs arquivos continuam neste computador e no outro; só a sincronização é desligada.`,
      { title: "Remover pasta da sincronização", kind: "warning", okLabel: "Parar de sincronizar", cancelLabel: "Cancelar" },
    );
    if (!ok) return;
    try {
      await invoke("remove_folder", { folderId: f.id });
      toast("ok", `"${f.label}" não é mais sincronizada. Os arquivos foram mantidos.`);
    } catch (e) {
      toast("error", `Não foi possível remover: ${e}`);
    } finally {
      onChange();
    }
  };

  return (
    <motion.article className="card folder" variants={item.variants} exit={item.exit}>
      <div className="row">
        <span className={`folder-icon tone-${h.tone}`} aria-hidden="true">
          <svg viewBox="0 0 24 24" width="22" height="22"><path d="M3 6.5A1.5 1.5 0 0 1 4.5 5h4.6l2 2h8.4A1.5 1.5 0 0 1 21 8.5v9a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 17.5z" fill="currentColor" /></svg>
        </span>
        <div className="grow">
          <strong>{f.label}</strong>
          <p className="muted small mono path" title={f.path}>{f.path}</p>
        </div>
        <SwapText swapKey={h.tone} className={`tag tag-${h.tone}`}>{h.label}</SwapText>
      </div>
      <Progress percent={percent} tone={h.tone} />
      <AnimatePresence initial={false}>
        {h.detail && (
          <motion.p
            key="detail"
            className="error small"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: motionTokens.duration.fast }}
          >
            {h.detail}
          </motion.p>
        )}
      </AnimatePresence>
      <div className="meta small">
        <span>{formatBytes(f.globalBytes)}</span>
        {f.needBytes > 0 && <span>faltam {formatBytes(f.needBytes)}</span>}
        {f.peers.map((p) => (
          <span key={p.deviceId}>
            <Dot tone={p.connected ? "ok" : "wait"} /> {p.name || shortId(p.deviceId)}: {p.connected ? `${p.completion.toLocaleString("pt-BR")}%` : "offline"}
          </span>
        ))}
        <span className="muted">última sincronização: {relativeTime(f.lastSync)}</span>
        <span className="spacer" />
        <button className="link-btn" onClick={onIgnores}>Exclusões</button>
        <button className="link-btn danger" onClick={remove}>Remover</button>
      </div>
    </motion.article>
  );
}

function Skeleton() {
  return (
    <div aria-busy="true" aria-label="Conectando ao Syncthing local">
      <div className="sk sk-title" />
      <div className="grid">
        <div className="card sk-card"><div className="sk sk-line" /><div className="sk sk-line short" /></div>
        <div className="card sk-card"><div className="sk sk-line" /><div className="sk sk-line short" /></div>
      </div>
      <div className="sk sk-title" />
      {[0, 1].map((i) => (
        <div key={i} className="card sk-card"><div className="sk sk-line" /><div className="sk sk-bar" /><div className="sk sk-line short" /></div>
      ))}
    </div>
  );
}

function Empty({ icon, title, text, action }: { icon: string; title: string; text: string; action?: React.ReactNode }) {
  const m = useSafeMotion(motionTokens.distance.sm);
  return (
    <motion.div className="empty" initial={m.initial} animate={m.animate} transition={m.reduce ? m.fade : springs.gentle}>
      <motion.span
        className="empty-icon"
        aria-hidden="true"
        initial={m.reduce ? false : { scale: motionTokens.scale.press }}
        animate={{ scale: 1 }}
        transition={springs.bouncy}
      >
        {icon}
      </motion.span>
      <strong>{title}</strong>
      <p className="muted">{text}</p>
      {action}
    </motion.div>
  );
}

function Problem({ title, message }: { title: string; message: string }) {
  return (
    <section className="card problem">
      <h2>{title}</h2>
      <p>{message}</p>
    </section>
  );
}

function AutostartToggle() {
  const [on, setOn] = useState<boolean | null>(null);
  useEffect(() => {
    isEnabled().then(setOn).catch(() => setOn(null));
  }, []);
  if (on === null) return null;
  const toggle = async () => {
    if (on) await disable();
    else await enable();
    setOn(await isEnabled());
  };
  return (
    <label className="toggle">
      <input type="checkbox" checked={on} onChange={toggle} />
      Abrir o TwinFolders ao entrar no computador
    </label>
  );
}

function Dashboard({ overview, refresh, onAdd, onEdit }: { overview: Overview; refresh: () => void; onAdd: () => void; onEdit: (f: FolderView) => void }) {
  // `layout` only for small lists (motion-patterns rule 4).
  const smallList = overview.folders.length <= 6;
  return (
    <>
      <Pending overview={overview} onChange={refresh} />
      <section>
        <h2>Computadores <span className="count">{overview.devices.length}</span></h2>
        {overview.devices.length === 0 ? (
          <Empty icon="🔗" title="Nenhum computador pareado" text="Copie o ID deste computador (no topo) e adicione-o no outro computador." />
        ) : (
          <motion.div className="grid" variants={staggerContainer} initial="hidden" animate="visible">
            <AnimatePresence mode="popLayout">
              {overview.devices.map((d) => <DeviceCard key={d.id} d={d} />)}
            </AnimatePresence>
          </motion.div>
        )}
      </section>
      <section>
        <h2>Pastas <span className="count">{overview.folders.length}</span></h2>
        {overview.folders.length === 0 ? (
          <Empty
            icon="📂"
            title="Nenhuma pasta sincronizada"
            text="Escolha uma pasta para manter igual nos dois computadores."
            action={overview.devices.length > 0 ? <button className="btn btn-primary" onClick={onAdd}>+ Adicionar pasta</button> : undefined}
          />
        ) : (
          <motion.div className="list" variants={staggerContainer} initial="hidden" animate="visible" layout={smallList}>
            <AnimatePresence mode="popLayout">
              {overview.folders.map((f) => (
                <FolderRow key={f.id} f={f} onIgnores={() => onEdit(f)} onChange={refresh} />
              ))}
            </AnimatePresence>
          </motion.div>
        )}
      </section>
    </>
  );
}

function Shell() {
  const [snap, setSnap] = useState<Snapshot | null>(null);
  const [adding, setAdding] = useState(false);
  const [editing, setEditing] = useState<FolderView | null>(null);
  const toast = useToast();
  const m = useSafeMotion(motionTokens.distance.sm);

  const refresh = useCallback(async () => {
    try {
      setSnap(await invoke<Snapshot>("get_overview"));
    } catch (e) {
      setSnap({ status: "notRunning", message: String(e) });
    }
  }, []);

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
  const view = snap ? snap.status : "loading";

  return (
    <div className="app">
      <Header overview={overview} onAdd={() => setAdding(true)} />
      <main>
        <AnimatePresence mode="wait" initial={false}>
          <motion.div
            key={view}
            initial={{ opacity: 0, y: m.reduce ? 0 : motionTokens.distance.xs }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0 }}
            transition={{ duration: motionTokens.duration.fast, ease: motionTokens.easing.smooth }}
          >
            {view === "loading" && <Skeleton />}
            {snap?.status === "notInstalled" && <Problem title="Serviço não instalado" message={snap.message} />}
            {snap?.status === "notRunning" && (
              <Problem title="Syncthing não está respondendo" message={`${snap.message} — o serviço reinicia sozinho; esta tela se atualiza quando ele voltar.`} />
            )}
            {overview && <Dashboard overview={overview} refresh={refresh} onAdd={() => setAdding(true)} onEdit={setEditing} />}
          </motion.div>
        </AnimatePresence>
      </main>
      <AnimatePresence>
        {adding && overview && (
          <AddFolder
            key="add"
            devices={overview.devices}
            onClose={() => setAdding(false)}
            onDone={(label) => {
              setAdding(false);
              toast("ok", `"${label}" agora é sincronizada. Aceite a pasta no outro computador.`);
              refresh();
            }}
          />
        )}
        {editing && <IgnoreEditor key="ign" folder={editing} onClose={() => setEditing(null)} />}
      </AnimatePresence>
      <footer className="footer">
        <AutostartToggle />
        <span className="muted small">A sincronização roda em segundo plano mesmo com esta janela fechada.</span>
      </footer>
    </div>
  );
}

export default function App() {
  return (
    <ToastProvider>
      <Shell />
    </ToastProvider>
  );
}
