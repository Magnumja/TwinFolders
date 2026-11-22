import { describe, expect, it } from "vitest";
import type { DeviceView, FolderView, Overview } from "../types";
import { foldersByDevice, others, sharedWith, teamLine, validSelection } from "./team";

const dev = (id: string, name: string, connected = true): DeviceView => ({ id, name, connected, paused: false, address: "", lastSeen: null, inRate: 0, outRate: 0 });
const folder = (id: string, peers: string[]): FolderView =>
  ({ id, label: id, path: `/x/${id}`, health: { kind: "upToDate" }, needBytes: 0, globalBytes: 0, lastSync: null, peers: peers.map((p) => ({ deviceId: p, name: "", connected: true, completion: 100 })) }) as FolderView;

const devices = [dev("A", "note-1"), dev("B", "note-2", false), dev("C", "pc-recepcao")];

describe("team helpers", () => {
  it("words the other side by role and count", () => {
    expect(others(1, "personal")).toBe("o outro computador");
    expect(others(3, "personal")).toBe("os outros computadores");
    expect(others(4, "hub")).toBe("os computadores do time");
    expect(others(1, "member")).toBe("a central");
  });

  it("lists who each folder goes to", () => {
    expect(sharedWith(folder("x", []), devices)).toBe("Só neste computador");
    expect(sharedWith(folder("x", ["A", "B"]), devices)).toBe("Com note-1 e note-2");
    expect(sharedWith(folder("x", ["A", "B", "C", "Z"]), devices)).toBe("Com note-1, note-2 e mais 2");
  });

  it("groups folders by computer, ignoring unknown peers", () => {
    const o = { devices, folders: [folder("p1", ["A", "C"]), folder("p2", ["C", "Z"])] } as unknown as Overview;
    const m = foldersByDevice(o);
    expect(m.get("A")?.map((f) => f.id)).toEqual(["p1"]);
    expect(m.get("B")).toEqual([]);
    expect(m.get("C")?.map((f) => f.id)).toEqual(["p1", "p2"]);
    expect(m.has("Z")).toBe(false);
  });

  it("summarizes how many are online", () => {
    expect(teamLine([])).toBe("Nenhum computador conectado ainda.");
    expect(teamLine(devices)).toBe("2 de 3 conectados agora.");
    expect(teamLine([dev("A", "a")])).toBe("Conectado agora.");
  });

  it("only accepts a non-empty selection of paired computers", () => {
    expect(validSelection([], devices)).toBe(false);
    expect(validSelection(["A", "A"], devices)).toBe(false);
    expect(validSelection(["A", "Z"], devices)).toBe(false);
    expect(validSelection(["A", "C"], devices)).toBe(true);
  });
});
