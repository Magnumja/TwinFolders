import type { FolderHealth, Overview } from "../types";

export type Tone = "ok" | "busy" | "wait" | "error" | "muted";

export interface HealthText {
  label: string;
  tone: Tone;
  detail?: string;
}

const pct = new Intl.NumberFormat("pt-BR", { maximumFractionDigits: 1 });

export function describeHealth(h: FolderHealth): HealthText {
  switch (h.kind) {
    case "upToDate":
      return { label: "Sincronizado", tone: "ok" };
    case "syncing":
      return { label: `Sincronizando ${pct.format(h.percent)}%`, tone: "busy" };
    case "waitingForPeer":
      return { label: "Aguardando o outro computador", tone: "wait" };
    case "scanning":
      return { label: "Verificando arquivos", tone: "busy" };
    case "paused":
      return { label: "Pausado", tone: "muted" };
    case "error":
      return { label: "Erro", tone: "error", detail: h.message };
  }
}

/** One-line summary for the header: errors > syncing > waiting > ok. */
export function overallStatus(o: Overview): HealthText {
  if (o.folders.length === 0) {
    return { label: "Nenhuma pasta sincronizada ainda", tone: "muted" };
  }
  const kinds = o.folders.map((f) => f.health.kind);
  const errors = kinds.filter((k) => k === "error").length;
  if (errors > 0) {
    return { label: `Atenção: ${errors} ${errors === 1 ? "pasta" : "pastas"} com erro`, tone: "error" };
  }
  if (kinds.some((k) => k === "syncing" || k === "scanning")) return { label: "Sincronizando…", tone: "busy" };
  if (kinds.includes("waitingForPeer")) return { label: "Aguardando o outro computador", tone: "wait" };
  if (kinds.every((k) => k === "paused")) return { label: "Tudo pausado", tone: "muted" };
  return { label: "Tudo sincronizado", tone: "ok" };
}
