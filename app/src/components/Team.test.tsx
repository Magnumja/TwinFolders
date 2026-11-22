import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import type { DeviceView, FolderView, Overview } from "../types";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ ask: async () => true }));

const { DeviceGrid, RoleContext, ShareEditor } = await import("./Team");

const dev = (id: string, name: string, connected = true): DeviceView => ({ id, name, connected, paused: false, address: "tcp://100.64.0.2:22001", lastSeen: "2026-11-22T10:00:00Z", inRate: 0, outRate: 0 });
const devices = [dev("A", "note-1"), dev("B", "note-2", false), dev("C", "pc-recepcao")];
const folder = { id: "proj", label: "Projetos", path: "/x/Projetos", health: { kind: "upToDate" }, needBytes: 0, globalBytes: 0, lastSync: null, peers: [{ deviceId: "A", name: "note-1", connected: true, completion: 100 }] } as FolderView;

afterEach(() => {
  cleanup();
  invoke.mockReset();
});

describe("ShareEditor", () => {
  it("sends the new selection and warns about who stops receiving", async () => {
    invoke.mockResolvedValue(undefined);
    const onSaved = vi.fn();
    const ui = render(<ShareEditor folder={folder} devices={devices} onClose={() => {}} onSaved={onSaved} />);
    const save = ui.getByRole("button", { name: "Salvar" }) as HTMLButtonElement;
    expect(save.disabled).toBe(true); // nothing changed yet
    fireEvent.click(ui.getByLabelText(/note-1/));
    expect(ui.getByText(/vai parar de receber/)).toBeTruthy();
    expect(save.disabled).toBe(true); // empty selection is not allowed
    fireEvent.click(ui.getByLabelText(/pc-recepcao/));
    fireEvent.click(ui.getByLabelText(/note-2/));
    fireEvent.click(save);
    await waitFor(() => expect(onSaved).toHaveBeenCalled());
    expect(invoke).toHaveBeenCalledWith("set_folder_peers", { folderId: "proj", peerIds: ["C", "B"] });
  });
});

describe("DeviceGrid", () => {
  const overview = { devices, folders: [folder], myId: "ME", myName: "mac", pendingDevices: [], pendingFolders: [] } as unknown as Overview;

  it("shows each computer with its folders and disconnects on confirm", async () => {
    invoke.mockResolvedValue(undefined);
    const onChanged = vi.fn();
    const ui = render(
      <RoleContext.Provider value="hub">
        <DeviceGrid overview={overview} onChanged={onChanged} />
      </RoleContext.Provider>,
    );
    expect(ui.getByText("Computadores do time")).toBeTruthy();
    expect(ui.getByText("2 de 3 conectados agora.")).toBeTruthy();
    expect(ui.getByText("Pasta: Projetos")).toBeTruthy();
    fireEvent.click(ui.getAllByRole("button", { name: "Desconectar" })[1]);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("remove_device", { deviceId: "B" }));
    await waitFor(() => expect(onChanged).toHaveBeenCalled());
  });

  it("a team member cannot disconnect computers", () => {
    const ui = render(
      <RoleContext.Provider value="member">
        <DeviceGrid overview={overview} onChanged={() => {}} />
      </RoleContext.Provider>,
    );
    expect(ui.queryByRole("button", { name: "Desconectar" })).toBeNull();
  });
});
