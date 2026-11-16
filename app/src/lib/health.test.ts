import { describe, expect, it } from "vitest";
import { describeHealth, overallStatus } from "./health";
import type { Overview } from "../types";

describe("describeHealth", () => {
  it("maps every state to a pt-BR label and tone", () => {
    expect(describeHealth({ kind: "upToDate" })).toEqual({ label: "Sincronizado", tone: "ok" });
    expect(describeHealth({ kind: "syncing", percent: 42.5 })).toEqual({ label: "Sincronizando 42,5%", tone: "busy" });
    expect(describeHealth({ kind: "waitingForPeer" })).toEqual({ label: "Aguardando o outro computador", tone: "wait" });
    expect(describeHealth({ kind: "scanning" })).toEqual({ label: "Verificando arquivos", tone: "busy" });
    expect(describeHealth({ kind: "paused" })).toEqual({ label: "Pausado", tone: "muted" });
    expect(describeHealth({ kind: "error", message: "sem espaço" })).toEqual({ label: "Erro", tone: "error", detail: "sem espaço" });
  });
});

const base: Overview = { myId: "ME", myName: "Mac", devices: [], folders: [], pendingDevices: [], pendingFolders: [] };
const folder = (kind: string, extra: object = {}) => ({
  id: kind, label: kind, path: "/x", needBytes: 0, globalBytes: 0, peers: [], lastSync: null,
  health: { kind, ...extra } as never,
});

describe("overallStatus", () => {
  it("says when there is nothing yet", () => {
    expect(overallStatus(base)).toEqual({ label: "Nenhuma pasta sincronizada ainda", tone: "muted" });
  });
  it("prioritizes errors, then syncing, then waiting", () => {
    expect(overallStatus({ ...base, folders: [folder("upToDate"), folder("error", { message: "x" })] }).tone).toBe("error");
    expect(overallStatus({ ...base, folders: [folder("upToDate"), folder("syncing", { percent: 1 })] })).toEqual({ label: "Sincronizando…", tone: "busy" });
    expect(overallStatus({ ...base, folders: [folder("upToDate"), folder("waitingForPeer")] })).toEqual({ label: "Aguardando o outro computador", tone: "wait" });
    expect(overallStatus({ ...base, folders: [folder("upToDate")] })).toEqual({ label: "Tudo sincronizado", tone: "ok" });
  });
  it("counts folders with errors", () => {
    const s = overallStatus({ ...base, folders: [folder("error", { message: "a" }), folder("error", { message: "b" })] });
    expect(s.label).toBe("Atenção: 2 pastas com erro");
  });
});

describe("overallStatus with conflicts", () => {
  it("conflicts outrank everything except errors", () => {
    expect(overallStatus({ ...base, folders: [folder("upToDate")] }, 1)).toEqual({ label: "1 conflito para resolver", tone: "wait" });
    expect(overallStatus({ ...base, folders: [folder("syncing", { percent: 3 })] }, 2)).toEqual({ label: "2 conflitos para resolver", tone: "wait" });
    expect(overallStatus({ ...base, folders: [folder("error", { message: "x" })] }, 2).tone).toBe("error");
    expect(overallStatus({ ...base, folders: [folder("upToDate")] }, 0).label).toBe("Tudo sincronizado");
  });
});
