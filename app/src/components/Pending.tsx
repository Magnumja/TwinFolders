import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { Overview } from "../types";
import { shortId } from "../lib/format";

/** Pairing requests and folder offers: nothing is accepted automatically. */
export function Pending({ overview, onChange }: { overview: Overview; onChange: () => void }) {
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const { pendingDevices, pendingFolders } = overview;
  if (pendingDevices.length === 0 && pendingFolders.length === 0) return null;

  const name = (id: string) => overview.devices.find((d) => d.id === id)?.name || shortId(id);

  const approve = async (deviceId: string, devName: string) => {
    setBusy(deviceId);
    setError(null);
    try {
      await invoke("approve_device", { deviceId, name: devName || shortId(deviceId) });
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
      onChange();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  return (
    <section>
      <h2>Pedidos pendentes</h2>
      <div className="list">
        {pendingDevices.map((d) => (
          <article key={d.deviceId} className="card row pending">
            <div className="grow">
              <strong>{d.name || "Computador desconhecido"}</strong> quer se conectar
              <p className="muted small mono">ID {d.deviceId}{d.address ? ` · ${d.address}` : ""}</p>
              <p className="muted small">Só aprove se for o seu outro computador (confira o ID nele).</p>
            </div>
            <button className="btn btn-primary" disabled={busy === d.deviceId} onClick={() => approve(d.deviceId, d.name)}>Aprovar</button>
          </article>
        ))}
        {pendingFolders.map((f) => (
          <article key={`${f.folderId}-${f.offeredBy}`} className="card row pending">
            <div className="grow">
              <strong>{name(f.offeredBy)}</strong> quer compartilhar a pasta <strong>{f.label || f.folderId}</strong>
            </div>
            <button className="btn btn-primary" disabled={busy === f.folderId} onClick={() => accept(f.folderId, f.offeredBy, f.label)}>Aceitar…</button>
          </article>
        ))}
      </div>
      {error && <p className="error">{error}</p>}
    </section>
  );
}
