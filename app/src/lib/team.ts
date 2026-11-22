import type { DeviceView, FolderView, Overview, Role } from "../types";
import { shortId } from "./format";

export const ROLE_LABEL: Record<Role, { title: string; text: string }> = {
  personal: { title: "Uso pessoal", text: "Seus próprios computadores, por exemplo o Mac e o PC de casa." },
  hub: { title: "Central do escritório", text: "Este computador acompanha e organiza os computadores do time." },
  member: { title: "Computador do time", text: "Este computador participa do espaço da central e só fala com ela." },
};

/** "o outro computador" with one peer, "os outros computadores" with more. */
export function others(n: number, role: Role): string {
  if (role === "hub") return n === 1 ? "o computador do time" : "os computadores do time";
  if (role === "member") return "a central";
  return n > 1 ? "os outros computadores" : "o outro computador";
}

export function deviceName(d: DeviceView | undefined, id: string): string {
  return d?.name || shortId(id);
}

/** Folders shared with each paired computer, by device ID. */
export function foldersByDevice(o: Overview): Map<string, FolderView[]> {
  const m = new Map<string, FolderView[]>(o.devices.map((d) => [d.id, []]));
  for (const f of o.folders) for (const p of f.peers) m.get(p.deviceId)?.push(f);
  return m;
}

/** Names of the computers a folder goes to, for the folder row. */
export function sharedWith(f: FolderView, devices: DeviceView[]): string {
  const names = f.peers.map((p) => deviceName(devices.find((d) => d.id === p.deviceId), p.deviceId));
  if (names.length === 0) return "Só neste computador";
  if (names.length <= 2) return `Com ${names.join(" e ")}`;
  return `Com ${names.slice(0, 2).join(", ")} e mais ${names.length - 2}`;
}

/** One line about the whole group of computers. */
export function teamLine(devices: DeviceView[]): string {
  const n = devices.length;
  const on = devices.filter((d) => d.connected && !d.paused).length;
  if (n === 0) return "Nenhum computador conectado ainda.";
  if (on === n) return n === 1 ? "Conectado agora." : `Os ${n} estão conectados agora.`;
  if (on === 0) return n === 1 ? "Desligado ou sem rede agora." : `Nenhum dos ${n} está conectado agora.`;
  return `${on} de ${n} conectados agora.`;
}

/** The selection sent when sharing: at least one paired computer, no duplicates. */
export function validSelection(selected: string[], devices: DeviceView[]): boolean {
  return selected.length > 0 && new Set(selected).size === selected.length && selected.every((id) => devices.some((d) => d.id === id));
}
