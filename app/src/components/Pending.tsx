"use client";
import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { Overview } from "../types";
import { shortId } from "../lib/format";
import { AnimatePresence, motion } from "motion/react";
import { motionTokens, springs } from "../lib/motion-tokens";
import { useSafeMotion } from "../hooks/useSafeMotion";
import { useToast } from "./Toasts";

/** Pairing requests and folder offers: nothing is accepted automatically. */
export function Pending({ overview, onChange }: { overview: Overview; onChange: () => void }) {
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const toast = useToast();
  const m = useSafeMotion(motionTokens.distance.sm);
  const { pendingDevices, pendingFolders } = overview;
  const empty = pendingDevices.length === 0 && pendingFolders.length === 0;
  const item = {
    initial: m.initial,
    animate: m.animate,
    exit: { opacity: 0, x: m.reduce ? 0 : motionTokens.distance.lg },
    transition: m.reduce ? m.fade : springs.snappy,
  };

  const name = (id: string) => overview.devices.find((d) => d.id === id)?.name || shortId(id);

  const approve = async (deviceId: string, devName: string) => {
    setBusy(deviceId);
    setError(null);
    try {
      await invoke("approve_device", { deviceId, name: devName || shortId(deviceId) });
      toast("ok", `${devName || shortId(deviceId)} aprovado. Agora vocês podem compartilhar pastas.`);
      onChange();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  const accept = async (folderId: string, offeredBy: string, label: string) => {
    const path = await open({ directory: true, multiple: false, title: `Onde guardar "${label || folderId}" neste computador?` });
    if (typeof path !== "string") return;
    setBusy(folderId);
    setError(null);
    try {
      await invoke("accept_folder", { folderId, offeredBy, label, path });
      toast("ok", `"${label || folderId}" aceita. A sincronização começou.`);
      onChange();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  return (
    <AnimatePresence initial={false}>
      {!empty && (
    <motion.section key="pending" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} transition={m.fade}>
      <h2>Pedidos pendentes <span className="count count-wait">{pendingDevices.length + pendingFolders.length}</span></h2>
      <div className="list">
        <AnimatePresence mode="popLayout" initial={false}>
        {pendingDevices.map((d) => (
          <motion.article key={d.deviceId} className="card row pending" {...item}>
            <div className="grow">
              <strong>{d.name || "Computador desconhecido"}</strong> quer se conectar
              <p className="muted small mono">ID {d.deviceId}{d.address ? ` · ${d.address}` : ""}</p>
              <p className="muted small">Só aprove se for o seu outro computador (confira o ID nele).</p>
            </div>
            <motion.button className="btn btn-primary" disabled={busy === d.deviceId} onClick={() => approve(d.deviceId, d.name)} whileTap={m.press}>Aprovar</motion.button>
          </motion.article>
        ))}
        {pendingFolders.map((f) => (
          <motion.article key={`${f.folderId}-${f.offeredBy}`} className="card row pending" {...item}>
            <div className="grow">
              <strong>{name(f.offeredBy)}</strong> quer compartilhar a pasta <strong>{f.label || f.folderId}</strong>
            </div>
            <motion.button className="btn btn-primary" disabled={busy === f.folderId} onClick={() => accept(f.folderId, f.offeredBy, f.label)} whileTap={m.press}>Aceitar…</motion.button>
          </motion.article>
        ))}
        </AnimatePresence>
      </div>
      {error && <p className="error">{error}</p>}
    </motion.section>
      )}
    </AnimatePresence>
  );
}
