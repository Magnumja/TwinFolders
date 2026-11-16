import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { DeviceView, PreflightReport } from "../types";
import { warningsFor } from "../lib/preflight";
import { shortId } from "../lib/format";
import { Modal } from "./Modal";

function baseName(p: string): string {
  return p.split(/[\\/]/).filter(Boolean).pop() ?? p;
}

export function AddFolder({ devices, onClose, onDone }: { devices: DeviceView[]; onClose: () => void; onDone: () => void }) {
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
      onDone();
    } catch (e) {
      setError(String(e));
      setBusy(null);
    }
  };

  return (
    <Modal title="Adicionar pasta" onClose={onClose}>
      {devices.length === 0 ? (
        <p>Pareie o outro computador primeiro: copie o ID deste computador (no topo) e adicione no outro.</p>
      ) : (
        <div className="form">
          <div className="field">
            <span className="field-label">Pasta neste computador</span>
            <div className="row">
              <code className="grow path-box">{path ?? "nenhuma escolhida"}</code>
              <button className="btn" onClick={pick} disabled={!!busy}>Escolher…</button>
            </div>
          </div>

          {busy && <p className="muted">{busy}</p>}

          {report && warnings.length === 0 && (
            <p className="ok-text">✓ Nenhum problema encontrado ({report.entriesScanned.toLocaleString("pt-BR")} itens verificados).</p>
          )}
          {warnings.map((w) => (
            <div key={w.title} className={`notice ${w.blocking ? "notice-error" : "notice-wait"}`}>
              <strong>{w.title}</strong>
              <p>{w.text}</p>
              {w.items.length > 0 && (
                <ul>{w.items.slice(0, 8).map((i) => <li key={i} className="mono small">{i}</li>)}</ul>
              )}
            </div>
          ))}
          {blocking && (
            <label className="toggle">
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

          {error && <p className="error">{error}</p>}
          <div className="actions">
            <button className="btn" onClick={onClose}>Cancelar</button>
            <button className="btn btn-primary" onClick={share} disabled={!canShare}>Compartilhar</button>
          </div>
        </div>
      )}
    </Modal>
  );
}
