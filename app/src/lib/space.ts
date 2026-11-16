import type { FolderView, Overview } from "../types";

export interface Hero {
  title: string;
  sub: string;
}

const NUM = ["", "Um", "Dois", "Três", "Quatro", "Cinco"];

/** Headline for the overview, from the real state of the space. */
export function hero(o: Overview, conflicts: number): Hero {
  if (o.devices.length === 0) {
    return {
      title: "Vamos conectar seus computadores.",
      sub: "Siga os passos abaixo. Leva um minuto e você só faz uma vez.",
    };
  }
  if (o.folders.some((f) => f.health.kind === "error")) {
    return { title: "Algo precisa de atenção.", sub: "Uma ou mais pastas pararam com erro. Veja os detalhes abaixo." };
  }
  if (conflicts > 0) {
    const n = conflicts <= 5 ? NUM[conflicts] : String(conflicts);
    return {
      title: conflicts === 1 ? "Um arquivo precisa de você." : `${n} arquivos precisam de você.`,
      sub: "Foram alterados nos dois computadores. Nada foi perdido: escolha o que manter.",
    };
  }
  if (!o.devices.some((d) => d.connected || d.lastSeen)) {
    return {
      title: "Falta aprovar no outro computador.",
      sub: "Abra o TwinFolders lá e aprove o pedido em Pendências. Esta tela muda sozinha quando eles se conectarem.",
    };
  }
  if (!o.devices.some((d) => d.connected && !d.paused)) {
    return {
      title: "Seu espaço espera o outro computador.",
      sub: "Assim que os dois estiverem ligados, tudo sincroniza sozinho.",
    };
  }
  return {
    title: "Seu espaço, em sintonia.",
    sub: "Acompanhe o que conecta seus computadores.",
  };
}

export interface Totals {
  folders: number;
  bytes: number;
  down: number;
  up: number;
  /** Name of the peer sending the most right now ("" when idle). */
  downFrom: string;
  upTo: string;
}

export function totals(o: Overview): Totals {
  const live = o.devices.filter((d) => d.connected && !d.paused);
  const top = (k: "inRate" | "outRate") => {
    const d = [...live].sort((a, b) => b[k] - a[k])[0];
    return d && d[k] >= 1 ? d.name || d.id.split("-")[0] : "";
  };
  return {
    folders: o.folders.length,
    bytes: o.folders.reduce((n, f) => n + f.globalBytes, 0),
    down: live.reduce((n, d) => n + d.inRate, 0),
    up: live.reduce((n, d) => n + d.outRate, 0),
    downFrom: top("inRate"),
    upTo: top("outRate"),
  };
}

export interface Transfer {
  folder: FolderView;
  percent: number;
  rate: number;
  /** Rough seconds left at the current rate; null when idle. */
  etaSeconds: number | null;
}

/** The syncing folder with the most left to fetch (real data, no file names). */
export function activeTransfer(o: Overview): Transfer | null {
  const syncing = o.folders.filter((f) => f.health.kind === "syncing");
  if (syncing.length === 0) return null;
  const f = [...syncing].sort((a, b) => b.needBytes - a.needBytes)[0];
  const rate = o.devices.filter((d) => d.connected).reduce((n, d) => n + d.inRate, 0);
  return {
    folder: f,
    percent: f.health.kind === "syncing" ? f.health.percent : 0,
    rate,
    etaSeconds: rate >= 1 ? Math.ceil(f.needBytes / rate) : null,
  };
}

export type FolderTab = "all" | "syncing" | "upToDate";

const fold = (s: string) => s.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();

export function filterFolders(list: FolderView[], tab: FolderTab, query: string): FolderView[] {
  const q = fold(query.trim());
  return list.filter((f) => {
    const k = f.health.kind;
    if (tab === "syncing" && k !== "syncing" && k !== "scanning") return false;
    if (tab === "upToDate" && k !== "upToDate") return false;
    return !q || fold(f.label).includes(q) || fold(f.path).includes(q);
  });
}

export function displayName(user: string): string {
  const parts = user.split(/[._\-\s]+/).filter(Boolean);
  if (parts.length === 0) return "Você";
  return parts.map((p) => p[0].toUpperCase() + p.slice(1)).join(" ");
}

export function initial(user: string): string {
  return user.trim() ? user.trim()[0].toUpperCase() : "?";
}

export function osLabel(os: string): string {
  return os === "macos" ? "macOS" : os === "windows" ? "Windows" : os ? os[0].toUpperCase() + os.slice(1) : "";
}

export function formatEta(s: number | null): string {
  if (s === null) return "";
  if (s < 60) return `~${s} s restantes`;
  if (s < 3600) return `~${Math.round(s / 60)} min restantes`;
  return `~${Math.round(s / 3600)} h restantes`;
}

/** Human name for how a peer is reached, from its connection address. */
export function connectionKind(address: string): string {
  const host = address.replace(/^[a-z]+:\/\//, "").replace(/:\d+$/, "").replace(/^\[|\]$/g, "");
  if (!host) return "";
  if (host.toLowerCase().startsWith("fd7a:115c:a1e0")) return "Tailscale"; // Tailscale IPv6 ULA
  const p = host.split(".").map(Number);
  if (p.length !== 4 || p.some((n) => !Number.isInteger(n))) return "Internet";
  const [a, b] = p;
  if (a === 127) return "Este computador";
  if (a === 100 && b >= 64 && b <= 127) return "Tailscale"; // CGNAT range used by Tailscale
  if (a === 10 || (a === 192 && b === 168) || (a === 172 && b >= 16 && b <= 31) || (a === 169 && b === 254)) return "Rede local";
  return "Internet";
}

/** "4 pastas, 24,3 GB no total" — context for the folder table. */
export function folderSummary(count: number, bytes: string): string {
  if (count === 0) return "Nenhuma pasta ainda";
  return `${count} ${count === 1 ? "pasta" : "pastas"}, ${bytes} no total`;
}

/** Packet travel time on the connection beam: faster when more flows. */
export function beamSeconds(rate: number): number {
  const r = Math.max(rate, 1);
  return Math.round(Math.min(3.2, Math.max(0.8, 3.6 - Math.log10(r) * 0.4)) * 100) / 100;
}
