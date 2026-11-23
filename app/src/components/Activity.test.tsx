import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import type { Overview } from "../types";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));

const { ActivityView, SecurityChecklist } = await import("./Activity");

const iso = (minAgo: number) => new Date(Date.now() - minAgo * 60000).toISOString();
const overview = {
  myId: "MACANAA-B", myName: "mac-ana",
  devices: [{ id: "NOTE1AA-BBBBBBB", name: "note-ana", connected: true, paused: false, address: "", lastSeen: null, inRate: 0, outRate: 0 }],
  folders: [{ id: "proj", label: "Projetos" }], pendingDevices: [], pendingFolders: [],
} as unknown as Overview;

afterEach(() => {
  cleanup();
  invoke.mockReset();
});

describe("ActivityView", () => {
  it("shows who changed what and filters by computer", async () => {
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === "get_activity") return [
        { time: iso(5), folderId: "proj", folderLabel: "Projetos", path: "orcamento.xlsx", action: "modified", kind: "file", by: "NOTE1AA" },
        { time: iso(30), folderId: "proj", folderLabel: "Projetos", path: "ata.docx", action: "deleted", kind: "file" },
      ];
      return [];
    });
    const ui = render(<ActivityView overview={overview} />);
    expect(await ui.findByText("orcamento.xlsx")).toBeTruthy();
    expect(ui.getByText("apagou")).toBeTruthy();
    expect(ui.getByText(/mudou algo há 5 min/)).toBeTruthy();
    fireEvent.click(ui.getByRole("button", { name: /note-ana/ }));
    await waitFor(() => expect(ui.queryByText("ata.docx")).toBeNull()); // exit animation
    expect(ui.getByText("orcamento.xlsx")).toBeTruthy();
  });

  it("shows the action log on its tab", async () => {
    invoke.mockImplementation(async (cmd: string) => (cmd === "get_audit" ? [{ at: 1795393000, action: "Computador aprovado", detail: "note-ana (NOTE1AA)" }] : []));
    const ui = render(<ActivityView overview={overview} />);
    fireEvent.click(ui.getByRole("tab", { name: "Ações neste computador" }));
    expect(await ui.findByText("Computador aprovado")).toBeTruthy();
  });
});

describe("SecurityChecklist", () => {
  it("marks each check as ok, attention or unknown", async () => {
    invoke.mockResolvedValue([
      { id: "disk", ok: false, title: "Disco criptografado (BitLocker)", detail: "Ligue o BitLocker" },
      { id: "local-only", ok: true, title: "Painel só neste computador", detail: "" },
      { id: "tailscale", ok: null, title: "Rede privada (Tailscale)", detail: "Não instalado" },
    ]);
    const ui = render(<SecurityChecklist />);
    const disk = (await ui.findByText("Disco criptografado (BitLocker)")).closest("li")!;
    expect(disk.className).toBe("check-bad");
    expect(ui.getByText("Painel só neste computador").closest("li")!.className).toBe("check-ok");
    expect(ui.getByText("Rede privada (Tailscale)").closest("li")!.className).toBe("check-unknown");
  });
});
