import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import type { Overview } from "../types";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: async () => null, ask: async () => true }));

const { InviteCreator, OpenInvites, JoinInvite } = await import("./Invite");
const { Pending } = await import("./Pending");
const { RoleContext } = await import("./Team");

const exp = Math.floor(Date.now() / 1000) + 23 * 3600;
const overview = {
  myId: "ME", myName: "mac-ana", devices: [],
  folders: [{ id: "proj", label: "Projetos", path: "/x", health: { kind: "upToDate" }, needBytes: 0, globalBytes: 0, lastSync: null, peers: [] }],
  pendingDevices: [
    { deviceId: "NOTE1AA-B", name: "note-ana [tf:QW3E-RT5Y]", address: "tcp://100.64.0.2:22001", time: "" },
    { deviceId: "STRANGE-B", name: "pc-estranho", address: "", time: "" },
  ],
  pendingFolders: [],
} as unknown as Overview;

afterEach(() => {
  cleanup();
  invoke.mockReset();
});

describe("InviteCreator and OpenInvites", () => {
  it("creates an invite with the chosen folders and lists it", async () => {
    let open: { code: string; expires: number; folders: string[] }[] = [];
    invoke.mockImplementation(async (cmd: string, args?: { folders?: string[] }) => {
      if (cmd === "list_invites") return { open, matches: {} };
      if (cmd === "create_invite") {
        open = [{ code: "K7Q2-9XPM", expires: exp, folders: args?.folders ?? [] }];
        return "twinfolders-convite:ABCDEF";
      }
    });
    const ui = render(<InviteCreator overview={overview} />);
    fireEvent.click(ui.getByLabelText(/Projetos/));
    fireEvent.click(ui.getByRole("button", { name: "Gerar convite" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("create_invite", { folders: ["proj"] }));
    expect(((await ui.findByLabelText("Convite")) as HTMLTextAreaElement).value).toBe("twinfolders-convite:ABCDEF");
    const list = render(<OpenInvites overview={overview} refreshKey={1} />);
    expect(await list.findByText("K7Q2-9XPM")).toBeTruthy();
    expect(list.getByText(/vence em 23 h/)).toBeTruthy();
  });
});

describe("JoinInvite", () => {
  it("previews the central before joining", async () => {
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === "read_invite") return { hubId: "MACANAA-XXXX", hubName: "mac-ana", expires: exp };
      if (cmd === "join_with_invite") return "mac-ana";
    });
    const onJoined = vi.fn();
    const ui = render(<JoinInvite onJoined={onJoined} />);
    const join = ui.getByRole("button", { name: "Entrar no time" }) as HTMLButtonElement;
    expect(join.disabled).toBe(true);
    fireEvent.change(ui.getByPlaceholderText("twinfolders-convite:…"), { target: { value: "twinfolders-convite:ABC" } });
    expect(await ui.findByText("Entrar no espaço de mac-ana")).toBeTruthy();
    expect(ui.getByText("MACANAA")).toBeTruthy();
    fireEvent.click(join);
    await waitFor(() => expect(onJoined).toHaveBeenCalled());
    expect(invoke).toHaveBeenCalledWith("join_with_invite", { text: "twinfolders-convite:ABC" });
  });

  it("does not call the backend for text that is not an invite", async () => {
    const ui = render(<JoinInvite onJoined={() => {}} />);
    fireEvent.change(ui.getByPlaceholderText("twinfolders-convite:…"), { target: { value: "oi" } });
    expect(invoke).not.toHaveBeenCalled();
  });
});

describe("Pending on the central", () => {
  it("marks invited computers and warns about the rest", async () => {
    invoke.mockImplementation(async (cmd: string) => (cmd === "list_invites" ? { open: [], matches: { "NOTE1AA-B": "K7Q2-9XPM" } } : undefined));
    const ui = render(
      <RoleContext.Provider value="hub">
        <Pending overview={overview} onChange={() => {}} />
      </RoleContext.Provider>,
    );
    expect(await ui.findByText("Convidado · K7Q2-9XPM")).toBeTruthy();
    expect(ui.getByText("note-ana")).toBeTruthy(); // tag hidden from the name
    expect(ui.getByText("Sem convite")).toBeTruthy();
  });
});
