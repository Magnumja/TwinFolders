"use client";
import { createContext, useContext, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { motion } from "motion/react";
import type { DeviceView, FolderView, Overview, Role } from "../types";
import { formatRate, relativeTime, shortId } from "../lib/format";
import { connectionKind } from "../lib/space";
import { ROLE_LABEL, deviceName, foldersByDevice, teamLine, validSelection } from "../lib/team";
import { staggerContainer, useSafeMotion } from "../hooks/useSafeMotion";
import { springs } from "../lib/motion-tokens";
import { Modal } from "./Modal";
import { useToast } from "./Toasts";
import { Dot, IconLaptop, IconMonitor } from "./ui";

/** This computer's role, loaded once by the shell. */
export const RoleContext = createContext<Role>("personal");
export const useRole = () => useContext(RoleContext);

function state(d: DeviceView): { tone: "ok" | "wait" | "muted"; text: string } {
  if (d.paused) return { tone: "muted", text: "Pausado" };
  if (d.connected) return { tone: "ok", text: `Online por ${connectionKind(d.address) || "conexão direta"}` };
  if (!d.lastSeen) return { tone: "muted", text: "Aguardando aprovação" };
  return { tone: "wait", text: `Visto ${relativeTime(d.lastSeen)}` };
}

/** Pick computers; used when sharing a new folder and when editing one. */
export function DevicePicker({ devices, selected, onChange }: { devices: DeviceView[]; selected: string[]; onChange: (ids: string[]) => void }) {
  const toggle = (id: string) => onChange(selected.includes(id) ? selected.filter((x) => x !== id) : [...selected, id]);
  return (
    <div className="picker" role="group" aria-label="Computadores">
      {devices.map((d) => {
        const s = state(d);
        return (
          <label key={d.id} className={selected.includes(d.id) ? "pick on" : "pick"}>
            <input type="checkbox" checked={selected.includes(d.id)} onChange={() => toggle(d.id)} />
            <span className="grow">
              <strong>{deviceName(d, d.id)}</strong>
              <span className={`small block state-${s.tone}`}>{s.text}</span>
            </span>
          </label>
        );
      })}
      {devices.length > 1 && (
        <button type="button" className="inline-link" onClick={() => onChange(selected.length === devices.length ? [] : devices.map((d) => d.id))}>
          {selected.length === devices.length ? "Desmarcar todos" : "Marcar todos"}
        </button>
      )}
    </div>
  );
}

/** "Compartilhar com…" for an existing folder. */
export function ShareEditor({ folder, devices, onClose, onSaved }: { folder: FolderView; devices: DeviceView[]; onClose: () => void; onSaved: () => void }) {
  const [selected, setSelected] = useState(folder.peers.map((p) => p.deviceId).filter((id) => devices.some((d) => d.id === id)));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const toast = useToast();
  const before = new Set(folder.peers.map((p) => p.deviceId));
  const removed = [...before].filter((id) => !selected.includes(id));
  const added = selected.filter((id) => !before.has(id));
  const save = async () => {
    setBusy(true);
    setError(null);
    try {
      await invoke("set_folder_peers", { folderId: folder.id, peerIds: selected });
      toast("ok", added.length ? `"${folder.label}" oferecida a ${added.length === 1 ? "mais um computador" : `mais ${added.length} computadores`}. Aceite lá.` : `"${folder.label}" atualizada.`);
      onSaved();
      onClose();
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  };
  return (
    <Modal title={`Compartilhar “${folder.label}”`} onClose={onClose}>
      <div className="form">
        <p className="modal-intro">Escolha quais computadores recebem esta pasta. Quem entrar recebe um pedido para aceitar; quem sair para de receber as mudanças, mas mantém os arquivos que já tem.</p>
        <DevicePicker devices={devices} selected={selected} onChange={setSelected} />
        {removed.length > 0 && (
          <p className="notice notice-wait">
            {removed.map((id) => deviceName(devices.find((d) => d.id === id), id)).join(", ")} {removed.length === 1 ? "vai parar" : "vão parar"} de receber esta pasta. Os arquivos que já estão lá continuam lá.
          </p>
        )}
        {error && <p className="error">{error}</p>}
        <div className="actions">
          <button className="btn" onClick={onClose}>Cancelar</button>
          <button className="btn btn-primary" onClick={save} disabled={busy || !validSelection(selected, devices) || (added.length === 0 && removed.length === 0)}>
            {busy ? "Salvando…" : "Salvar"}
          </button>
        </div>
      </div>
    </Modal>
  );
}

/** Every paired computer with its state and folders; disconnect from here. */
export function DeviceGrid({ overview, onChanged }: { overview: Overview; onChanged: () => void }) {
  const role = useRole();
  const toast = useToast();
  const m = useSafeMotion(8);
  const byDevice = foldersByDevice(overview);
  const remove = async (d: DeviceView) => {
    const name = deviceName(d, d.id);
    const ok = await ask(
      `Desconectar ${name}?\n\nEle para de receber e de enviar mudanças para este computador, e as pastas deixam de ser compartilhadas com ele. Os arquivos que já estão nele continuam lá.\n\nSe o computador foi perdido ou roubado, tire-o também da sua rede do Tailscale.`,
      { title: "Desconectar computador", kind: "warning", okLabel: "Desconectar", cancelLabel: "Cancelar" },
    );
    if (!ok) return;
    try {
      await invoke("remove_device", { deviceId: d.id });
      toast("ok", `${name} foi desconectado.`);
    } catch (e) {
      toast("error", `Não foi possível desconectar: ${e}`);
    } finally {
      onChanged();
    }
  };
  return (
    <section className="panel devices">
      <div className="panel-head">
        <div>
          <h2>{role === "hub" ? "Computadores do time" : "Computadores conectados"}</h2>
          <p className="sub">{teamLine(overview.devices)}</p>
        </div>
      </div>
      <motion.div className="device-grid" variants={staggerContainer} initial="hidden" animate="visible">
        {overview.devices.map((d) => {
          const s = state(d);
          const folders = byDevice.get(d.id) ?? [];
          const rate = d.inRate + d.outRate;
          return (
            <motion.article key={d.id} className="device-card" variants={{ hidden: m.initial, visible: { ...m.animate, transition: m.reduce ? m.fade : springs.snappy } }}>
              <div className="device-top">
                <span className="machine-icon small-icon">{/mac|book/i.test(d.name) ? <IconLaptop size={20} /> : <IconMonitor size={20} />}</span>
                <span className="grow">
                  <strong className="ellipsis block" title={d.id}>{deviceName(d, d.id)}</strong>
                  <span className={`small state-${s.tone}`}><Dot tone={s.tone} /> {s.text}</span>
                </span>
              </div>
              <p className="sub device-folders">
                {folders.length === 0 ? "Nenhuma pasta compartilhada" : folders.length === 1 ? `Pasta: ${folders[0].label}` : `${folders.length} pastas: ${folders.slice(0, 3).map((f) => f.label).join(", ")}${folders.length > 3 ? "…" : ""}`}
              </p>
              <div className="device-foot">
                <span className="sub small">{rate >= 1 ? `Trocando ${formatRate(rate)}` : `ID ${shortId(d.id)}`}</span>
                {role !== "member" && (
                  <button className="link-btn danger" onClick={() => remove(d)}>Desconectar</button>
                )}
              </div>
            </motion.article>
          );
        })}
      </motion.div>
    </section>
  );
}

/** Preferences: how this computer is used. */
export function RolePicker({ role, onChange }: { role: Role; onChange: (r: Role) => void }) {
  return (
    <fieldset className="role-picker">
      <legend><strong>Como você usa este computador</strong></legend>
      {(Object.keys(ROLE_LABEL) as Role[]).map((r) => (
        <label key={r} className={r === role ? "pick on" : "pick"}>
          <input type="radio" name="role" checked={r === role} onChange={() => onChange(r)} />
          <span className="grow">
            <strong>{ROLE_LABEL[r].title}</strong>
            <span className="muted small block">{ROLE_LABEL[r].text}</span>
          </span>
        </label>
      ))}
    </fieldset>
  );
}
