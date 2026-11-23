"use client";
import { createContext, useContext, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { AnimatePresence, motion } from "motion/react";
import type { DeviceView, FolderView, Overview, Role } from "../types";
import { relativeTime, shortId } from "../lib/format";
import { connectionKind } from "../lib/space";
import { ROLE_LABEL, deviceName, foldersByDevice, teamLine, validSelection } from "../lib/team";
import { staggerContainer, useSafeMotion } from "../hooks/useSafeMotion";
import { springs } from "../lib/motion-tokens";
import { Modal } from "./Modal";
import { useToast } from "./Toasts";
import { Dot, IconCopy, IconLaptop, IconMonitor, SwapText } from "./ui";
import { ActionMenu, type MenuItem } from "./Menu";

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

/** Every paired computer with its state and folders. Actions (terminal,
 * disconnect) live in each card's ⋯ menu, destructive last. */
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
  const terminal = async (d: DeviceView) => {
    try {
      await invoke("open_terminal", { deviceId: d.id });
    } catch (e) {
      toast("error", String(e));
    }
  };
  return (
    <section className="panel devices" aria-label={role === "hub" ? "Computadores do time" : "Computadores conectados"}>
      <motion.div className="device-grid" variants={staggerContainer} initial="hidden" animate="visible">
        <AnimatePresence mode="popLayout" initial={false}>
          {overview.devices.map((d) => {
            const s = state(d);
            const folders = byDevice.get(d.id) ?? [];
            const items: MenuItem[] = [{ label: "Abrir terminal", onSelect: () => terminal(d) }];
            if (role !== "member") items.push("separator", { label: "Desconectar", onSelect: () => remove(d), danger: true });
            return (
              <motion.article
                key={d.id}
                layout
                className="device-card"
                variants={{ hidden: m.initial, visible: { ...m.animate, transition: m.reduce ? m.fade : springs.snappy } }}
                exit={{ opacity: 0, scale: m.reduce ? 1 : 0.98, transition: m.fade }}
              >
                <div className="device-top">
                  <span className="machine-icon small-icon">{/mac|book/i.test(d.name) ? <IconLaptop size={20} /> : <IconMonitor size={20} />}</span>
                  <span className="grow">
                    <strong className="ellipsis block" title={d.id}>{deviceName(d, d.id)}</strong>
                    <span className={`small state-${s.tone}`}><Dot tone={s.tone} /> {s.text}</span>
                  </span>
                  <ActionMenu label={`Ações para ${deviceName(d, d.id)}`} items={items} />
                </div>
                <p className="sub device-folders">
                  {folders.length === 0 ? "Nenhuma pasta compartilhada" : folders.length === 1 ? `Pasta: ${folders[0].label}` : `${folders.length} pastas: ${folders.slice(0, 3).map((f) => f.label).join(", ")}${folders.length > 3 ? "…" : ""}`}
                </p>
                <span className="sub small">ID {shortId(d.id)}</span>
              </motion.article>
            );
          })}
        </AnimatePresence>
      </motion.div>
    </section>
  );
}

/** Overview: one line about the computers, no actions besides going to manage them. */
export function TeamStrip({ devices, onManage }: { devices: DeviceView[]; onManage: () => void }) {
  const role = useRole();
  const line = teamLine(devices);
  return (
    <section className="panel team-strip" aria-label="Computadores">
      <strong>{role === "hub" ? "Computadores do time" : "Computadores"}</strong>
      <span className="sub"><SwapText swapKey={line}>{line}</SwapText></span>
      <ul className="team-chips">
        {devices.map((d) => {
          const s = state(d);
          return (
            <li key={d.id} className="team-chip" title={s.text}>
              <Dot tone={s.tone} /> {deviceName(d, d.id)}
              {s.tone !== "ok" && <span className="sub"> · {s.text.toLowerCase()}</span>}
            </li>
          );
        })}
      </ul>
      <button className="link-btn" onClick={onManage}>Gerenciar →</button>
    </section>
  );
}

/** This computer's ID, shown once (Computers header) with copy. */
export function IdChip({ id }: { id: string }) {
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(id);
      setCopied(true);
      setTimeout(() => setCopied(false), 1600);
    } catch {
      // The full ID is in the tooltip.
    }
  };
  return (
    <button className="id-chip" onClick={copy} title={`ID completo: ${id}`}>
      <span className="sub small">Meu ID</span>
      <strong>{id.split("-")[0]}</strong>
      <SwapText swapKey={copied ? "ok" : "copy"} className="id-chip-icon">{copied ? "✓" : <IconCopy />}</SwapText>
    </button>
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
