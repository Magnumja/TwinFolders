"use client";
import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { AnimatePresence, motion } from "motion/react";
import type { ActivityEntry, AuditEntry, Overview, SecurityCheck } from "../types";
import { PERIODS, filterActivity, groupByDay, lastByWho, verb, whoName, type ActivityFilter } from "../lib/activity";
import { relativeTime } from "../lib/format";
import { motionTokens, springs } from "../lib/motion-tokens";
import { useSafeMotion } from "../hooks/useSafeMotion";
import { Empty, IconPulse } from "./ui";

const LIMIT = 2000;

function time(iso: string) {
  return new Date(iso).toLocaleTimeString("pt-BR", { hour: "2-digit", minute: "2-digit" });
}

/** Changes in the synced folders, here and on the other computers. */
function FileActivity({ overview }: { overview: Overview }) {
  const [list, setList] = useState<ActivityEntry[] | null>(null);
  const [f, setF] = useState<ActivityFilter>({ who: "all", folder: "all", period: "week", query: "" });
  const m = useSafeMotion(motionTokens.distance.xs);
  const load = useCallback(async () => {
    try {
      setList(await invoke<ActivityEntry[]>("get_activity", { limit: LIMIT }));
    } catch {
      setList([]);
    }
  }, []);
  useEffect(() => {
    load();
    const un = listen("activity-changed", load).catch(() => null);
    return () => {
      un.then((u) => u?.());
    };
  }, [load]);

  const shown = useMemo(() => (list ? filterActivity(list, f) : []), [list, f]);
  const groups = useMemo(() => groupByDay(shown.slice(0, 400)), [shown]);
  const last = useMemo(() => lastByWho(list ?? []), [list]);
  const me = overview.myName || "Este computador";
  const who = [{ id: "local", name: me }, ...overview.devices.map((d) => ({ id: d.id, name: d.name || d.id.slice(0, 7) }))];

  return (
    <>
      <section className="panel activity-people" aria-label="Última mudança por computador">
        {who.map((w) => {
          const e = last.get(w.id === "local" ? "local" : w.id.slice(0, 7));
          const on = f.who === w.id;
          return (
            <button key={w.id} className={on ? "person on" : "person"} aria-pressed={on} onClick={() => setF({ ...f, who: on ? "all" : w.id })}>
              <strong className="ellipsis">{w.name}</strong>
              <span className="sub small">{e ? `mudou algo ${relativeTime(e.time)}` : "nenhuma mudança registrada"}</span>
            </button>
          );
        })}
      </section>

      <section className="panel activity" aria-label="Mudanças nos arquivos">
        <div className="table-tools activity-tools">
          <div className="tabs" role="tablist" aria-label="Período">
            {PERIODS.map((p) => (
              <button key={p.id} role="tab" aria-selected={f.period === p.id} className={f.period === p.id ? "tab active" : "tab"} onClick={() => setF({ ...f, period: p.id })}>
                {f.period === p.id && <motion.span layoutId="activity-period" className="tab-bg" transition={springs.snappy} />}
                <span>{p.label}</span>
              </button>
            ))}
          </div>
          <label className="select">
            <span className="sr-only">Pasta</span>
            <select value={f.folder} onChange={(e) => setF({ ...f, folder: e.target.value })}>
              <option value="all">Todas as pastas</option>
              {overview.folders.map((x) => (
                <option key={x.id} value={x.id}>{x.label}</option>
              ))}
            </select>
          </label>
          <label className="search">
            <input value={f.query} onChange={(e) => setF({ ...f, query: e.target.value })} placeholder="Buscar arquivo" aria-label="Buscar arquivo" />
          </label>
        </div>

        {list === null ? (
          <p className="sub activity-note">Carregando…</p>
        ) : shown.length === 0 ? (
          <Empty
            icon={<IconPulse size={28} />}
            title={list.length === 0 ? "Nenhuma mudança registrada ainda" : "Nada nesse filtro"}
            text={list.length === 0 ? "Quando alguém mudar um arquivo numa pasta sincronizada, aparece aqui: quem, o quê e quando." : "Troque o período, a pasta ou o computador."}
          />
        ) : (
          <div className="activity-days">
            {groups.map((g) => (
              <div key={g.label} className="activity-day">
                <h3 className="side-title">{g.label}</h3>
                <ul className="activity-list">
                  <AnimatePresence initial={false}>
                    {g.items.map((e) => (
                      <motion.li key={`${e.time}|${e.folderId}|${e.path}|${e.by ?? ""}|${e.action}`} layout="position" initial={m.initial} animate={m.animate} transition={springs.snappy}>
                        <span className="activity-time sub">{time(e.time)}</span>
                        <span className={e.by ? "who" : "who who-me"}>{whoName(e, overview.devices, me)}</span>
                        <span className={e.action === "deleted" ? "act deleted" : "act"}>{verb(e)}</span>
                        <span className="activity-path ellipsis" title={`${e.folderLabel}/${e.path}`}>
                          <span className="sub">{e.folderLabel} / </span>
                          {e.path}
                        </span>
                      </motion.li>
                    ))}
                  </AnimatePresence>
                </ul>
              </div>
            ))}
            {shown.length > 400 && <p className="sub activity-note">Mostrando as 400 mudanças mais recentes desse filtro.</p>}
          </div>
        )}
        <p className="sub small activity-note">
          Só aparecem mudanças dentro das pastas sincronizadas, registradas enquanto o TwinFolders está aberto neste computador. O resto de cada computador nunca é visto.
        </p>
      </section>
    </>
  );
}

/** What was done through the app on this computer. */
function ActionLog() {
  const [list, setList] = useState<AuditEntry[] | null>(null);
  useEffect(() => {
    invoke<AuditEntry[]>("get_audit").then(setList).catch(() => setList([]));
  }, []);
  if (list === null) return <p className="sub">Carregando…</p>;
  if (list.length === 0) return <Empty icon={<IconPulse size={28} />} title="Nenhuma ação registrada" text="Aprovações, compartilhamentos, convites e desconexões feitos neste computador aparecem aqui." />;
  return (
    <section className="panel" aria-label="Ações neste computador">
      <ul className="activity-list audit-list">
        {list.slice(0, 500).map((e, i) => (
          <li key={`${e.at}-${i}`}>
            <span className="activity-time sub">{new Date(e.at * 1000).toLocaleString("pt-BR", { day: "2-digit", month: "2-digit", hour: "2-digit", minute: "2-digit" })}</span>
            <strong>{e.action}</strong>
            <span className="activity-path ellipsis sub" title={e.detail}>{e.detail}</span>
          </li>
        ))}
      </ul>
    </section>
  );
}

export function ActivityView({ overview }: { overview: Overview }) {
  const [tab, setTab] = useState<"files" | "log">("files");
  const m = useSafeMotion(motionTokens.distance.xs);
  return (
    <div className="view">
      <header className="hero">
        <div className="grow">
          <h1>Atividade</h1>
          <p className="hero-sub">Quem mudou o quê, e quando, nas pastas sincronizadas. E o que foi feito pelo app neste computador.</p>
        </div>
        <div className="tabs" role="tablist" aria-label="Tipo de atividade">
          {(["files", "log"] as const).map((t) => (
            <button key={t} role="tab" aria-selected={tab === t} className={tab === t ? "tab active" : "tab"} onClick={() => setTab(t)}>
              {tab === t && <motion.span layoutId="activity-tab" className="tab-bg" transition={springs.snappy} />}
              <span>{t === "files" ? "Arquivos" : "Ações neste computador"}</span>
            </button>
          ))}
        </div>
      </header>
      <AnimatePresence mode="wait" initial={false}>
        <motion.div key={tab} className="view-stack" initial={m.initial} animate={m.animate} exit={m.exit} transition={{ duration: motionTokens.duration.fast }}>
          {tab === "files" ? <FileActivity overview={overview} /> : <ActionLog />}
        </motion.div>
      </AnimatePresence>
    </div>
  );
}

/** Preferences: this computer's security checklist. */
export function SecurityChecklist() {
  const [checks, setChecks] = useState<SecurityCheck[] | null>(null);
  useEffect(() => {
    invoke<SecurityCheck[]>("get_security").then(setChecks).catch(() => setChecks([]));
  }, []);
  return (
    <div className="security">
      <strong>Segurança deste computador</strong>
      {checks === null ? (
        <p className="muted small">Conferindo…</p>
      ) : (
        <ul className="checks">
          {checks.map((c) => (
            <li key={c.id} className={c.ok === true ? "check-ok" : c.ok === false ? "check-bad" : "check-unknown"}>
              <span className="check-mark" aria-hidden="true">{c.ok === true ? "✓" : c.ok === false ? "!" : "?"}</span>
              <span className="grow">
                <span className="block">{c.title}</span>
                <span className="muted small block">{c.detail}</span>
              </span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
