import type { CloudProvider, PreflightReport } from "../types";

export interface Warning {
  title: string;
  text: string;
  items: string[];
  /** Would cause sync failures on the other OS; requires explicit consent. */
  blocking: boolean;
}

const CLOUD: Record<CloudProvider, string> = {
  iCloud: "iCloud Drive",
  oneDrive: "OneDrive",
  dropbox: "Dropbox",
  googleDrive: "Google Drive",
  other: "uma pasta de nuvem",
};

/** pt-BR warnings shown before sharing a folder (SPEC R9). */
export function warningsFor(r: PreflightReport): Warning[] {
  const out: Warning[] = [];
  if (r.invalidNamesTotal > 0) {
    const n = r.invalidNamesTotal;
    out.push({
      title: `${n} ${n === 1 ? "nome" : "nomes"} que o Windows não aceita`,
      text: "Esses itens não vão aparecer no Windows (o Syncthing mostra erro neles). Renomeie antes de compartilhar.",
      items: r.invalidNames.map((w) => `${w.path} — ${w.problem}`),
      blocking: true,
    });
  }
  if (r.caseCollisions.length > 0) {
    const n = r.caseCollisions.length;
    out.push({
      title: `${n} ${n === 1 ? "conflito" : "conflitos"} de maiúsculas/minúsculas`,
      text: "No Windows e no Mac esses nomes são o mesmo arquivo; só um deles sobrevive. Renomeie um dos dois.",
      items: r.caseCollisions.map((g) => g.join(" ↔ ")),
      blocking: true,
    });
  }
  if (r.cloud) {
    const extra = r.cloudOnlyFiles > 0 ? ` Há ${r.cloudOnlyFiles} arquivo(s) só na nuvem (não baixados) que não serão sincronizados até você baixá-los.` : "";
    out.push({
      title: `Pasta dentro do ${CLOUD[r.cloud]}`,
      text: `Duas ferramentas de sincronização na mesma pasta podem brigar entre si e duplicar arquivos. Prefira uma pasta fora da nuvem.${extra}`,
      items: [],
      blocking: false,
    });
  } else if (r.cloudOnlyFiles > 0) {
    out.push({
      title: `${r.cloudOnlyFiles} arquivo(s) só na nuvem`,
      text: "Esses arquivos não estão baixados neste computador e não serão sincronizados até serem baixados.",
      items: [],
      blocking: false,
    });
  }
  if (r.symlinks > 0) {
    out.push({
      title: `${r.symlinks} ${r.symlinks === 1 ? "link simbólico" : "links simbólicos"}`,
      text: "Links simbólicos não funcionam de forma confiável entre Windows e Mac.",
      items: [],
      blocking: false,
    });
  }
  if (r.truncated) {
    out.push({
      title: "Pasta muito grande: verificação parcial",
      text: `Verificamos os primeiros ${r.entriesScanned.toLocaleString("pt-BR")} itens.`,
      items: [],
      blocking: false,
    });
  }
  return out;
}
