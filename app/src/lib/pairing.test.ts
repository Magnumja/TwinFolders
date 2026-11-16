import { describe, expect, it } from "vitest";
import { idGroups, looksLikeId, pairingStage } from "./pairing";
import type { DeviceView, Overview } from "../types";

const dev = (p: Partial<DeviceView>): DeviceView => ({
  id: "HHI5PAB-X", name: "Windows PC", connected: false, paused: false, address: "", lastSeen: null, inRate: 0, outRate: 0, ...p,
});
const ov = (p: Partial<Overview>): Overview => ({
  myId: "ME", myName: "Mac", devices: [], folders: [], pendingDevices: [], pendingFolders: [], ...p,
});

describe("pairingStage", () => {
  it("follows the real pairing sequence", () => {
    expect(pairingStage(ov({}))).toBe("start");
    expect(pairingStage(ov({ pendingDevices: [{ deviceId: "X", name: "", address: "", time: "" }] }))).toBe("approveHere");
    expect(pairingStage(ov({ devices: [dev({})] }))).toBe("waitingOther");
    expect(pairingStage(ov({ devices: [dev({ lastSeen: "2026-11-16T10:00:00Z" })] }))).toBe("offline");
    expect(pairingStage(ov({ devices: [dev({ connected: true })] }))).toBe("noFolders");
    expect(
      pairingStage(ov({ devices: [dev({ connected: true })], folders: [{ id: "f", label: "F", path: "/f", health: { kind: "upToDate" }, needBytes: 0, globalBytes: 0, peers: [], lastSync: null }] })),
    ).toBe("done");
  });
});

describe("idGroups / looksLikeId", () => {
  const ID = "HHI5PAB-J5EJ7WD-4TUXQXE-OG7GERX-PJKGPLY-D7KUYMC-5KBMGDN-FVNAEQM";
  it("splits an ID into its 8 blocks", () => {
    expect(idGroups(ID)).toHaveLength(8);
    expect(idGroups(ID)[0]).toBe("HHI5PAB");
  });
  it("gives instant feedback while pasting", () => {
    expect(looksLikeId("")).toBe("empty");
    expect(looksLikeId("HHI5PAB")).toBe("short");
    expect(looksLikeId(ID)).toBe("ok");
    expect(looksLikeId(ID.toLowerCase().replace(/-/g, " "))).toBe("ok");
    expect(looksLikeId(ID + "AAAA")).toBe("long");
    expect(looksLikeId(ID.replace("H", "1"))).toBe("invalid");
  });
});
