import { describe, expect, it } from "vitest";
import { activeTransfer, beamSeconds, connectionKind, displayName, folderSummary, filterFolders, hero, initial, osLabel, totals } from "./space";
import type { DeviceView, FolderView, Overview } from "../types";

const dev = (p: Partial<DeviceView>): DeviceView => ({
  id: "WIN1234-X", name: "Windows PC", connected: true, paused: false, address: "", lastSeen: null, inRate: 0, outRate: 0, ...p,
});
const folder = (p: Partial<FolderView>): FolderView => ({
  id: "f", label: "F", path: "/f", health: { kind: "upToDate" }, needBytes: 0, globalBytes: 100, peers: [], lastSync: null, ...p,
});
const ov = (p: Partial<Overview>): Overview => ({
  myId: "MAC1234-Y", myName: "MacBook", devices: [], folders: [], pendingDevices: [], pendingFolders: [], ...p,
});

describe("hero", () => {
  it("speaks to the state of the space", () => {
    expect(hero(ov({}), 0).title).toBe("Vamos conectar seus computadores.");
    expect(hero(ov({ devices: [dev({})] }), 0).title).toBe("Seu espaço, em sintonia.");
    expect(hero(ov({ devices: [dev({ connected: false })] }), 0).title).toBe("Falta aprovar no outro computador.");
    expect(hero(ov({ devices: [dev({ connected: false, lastSeen: "2026-11-16T10:00:00Z" })] }), 0).title).toBe("Seu espaço espera o outro computador.");
    expect(hero(ov({ devices: [dev({})] }), 2).title).toBe("Dois arquivos precisam de você.");
    expect(hero(ov({ devices: [dev({})] }), 1).title).toBe("Um arquivo precisa de você.");
    expect(hero(ov({ devices: [dev({})] }), 7).title).toBe("7 arquivos precisam de você.");
    expect(hero(ov({ devices: [dev({})], folders: [folder({ health: { kind: "error", message: "x" } })] }), 0).title).toBe("Algo precisa de atenção.");
  });
});

describe("totals", () => {
  it("sums folders, bytes and live rates, naming who sends", () => {
    const t = totals(
      ov({
        devices: [dev({ inRate: 1000, outRate: 10 }), dev({ id: "B", name: "Velho", connected: false, inRate: 5 })],
        folders: [folder({ globalBytes: 100 }), folder({ globalBytes: 50 })],
      }),
    );
    expect(t).toEqual({ folders: 2, bytes: 150, down: 1000, up: 10, downFrom: "Windows PC", upTo: "Windows PC" });
  });
});

describe("activeTransfer", () => {
  it("picks the syncing folder with the most left to fetch", () => {
    const o = ov({
      devices: [dev({ inRate: 2048 })],
      folders: [
        folder({ id: "a", health: { kind: "syncing", percent: 90 }, needBytes: 10, globalBytes: 100 }),
        folder({ id: "b", health: { kind: "syncing", percent: 40 }, needBytes: 600, globalBytes: 1000 }),
        folder({ id: "c" }),
      ],
    });
    expect(activeTransfer(o)).toMatchObject({ folder: { id: "b" }, percent: 40, rate: 2048, etaSeconds: 1 });
    expect(activeTransfer(ov({ folders: [folder({})] }))).toBeNull();
  });
});

describe("filterFolders", () => {
  const list = [
    folder({ id: "1", label: "Design", health: { kind: "syncing", percent: 3 } }),
    folder({ id: "2", label: "Documentos", path: "/Users/m/Documents" }),
    folder({ id: "3", label: "Fotos", health: { kind: "waitingForPeer" } }),
  ];
  it("filters by tab and accent-insensitive search", () => {
    expect(filterFolders(list, "all", "").map((f) => f.id)).toEqual(["1", "2", "3"]);
    expect(filterFolders(list, "syncing", "").map((f) => f.id)).toEqual(["1"]);
    expect(filterFolders(list, "upToDate", "").map((f) => f.id)).toEqual(["2"]);
    expect(filterFolders(list, "all", "docu").map((f) => f.id)).toEqual(["2"]);
    expect(filterFolders(list, "all", "FÓTOS").map((f) => f.id)).toEqual(["3"]);
  });
});

describe("profile helpers", () => {
  it("formats the account name and OS", () => {
    expect(displayName("magnum")).toBe("Magnum");
    expect(displayName("ana.paula")).toBe("Ana Paula");
    expect(displayName("")).toBe("Você");
    expect(initial("magnum")).toBe("M");
    expect(initial("")).toBe("?");
    expect(osLabel("macos")).toBe("macOS");
    expect(osLabel("windows")).toBe("Windows");
    expect(osLabel("linux")).toBe("Linux");
  });
});

describe("connectionKind", () => {
  it("names the path from the peer address", () => {
    expect(connectionKind("tcp://100.92.8.12:22001")).toBe("Tailscale");
    expect(connectionKind("tcp://100.127.0.1:22001")).toBe("Tailscale");
    expect(connectionKind("tcp://100.128.0.1:22001")).toBe("Internet");
    expect(connectionKind("tcp://192.168.1.20:22001")).toBe("Rede local");
    expect(connectionKind("tcp://10.0.0.5:22001")).toBe("Rede local");
    expect(connectionKind("tcp://172.20.1.1:22001")).toBe("Rede local");
    expect(connectionKind("tcp://127.0.0.1:22001")).toBe("Este computador");
    expect(connectionKind("tcp://[fd7a:115c:a1e0::1]:22001")).toBe("Tailscale");
    expect(connectionKind("tcp://8.8.8.8:22001")).toBe("Internet");
    expect(connectionKind("")).toBe("");
  });
});

describe("folderSummary and beamSeconds", () => {
  it("summarizes folders in a sentence", () => {
    expect(folderSummary(0, "0 B")).toBe("Nenhuma pasta ainda");
    expect(folderSummary(1, "2 GB")).toBe("1 pasta, 2 GB no total");
    expect(folderSummary(4, "24,3 GB")).toBe("4 pastas, 24,3 GB no total");
  });
  it("speeds packets up with the rate, within bounds", () => {
    expect(beamSeconds(0)).toBe(3.2);
    expect(beamSeconds(1e6)).toBeLessThan(beamSeconds(1e3));
    expect(beamSeconds(1e12)).toBe(0.8);
  });
});
