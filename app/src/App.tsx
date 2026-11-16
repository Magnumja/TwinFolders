import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import type { DeviceView, FolderView, Overview, Snapshot } from "./types";
import { formatBytes, formatRate, relativeTime, shortId } from "./lib/format";
import { describeHealth, overallStatus, type Tone } from "./lib/health";

const POLL_MS = 3000;

function Dot({ tone }: { tone: Tone }) {
  return <span className={`dot dot-${tone}`} aria-hidden="true" />;
}

function Header({ overview }: { overview: Overview | null }) {
  const [copied, setCopied] = useState(false);
  const status = overview ? overallStatus(overview) : null;
  const copy = async () => {
    if (!overview) return;
    await navigator.clipboard.writeText(overview.myId);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };
  return (
    <header className="header">
      <div className="brand">
        <img src="/icon.png" alt="" width={28} height={28} />
        <h1>TwinFolders</h1>
      </div>
      {status && (
        <div className={`pill pill-${status.tone}`} role="status">
          <Dot tone={status.tone} />
          {status.label}
        </div>
      )}
      {overview && (
        <button className="device-id" onClick={copy} title="Copiar o ID deste computador (use no outro para parear)">
          <span className="muted">Este computador</span>
          <code>{shortId(overview.myId)}</code>
          <span className="copy">{copied ? "copiado ✓" : "copiar ID"}</span>
        </button>
      )}
    </header>
  );
}

function DeviceCard({ d }: { d: DeviceView }) {
  const tone: Tone = d.paused ? "muted" : d.connected ? "ok" : "wait";
  const state = d.paused ? "Pausado" : d.connected ? "Online" : "Offline";
  return (
    <article className="card device">
      <div className="row">
        <Dot tone={tone} />
        <strong>{d.name || shortId(d.id)}</strong>
        <span className={`tag tag-${tone}`}>{state}</span>
      </div>
      {d.connected ? (
        <div className="rates">
          <span title="Recebendo">↓ {formatRate(d.inRate)}</span>
          <span title="Enviando">↑ {formatRate(d.outRate)}</span>
        </div>
      ) : (
        <p className="muted small">Visto pela última vez: {relativeTime(d.lastSeen)}</p>
      )}
      <p className="muted small mono">{shortId(d.id)}</p>
    </article>
  );
}

function FolderRow({ f }: { f: FolderView }) {
  const h = describeHealth(f.health);
  const percent =
    f.health.kind === "syncing" ? f.health.percent : f.health.kind === "upToDate" || f.health.kind === "waitingForPeer" ? 100 : 0;
  return (
    <article className="card folder">
      <div className="row">
        <span className="folder-icon" aria-hidden="true">📁</span>
        <div className="grow">
          <strong>{f.label}</strong>
          <p className="muted small mono path" title={f.path}>{f.path}</p>
        </div>
        <span className={`tag tag-${h.tone}`}>{h.label}</span>
      </div>
      <div className={`bar bar-${h.tone}`} role="progressbar" aria-valuenow={percent} aria-valuemin={0} aria-valuemax={100}>
        <span style={{ width: `${percent}%` }} />
      </div>
      {h.detail && <p className="error small">{h.detail}</p>}
      <div className="meta small">
        <span>{formatBytes(f.globalBytes)}</span>
        {f.needBytes > 0 && <span>faltam {formatBytes(f.needBytes)}</span>}
        {f.peers.map((p) => (
          <span key={p.deviceId}>
            <Dot tone={p.connected ? "ok" : "wait"} /> {p.name || shortId(p.deviceId)}: {p.connected ? `${p.completion.toLocaleString("pt-BR")}%` : "offline"}
          </span>
        ))}
        <span className="muted">última sincronização: {relativeTime(f.lastSync)}</span>
      </div>
    </article>
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

export default function App() {
  const [snap, setSnap] = useState<Snapshot | null>(null);

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
    const unlisten = listen("sync-changed", refresh);
    return () => {
      clearInterval(id);
      unlisten.then((f) => f());
    };
  }, [refresh]);

  const overview = snap?.status === "ok" ? snap.overview : null;

  return (
    <div className="app">
      <Header overview={overview} />
      <main>
        {!snap && <p className="muted">Conectando ao Syncthing local…</p>}
        {snap?.status === "notInstalled" && <Problem title="Serviço não instalado" message={snap.message} />}
        {snap?.status === "notRunning" && (
          <Problem title="Syncthing não está respondendo" message={`${snap.message} — o serviço reinicia sozinho; esta tela se atualiza quando ele voltar.`} />
        )}
        {overview && (
          <>
            <section>
              <h2>Computadores</h2>
              {overview.devices.length === 0 ? (
                <p className="muted">Nenhum computador pareado ainda. Copie o ID acima e adicione no outro computador.</p>
              ) : (
                <div className="grid">{overview.devices.map((d) => <DeviceCard key={d.id} d={d} />)}</div>
              )}
            </section>
            <section>
              <h2>Pastas</h2>
              {overview.folders.length === 0 ? (
                <p className="muted">Nenhuma pasta sincronizada ainda.</p>
              ) : (
                <div className="list">{overview.folders.map((f) => <FolderRow key={f.id} f={f} />)}</div>
              )}
            </section>
          </>
        )}
      </main>
      <footer className="footer">
        <AutostartToggle />
        <span className="muted small">A sincronização roda em segundo plano mesmo com esta janela fechada.</span>
      </footer>
    </div>
  );
}
