import type { ActivityEntry, DeviceView } from "../types";

export type Period = "today" | "week" | "month" | "all";
export const PERIODS: { id: Period; label: string }[] = [
  { id: "today", label: "Hoje" },
  { id: "week", label: "7 dias" },
  { id: "month", label: "30 dias" },
  { id: "all", label: "Tudo" },
];

/** "local" or a device ID; matches an entry's short `by`. */
export interface ActivityFilter {
  who: "all" | "local" | string;
  folder: "all" | string;
  period: Period;
  query: string;
}

export function whoName(e: ActivityEntry, devices: DeviceView[], myName: string): string {
  if (!e.by) return myName || "Este computador";
  return devices.find((d) => d.id.startsWith(e.by!))?.name || e.by;
}

/** Syncthing reports a new file as "modified" too, so both read as a change. */
export function verb(e: ActivityEntry): string {
  if (e.action === "deleted") return "apagou";
  if (e.kind === "dir" && e.action === "added") return "criou a pasta";
  return "alterou";
}

function since(period: Period, now: Date): number {
  if (period === "all") return 0;
  if (period === "today") return new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  return now.getTime() - (period === "week" ? 7 : 30) * 86_400_000;
}

export function filterActivity(list: ActivityEntry[], f: ActivityFilter, now = new Date()): ActivityEntry[] {
  const from = since(f.period, now);
  const q = f.query.trim().toLowerCase();
  return list.filter((e) => {
    if (f.who === "local" ? !!e.by : f.who !== "all" && !(e.by && f.who.startsWith(e.by))) return false;
    if (f.folder !== "all" && e.folderId !== f.folder) return false;
    if (from && new Date(e.time).getTime() < from) return false;
    return !q || e.path.toLowerCase().includes(q) || e.folderLabel.toLowerCase().includes(q);
  });
}

/** Groups (already newest-first) entries into Hoje / Ontem / dd de mês. */
export function groupByDay(list: ActivityEntry[], now = new Date()): { label: string; items: ActivityEntry[] }[] {
  const day = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const today = day(now);
  const groups: { label: string; items: ActivityEntry[] }[] = [];
  for (const e of list) {
    const d = day(new Date(e.time));
    const label =
      d === today ? "Hoje" : d === today - 86_400_000 ? "Ontem" : new Date(d).toLocaleDateString("pt-BR", { day: "numeric", month: "long" });
    const last = groups[groups.length - 1];
    if (last && last.label === label) last.items.push(e);
    else groups.push({ label, items: [e] });
  }
  return groups;
}

/** Latest change per computer ("local" for this one). */
export function lastByWho(list: ActivityEntry[]): Map<string, ActivityEntry> {
  const m = new Map<string, ActivityEntry>();
  for (const e of list) {
    const k = e.by ?? "local";
    if (!m.has(k)) m.set(k, e);
  }
  return m;
}
