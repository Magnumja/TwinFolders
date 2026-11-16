import type { FolderConflicts, VersioningChoice } from "../types";

const pad = (n: number, w = 2) => String(n).padStart(w, "0");

/** Local time as Syncthing's version tag `YYYYMMDD-HHMMSS`. */
export function versionTag(d: Date = new Date()): string {
  return `${d.getFullYear()}${pad(d.getMonth() + 1)}${pad(d.getDate())}-${pad(d.getHours())}${pad(d.getMinutes())}${pad(d.getSeconds())}`;
}

/** Who made the conflicting change, from the short ID in the file name. */
export function authorOf(short: string, myId: string, devices: { id: string; name: string }[]): string {
  if (myId.startsWith(short)) return "este computador";
  const d = devices.find((x) => x.id.startsWith(short));
  return d?.name ? d.name : `dispositivo ${short}`;
}

export function baseName(path: string): string {
  return path.slice(path.lastIndexOf("/") + 1);
}

const dias = (n: number) => (n === 1 ? "1 dia" : `${n} dias`);

export function describeChoice(c: VersioningChoice | null): string {
  if (!c) return "Personalizado (configurado fora do TwinFolders)";
  switch (c.kind) {
    case "off":
      return "Desligado — sem histórico de versões";
    case "staggered":
      return c.days === 0 ? "Histórico escalonado para sempre" : `Histórico escalonado por ${dias(c.days)}`;
    case "trashcan":
      return c.days === 0 ? "Lixeira sem limpeza automática" : `Lixeira por ${dias(c.days)}`;
    case "simple": {
      const base = c.keep === 1 ? "Última 1 versão" : `Últimas ${c.keep} versões`;
      return c.days === 0 ? base : `${base}, por até ${dias(c.days)}`;
    }
  }
}

export function totalConflicts(all: FolderConflicts[]): number {
  return all.reduce((n, f) => n + f.conflicts.length, 0);
}
