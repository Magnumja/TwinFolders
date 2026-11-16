"use client";
import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { DeviceView, PreflightReport } from "../types";
import { warningsFor } from "../lib/preflight";
import { shortId } from "../lib/format";
import { Modal } from "./Modal";
import { AnimatePresence, motion } from "motion/react";
import { motionTokens, springs } from "../lib/motion-tokens";
import { staggerContainer, useSafeMotion } from "../hooks/useSafeMotion";

function baseName(p: string): string {
  return p.split(/[\\/]/).filter(Boolean).pop() ?? p;
}

export function AddFolder({ devices, onClose, onDone }: { devices: DeviceView[]; onClose: () => void; onDone: (label: string) => void }) {
  const m = useSafeMotion(motionTokens.distance.sm);
  const [path, setPath] = useState<string | null>(null);
  const [report, setReport] = useState<PreflightReport | null>(null);
  const [label, setLabel] = useState("");
  const [peerId, setPeerId] = useState(devices.find((d) => d.connected)?.id ?? devices[0]?.id ?? "");
  const [extras, setExtras] = useState("");
  const [ack, setAck] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const pick = async () => {
    setError(null);
    const chosen = await open({ directory: true, multiple: false, title: "Escolha a pasta para sincronizar" });
    if (typeof chosen !== "string") return;
    setPath(chosen);
    setLabel(baseName(chosen));
    setReport(null);
    setAck(false);
    setBusy("Verificando a pasta…");
    try {
      setReport(await invoke<PreflightReport>("scan_folder", { path: chosen }));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  const warnings = report ? warningsFor(report) : [];
  const blocking = warnings.some((w) => w.blocking);
  const canShare = !!path && !!report && !!peerId && label.trim() !== "" && (!blocking || ack) && !busy;

  const share = async () => {
    setBusy("Compartilhando…");
    setError(null);
    try {
      await invoke<string>("share_folder", {
        path,
        label: label.trim(),
        peerId,
        extraIgnores: extras.split("\n").map((l) => l.trim()).filter(Boolean),
      });
      onDone(label.trim());
    } catch (e) {
      setError(String(e));
      setBusy(null);
    }
  };

  const peer = devices.find((d) => d.id === peerId);
  const peerName = peer ? peer.name || shortId(peer.id) : "";

  return (
    <Modal title="Adicionar pasta" onClose={onClose}>
      {devices.length === 0 ? (
        <div className="form">
          <p className="hero-sub">Primeiro conecte o outro computador. Em Computadores, cole o ID dele e aprove o pedido lá.</p>
          <div className="actions">
            <button className="btn" onClick={onClose}>Entendi</button>
          </div>
        </div>
      ) : (
        <div className="form">
          <p className="modal-intro">
            Escolha uma pasta deste computador. O outro recebe um pedido em Pendências e escolhe onde guardar. Nada aqui é apagado ou substituído.
          </p>
          <div className="field">
            <span className="field-label">Pasta neste computador</span>
            <div className="row">
              <span className={path ? "grow path-box" : "grow path-box empty-path"} title={path ?? undefined}>{path ?? "Nenhuma pasta escolhida"}</span>
              <button className="btn" onClick={pick} disabled={!!busy}>Escolher…</button>
            </div>
          </div>

          <AnimatePresence mode="wait" initial={false}>
            {busy && (
              <motion.p key="busy" className="muted busy-line" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} transition={m.fade}>
                <span className="spinner" aria-hidden="true" /> {busy}
              </motion.p>
            )}
          </AnimatePresence>

          {report && warnings.length === 0 && (
            <motion.p className="ok-text" initial={m.initial} animate={m.animate} transition={springs.snappy}>
              ✓ Nenhum problema encontrado ({report.entriesScanned.toLocaleString("pt-BR")} itens verificados).
            </motion.p>
          )}
          {warnings.length > 0 && (
            <motion.div className="form" variants={staggerContainer} initial="hidden" animate="visible" key={path ?? ""}>
              {warnings.map((w) => (
                <motion.div
                  key={w.title}
                  className={`notice ${w.blocking ? "notice-error" : "notice-wait"}`}
                  variants={{ hidden: m.initial, visible: { ...m.animate, transition: m.reduce ? m.fade : springs.snappy } }}
                >
                  <strong>{w.title}</strong>
                  <p>{w.text}</p>
                  {w.items.length > 0 && (
                    <ul>{w.items.slice(0, 8).map((i) => <li key={i} className="mono small">{i}</li>)}</ul>
                  )}
                </motion.div>
              ))}
            </motion.div>
          )}
          {blocking && (
            <label className="check">
              <input type="checkbox" checked={ack} onChange={(e) => setAck(e.target.checked)} />
              Entendo os problemas acima e quero compartilhar mesmo assim
            </label>
          )}

          {report && (
            <>
              <label className="field">
                <span className="field-label">Nome da pasta</span>
                <input value={label} onChange={(e) => setLabel(e.target.value)} />
              </label>
              <label className="field">
                <span className="field-label">Compartilhar com</span>
                <select value={peerId} onChange={(e) => setPeerId(e.target.value)}>
                  {devices.map((d) => (
                    <option key={d.id} value={d.id}>{d.name || shortId(d.id)}{d.connected ? "" : " (offline)"}</option>
                  ))}
                </select>
              </label>
              <label className="field">
                <span className="field-label">Ignorar também (opcional, um padrão por linha)</span>
                <textarea rows={3} placeholder={"*.psd\nCache/"} value={extras} onChange={(e) => setExtras(e.target.value)} />
                <span className="muted small">Já ignorados por padrão: node_modules, .git, builds, .DS_Store, Thumbs.db…</span>
              </label>
            </>
          )}

          <AnimatePresence initial={false}>
            {error && (
              <motion.p key="err" className="error" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} transition={m.fade}>
                {error}
              </motion.p>
            )}
          </AnimatePresence>
          <div className="actions">
            <button className="btn" onClick={onClose}>Cancelar</button>
            <motion.button className="btn btn-primary" onClick={share} disabled={!canShare} whileTap={canShare ? m.press : undefined} transition={springs.snappy}>
              {peerName ? `Compartilhar com ${peerName}` : "Compartilhar"}
            </motion.button>
          </div>
        </div>
      )}
    </Modal>
  );
}
