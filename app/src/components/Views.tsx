"use client";
import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import type { DeviceView, FolderConflicts, FolderView, Overview, Profile, View } from "../types";
import { formatBytes, formatRate, relativeTime, shortId } from "../lib/format";
import { describeHealth } from "../lib/health";
import { totalConflicts } from "../lib/conflicts";
import {
  activeTransfer,
  beamSeconds,
  connectionKind,
  filterFolders,
  folderSummary,
  formatEta,
  hero,
  osLabel,
  totals,
  type FolderTab,
} from "../lib/space";
import { motionTokens, springs } from "../lib/motion-tokens";
import { useSafeMotion } from "../hooks/useSafeMotion";
import { Conflicts } from "./Conflicts";
import { Pending } from "./Pending";
import { useToast } from "./Toasts";
import { AddDeviceForm, ConnectGuide, IdBlocks } from "./Guide";
import { pairingStage } from "../lib/pairing";
import { RemotePanel } from "./Remote";
import {
  Dot,
  Empty,
  IconAlert,
  IconCopy,
  IconFolder,
  IconInbox,
  IconLaptop,
  IconLock,
  IconMonitor,
  IconMore,
  IconPlus,
  IconSearch,
  PressButton,
  Progress,
} from "./ui";

export type LinkState = "flow" | "on" | "off" | "none";

/** Aggregate state of the connection between this computer and its peers. */
export function linkState(devices: DeviceView[]): LinkState {
  if (devices.length === 0) return "none";
  const live = devices.filter((d) => d.connected && !d.paused);
  if (live.length === 0) return "off";
  return live.some((d) => d.inRate >= 1 || d.outRate >= 1) ? "flow" : "on";
}

export interface Actions {
  refresh: () => Promise<void>;
  refreshConflicts: () => Promise<void>;
  add: () => void;
  ignores: (f: FolderView) => void;
  versions: (f: FolderView) => void;
  go: (v: View) => void;
}

/* ---------- shared pieces ---------- */

function Hero({ title, sub, action }: { title: string; sub: string; action?: ReactNode }) {
  const m = useSafeMotion(motionTokens.distance.xs);
  return (
    <header className="hero">
      <div className="grow">
        <AnimatePresence mode="wait" initial={false}>
          <motion.h1 key={title} initial={m.initial} animate={m.animate} exit={m.exit} transition={{ duration: motionTokens.duration.fast }}>
            {title}
          </motion.h1>
        </AnimatePresence>
        <p className="hero-sub">{sub}</p>
      </div>
      {action}
    </header>
  );
}

function AddButton({ onClick }: { onClick: () => void }) {
  return (
    <PressButton className="btn btn-primary" onClick={onClick} title="Adicionar pasta (N)">
      <IconPlus /> Adicionar pasta
    </PressButton>
  );
}

/* ---------- the connection beam (signature element) ---------- */

/** One direction of the link. Packets travel at a speed tied to the real rate. */
function Lane({ dir, rate, live, label }: { dir: "up" | "down"; rate: number; live: boolean; label: string }) {
  const track = useRef<HTMLDivElement>(null);
  const reduce = useReducedMotion();
  const [len, setLen] = useState(0);
  useLayoutEffect(() => {
    const el = track.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setLen(el.clientWidth));
    ro.observe(el);
    setLen(el.clientWidth);
    return () => ro.disconnect();
  }, []);
  const moving = live && rate >= 1 && !reduce && len > 0;
  const secs = beamSeconds(rate);
  return (
    <div className={`lane lane-${dir}${live ? "" : " lane-idle"}`}>
      <span className="lane-label">{label}</span>
      <div className="track" ref={track} style={{ ["--len" as string]: `${len}px`, ["--dur" as string]: `${secs}s` }}>
        {moving &&
          [0, 1, 2].map((i) => <span key={i} className="packet" style={{ animationDelay: `${(-secs * i) / 3}s` }} />)}
      </div>
    </div>
  );
}

function Beam({ overview }: { overview: Overview }) {
  const t = totals(overview);
  const state = linkState(overview.devices);
  const live = state === "flow" || state === "on";
  return (
    <div className={`beam beam-${state}`} aria-hidden="true">
      <Lane dir="up" rate={t.up} live={live} label={t.up >= 1 ? `Enviando ${formatRate(t.up)}` : live ? "Nada saindo" : ""} />
      <Lane dir="down" rate={t.down} live={live} label={t.down >= 1 ? `Recebendo ${formatRate(t.down)}` : live ? "Nada chegando" : ""} />
    </div>
  );
}

function Machine({ name, lines, online, kind, side }: { name: string; lines: string[]; online: { tone: "ok" | "wait" | "muted"; text: string }; kind: "laptop" | "monitor"; side: "here" | "there" | "ghost" }) {
  return (
    <div className={`machine machine-${side}`}>
      <span className="machine-icon">{kind === "laptop" ? <IconLaptop size={24} /> : <IconMonitor size={24} />}</span>
      <span className="machine-text">
        <strong className="ellipsis">{name}</strong>
        {lines.map((l) => (
          <span key={l} className="ellipsis">{l}</span>
        ))}
        <span className={`machine-state state-${online.tone}`}>
          <Dot tone={online.tone} /> {online.text}
        </span>
      </span>
    </div>
  );
}

function peerOnline(d: DeviceView): { tone: "ok" | "wait" | "muted"; text: string } {
  if (d.paused) return { tone: "muted", text: "Pausado" };
  if (d.connected) return { tone: "ok", text: `Online por ${connectionKind(d.address) || "conexão direta"}` };
  if (!d.lastSeen) return { tone: "muted", text: "Aguardando aprovação" };
  return { tone: "wait", text: `Visto ${relativeTime(d.lastSeen)}` };
}

export function LinkCard({ overview, profile }: { overview: Overview; profile: Profile | null }) {
  const [copied, setCopied] = useState(false);
  const state = linkState(overview.devices);
  const summary = {
    flow: "Os dois estão trocando arquivos agora.",
    on: "Os dois estão conectados e em dia.",
    off: overview.devices.some((d) => d.lastSeen) ? "O outro computador está desligado ou sem rede." : "Esperando o outro computador aprovar o pedido.",
    none: "Nenhum outro computador pareado ainda.",
  }[state];
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(overview.myId);
      setCopied(true);
      setTimeout(() => setCopied(false), 1600);
    } catch {
      // Clipboard unavailable: the full ID is in the button's tooltip.
    }
  };
  const os = osLabel(profile?.os ?? "");
  return (
    <section className={`panel link-card link-${state}`}>
      <div className="link-grid">
        <Machine
          name={overview.myName || "Este computador"}
          lines={[os ? `Este computador, ${os}` : "Este computador"]}
          online={{ tone: "ok", text: "Online" }}
          kind={profile?.os === "macos" ? "laptop" : "monitor"}
          side="here"
        />
        <Beam overview={overview} />
        {overview.devices.length === 0 ? (
          <Machine name="Outro computador" lines={["Adicione este lá com o seu ID"]} online={{ tone: "muted", text: "Aguardando" }} kind="monitor" side="ghost" />
        ) : (
          <div className="machines">
            {overview.devices.map((d) => (
              <Machine key={d.id} name={d.name || shortId(d.id)} lines={[]} online={peerOnline(d)} kind="monitor" side="there" />
            ))}
          </div>
        )}
      </div>
      <div className="panel-foot">
        <span className="foot-note">
          <IconLock /> {summary} A conexão é criptografada e não passa por nenhuma nuvem.
        </span>
        <button className="link-btn" onClick={copy} title={`ID completo: ${overview.myId}`}>
          <IconCopy /> {copied ? "ID copiado" : "Copiar meu ID"}
        </button>
      </div>
    </section>
  );
}

/* ---------- folders ---------- */

function useFolderActions(f: FolderView, actions: Actions) {
  const toast = useToast();
  const reveal = async () => {
    try {
      await invoke("reveal_folder", { folderId: f.id });
    } catch (e) {
      toast("error", String(e));
    }
  };
  const togglePause = async () => {
    const paused = f.health.kind !== "paused";
    try {
      await invoke("set_folder_paused", { folderId: f.id, paused });
      toast("ok", paused ? `"${f.label}" pausada. Nada muda até você retomar.` : `"${f.label}" voltou a sincronizar.`);
    } catch (e) {
      toast("error", `Não foi possível ${paused ? "pausar" : "retomar"}: ${e}`);
    } finally {
      await actions.refresh();
    }
  };
  const remove = async () => {
    const ok = await ask(
      `Parar de sincronizar "${f.label}"?\n\nOs arquivos continuam neste computador e no outro; só a sincronização é desligada.`,
      { title: "Parar de sincronizar", kind: "warning", okLabel: "Parar de sincronizar", cancelLabel: "Cancelar" },
    );
    if (!ok) return;
    try {
      await invoke("remove_folder", { folderId: f.id });
      toast("ok", `"${f.label}" não é mais sincronizada. Os arquivos foram mantidos.`);
    } catch (e) {
      toast("error", `Não foi possível parar: ${e}`);
    } finally {
      await actions.refresh();
    }
  };
  return { reveal, togglePause, remove };
}

function RowMenu({ f, actions }: { f: FolderView; actions: Actions }) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const act = useFolderActions(f, actions);
  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent | KeyboardEvent) => {
      if (e instanceof KeyboardEvent ? e.key === "Escape" : !ref.current?.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener("mousedown", close);
    window.addEventListener("keydown", close);
    return () => {
      window.removeEventListener("mousedown", close);
      window.removeEventListener("keydown", close);
    };
  }, [open]);
  const run = (fn: () => void) => () => {
    setOpen(false);
    fn();
  };
  return (
    <div className="row-menu" ref={ref}>
      <button className="icon-btn" aria-label={`Mais ações para ${f.label}`} aria-haspopup="menu" aria-expanded={open} onClick={() => setOpen((o) => !o)}>
        <IconMore />
      </button>
      <AnimatePresence>
        {open && (
          <motion.div
            className="menu"
            role="menu"
            initial={{ opacity: 0, scale: motionTokens.scale.subtle, y: -4 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: motionTokens.scale.subtle, y: -4 }}
            transition={springs.instant}
          >
            <button role="menuitem" onClick={run(act.reveal)}>Abrir no {navigator.userAgent.includes("Windows") ? "Explorer" : "Finder"}</button>
            <button role="menuitem" onClick={run(act.togglePause)}>{f.health.kind === "paused" ? "Retomar sincronização" : "Pausar sincronização"}</button>
            <button role="menuitem" onClick={run(() => actions.versions(f))}>Histórico de versões</button>
            <button role="menuitem" onClick={run(() => actions.ignores(f))}>Arquivos ignorados</button>
            <hr />
            <button role="menuitem" className="danger" onClick={run(act.remove)}>Parar de sincronizar</button>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

function StatusCell({ f, onResume }: { f: FolderView; onResume: () => void }) {
  const h = describeHealth(f.health);
  if (f.health.kind === "syncing") {
    return (
      <span className="status status-busy" title={h.label}>
        <span className="status-line">{Math.round(f.health.percent)}% sincronizado</span>
        <Progress percent={f.health.percent} tone="busy" thin />
      </span>
    );
  }
  if (f.health.kind === "paused") {
    return (
      <span className="status status-muted">
        <span className="status-line">Pausada</span>
        <button className="inline-link" onClick={onResume}>Retomar</button>
      </span>
    );
  }
  const text = f.health.kind === "upToDate" ? "Em dia" : f.health.kind === "waitingForPeer" ? "Esperando o outro" : h.label;
  return (
    <span className={`status status-${h.tone}`}>
      <span className="status-line">
        <Dot tone={h.tone} /> {text}
      </span>
    </span>
  );
}

function FolderRow({ f, actions, compact }: { f: FolderView; actions: Actions; compact?: boolean }) {
  const h = describeHealth(f.health);
  const act = useFolderActions(f, actions);
  return (
    <motion.div className="tr" role="row" layout="position" exit={{ opacity: 0, transition: { duration: motionTokens.duration.fast } }}>
      <span role="cell" className="cell-name">
        <button className={`folder-icon tone-${h.tone}`} onClick={act.reveal} title="Abrir a pasta" aria-label={`Abrir ${f.label}`}>
          <IconFolder />
        </button>
        <span className="grow">
          <button className="name-btn ellipsis" onClick={act.reveal} title={`Abrir ${f.path}`}>{f.label}</button>
          <span className="sub ellipsis" title={f.path}>{f.path}</span>
          {h.detail && <span className="error small block">{h.detail}</span>}
        </span>
      </span>
      <span role="cell"><StatusCell f={f} onResume={act.togglePause} /></span>
      <span role="cell" className="col-size">
        {formatBytes(f.globalBytes)}
        {f.needBytes > 0 && <span className="sub block">faltam {formatBytes(f.needBytes)}</span>}
      </span>
      {!compact && <span role="cell" className="col-when sub">{relativeTime(f.lastSync)}</span>}
      <span role="cell" className="cell-menu">
        <span className="row-quick">
          <button className="chip" onClick={() => actions.versions(f)}>Versões</button>
        </span>
        <RowMenu f={f} actions={actions} />
      </span>
    </motion.div>
  );
}

export function FoldersTable({ folders, actions, limit, compact, title = "Suas pastas" }: { folders: FolderView[]; actions: Actions; limit?: number; compact?: boolean; title?: string }) {
  const [tab, setTab] = useState<FolderTab>("all");
  const [query, setQuery] = useState("");
  const syncing = folders.filter((f) => f.health.kind === "syncing" || f.health.kind === "scanning").length;
  const shown = filterFolders(folders, tab, query);
  const visible = limit ? shown.slice(0, limit) : shown;
  const bytes = formatBytes(folders.reduce((n, f) => n + f.globalBytes, 0));
  const tabs: { id: FolderTab; label: string; count?: number }[] = [
    { id: "all", label: "Todas" },
    { id: "syncing", label: "Sincronizando", count: syncing },
    { id: "upToDate", label: "Em dia" },
  ];
  return (
    <section className="panel folders">
      <div className="panel-head">
        <div>
          <h2>{title}</h2>
          <p className="sub">{folderSummary(folders.length, bytes)}</p>
        </div>
        {limit && folders.length > limit && (
          <button className="link-btn" onClick={() => actions.go("folders")}>Ver todas as {folders.length}</button>
        )}
      </div>
      {folders.length === 0 ? (
        <Empty
          icon={<IconFolder size={28} />}
          title="Nenhuma pasta ainda"
          text="Escolha uma pasta para manter igual nos dois computadores."
          action={<button className="btn btn-primary" onClick={actions.add}><IconPlus /> Adicionar pasta</button>}
        />
      ) : (
        <>
          <div className="table-tools">
            <div className="tabs" role="tablist" aria-label="Filtrar pastas">
              {tabs.map((t) => (
                <button key={t.id} role="tab" aria-selected={tab === t.id} className={tab === t.id ? "tab active" : "tab"} onClick={() => setTab(t.id)}>
                  {tab === t.id && <motion.span layoutId={`tab-${title}`} className="tab-bg" transition={springs.snappy} />}
                  <span>{t.label}</span>
                  {t.count ? <span className="tab-count">{t.count}</span> : null}
                </button>
              ))}
            </div>
            <label className="search">
              <IconSearch />
              <input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Buscar pasta" aria-label="Buscar pasta" />
            </label>
          </div>
          <div className={compact ? "table compact" : "table"} role="table" aria-label={title}>
            <div className="tr th" role="row">
              <span role="columnheader">Pasta</span>
              <span role="columnheader">Situação</span>
              <span role="columnheader" className="col-size">Tamanho</span>
              {!compact && <span role="columnheader" className="col-when">Última mudança</span>}
              <span role="columnheader" aria-label="Ações" />
            </div>
            <AnimatePresence mode="popLayout" initial={false}>
              {visible.map((f) => (
                <FolderRow key={f.id} f={f} actions={actions} compact={compact} />
              ))}
            </AnimatePresence>
            {visible.length === 0 && (
              <p className="table-empty sub">
                Nenhuma pasta {query ? `com “${query}”` : "nesse filtro"}.{" "}
                <button className="inline-link" onClick={() => { setQuery(""); setTab("all"); }}>Mostrar todas</button>
              </p>
            )}
          </div>
        </>
      )}
    </section>
  );
}

/* ---------- side column ---------- */

function TransferCard({ overview }: { overview: Overview }) {
  const t = activeTransfer(overview);
  const from = overview.devices.find((d) => d.connected && d.inRate >= 1);
  const newest = overview.folders.map((f) => f.lastSync).filter(Boolean).sort().pop() ?? null;
  return (
    <section className="panel transfer">
      {t ? (
        <div className="transfer-body">
          <p className="side-title">Chegando agora</p>
          <strong className="ellipsis">{t.folder.label}</strong>
          <div className="transfer-numbers">
            <span className="big">{Math.round(t.percent)}<small>%</small></span>
            <span className="sub">faltam {formatBytes(t.folder.needBytes)}</span>
          </div>
          <Progress percent={t.percent} tone="busy" />
          <p className="sub">
            {t.rate >= 1 ? `${formatRate(t.rate)} vindo de ${from?.name || "outro computador"}` : "Esperando dados do outro computador"}
            {t.etaSeconds !== null && <>, {formatEta(t.etaSeconds).replace("~", "cerca de ").replace(" restantes", "")}</>}
          </p>
        </div>
      ) : (
        <div className="transfer-idle">
          <p className="side-title">Nada chegando agora</p>
          <p className="sub">{newest ? `A última mudança chegou ${relativeTime(newest)}.` : "Quando um arquivo mudar no outro computador, você vê o progresso aqui."}</p>
        </div>
      )}
    </section>
  );
}

function Callout({ tone, icon, title, text, onClick }: { tone: "wait" | "busy"; icon: ReactNode; title: string; text: string; onClick: () => void }) {
  return (
    <motion.button
      className={`callout callout-${tone}`}
      onClick={onClick}
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: motionTokens.duration.fast }}
      layout="position"
    >
      <span className="callout-icon">{icon}</span>
      <span className="grow">
        <strong>{title}</strong>
        <span className="sub block">{text}</span>
      </span>
    </motion.button>
  );
}

function SideCallouts({ overview, conflicts, go }: { overview: Overview; conflicts: number; go: (v: View) => void }) {
  const name = (id: string) => overview.devices.find((d) => d.id === id)?.name || shortId(id);
  const pf = overview.pendingFolders;
  const pd = overview.pendingDevices;
  return (
    <AnimatePresence initial={false}>
      {conflicts > 0 && (
        <Callout key="c" tone="wait" icon={<IconAlert />} title={conflicts === 1 ? "Resolver 1 conflito" : `Resolver ${conflicts} conflitos`} text="O mesmo arquivo mudou nos dois lados. Nada foi perdido." onClick={() => go("pending")} />
      )}
      {pd.length > 0 && (
        <Callout key="d" tone="busy" icon={<IconMonitor />} title={`${pd[0].name || shortId(pd[0].deviceId)} quer se conectar`} text="Aprove só se for um computador seu." onClick={() => go("pending")} />
      )}
      {pf.length > 0 && (
        <Callout key="f" tone="busy" icon={<IconInbox />} title={`“${pf[0].label || pf[0].folderId}” espera por você`} text={`${name(pf[0].offeredBy)} quer compartilhar esta pasta.${pf.length > 1 ? ` E mais ${pf.length - 1}.` : ""}`} onClick={() => go("pending")} />
      )}
    </AnimatePresence>
  );
}

/* ---------- views ---------- */

export function OverviewView({ overview, profile, conflicts, actions }: { overview: Overview; profile: Profile | null; conflicts: FolderConflicts[]; actions: Actions }) {
  const n = totalConflicts(conflicts);
  const h = hero(overview, n);
  return (
    <div className="view">
      <Hero title={h.title} sub={h.sub} action={overview.devices.some((d) => d.connected || d.lastSeen) ? <AddButton onClick={actions.add} /> : undefined} />
      <ConnectGuide overview={overview} onChanged={actions.refresh} onAddFolder={actions.add} go={actions.go} />
      {overview.devices.length > 0 && <LinkCard overview={overview} profile={profile} />}
      {overview.folders.length > 0 && <div className="split">
        <FoldersTable folders={overview.folders} actions={actions} limit={6} compact />
        <div className="side">
          <TransferCard overview={overview} />
          <SideCallouts overview={overview} conflicts={n} go={actions.go} />
        </div>
      </div>}
    </div>
  );
}

export function FoldersView({ overview, actions }: { overview: Overview; actions: Actions }) {
  return (
    <div className="view">
      <Hero title="Suas pastas" sub="Tudo o que fica igual nos dois computadores. Clique numa pasta para abri-la." action={overview.devices.length > 0 ? <AddButton onClick={actions.add} /> : undefined} />
      <FoldersTable folders={overview.folders} actions={actions} title="Todas as pastas" />
    </div>
  );
}

export function ComputersView({ overview, profile, actions }: { overview: Overview; profile: Profile | null; actions: Actions }) {
  const stage = pairingStage(overview);
  const guided = stage !== "done" && stage !== "offline";
  return (
    <div className="view">
      <Hero title="Seus computadores" sub="Os aparelhos que fazem parte do seu espaço. Para conectar um novo, cole o ID dele aqui." />
      {overview.devices.length > 0 && <LinkCard overview={overview} profile={profile} />}
      {guided ? (
        <ConnectGuide overview={overview} onChanged={actions.refresh} onAddFolder={actions.add} go={actions.go} />
      ) : (
        <section className="panel add-panel">
          <div className="panel-head">
            <div>
              <h2>Conectar outro computador</h2>
              <p className="sub">Cole o ID do computador novo. Ele recebe um pedido e você aprova lá.</p>
            </div>
          </div>
          <div className="add-grid">
            <AddDeviceForm onDone={actions.refresh} />
            <div className="my-id">
              <p className="field-label">ID deste computador</p>
              <IdBlocks id={overview.myId} />
              <p className="sub">Para conectar pelo outro lado, cole este ID lá. Quem receber o pedido confere o primeiro bloco ({overview.myId.split("-")[0]}) e aprova.</p>
            </div>
          </div>
        </section>
      )}
      {overview.devices.length > 0 && <RemotePanel />}
    </div>
  );
}

export function PendingView({ overview, conflicts, actions }: { overview: Overview; conflicts: FolderConflicts[]; actions: Actions }) {
  const n = totalConflicts(conflicts);
  const nothing = n === 0 && overview.pendingDevices.length === 0 && overview.pendingFolders.length === 0;
  return (
    <div className="view">
      <Hero title={nothing ? "Nada esperando por você." : "O que precisa de você"} sub="Pedidos de pareamento, pastas oferecidas e conflitos. Nada é aceito sem a sua escolha." />
      <Pending overview={overview} onChange={actions.refresh} />
      <Conflicts all={conflicts} overview={overview} onChange={actions.refreshConflicts} />
      {nothing && <Empty icon={<IconInbox size={28} />} title="Tudo em dia" text="Quando o outro computador pedir algo, aparece aqui e numa notificação." />}
    </div>
  );
}
