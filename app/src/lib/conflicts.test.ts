import { describe, expect, it } from "vitest";
import { authorOf, baseName, describeChoice, totalConflicts, versionTag } from "./conflicts";

describe("versionTag", () => {
  it("formats local time like Syncthing (YYYYMMDD-HHMMSS)", () => {
    expect(versionTag(new Date(2026, 0, 2, 3, 4, 5))).toBe("20260102-030405");
    expect(versionTag(new Date(2026, 10, 16, 23, 59, 59))).toBe("20261116-235959");
  });
});

describe("authorOf", () => {
  const myId = "MFZWI3D-BONSGYC-AAAAAAA";
  const devices = [{ id: "ABCDEF2-XXXXXXX-YYYYYYY", name: "Windows (escritório)" }];
  it("names this computer", () => {
    expect(authorOf("MFZWI3D", myId, devices)).toBe("este computador");
  });
  it("names a paired device", () => {
    expect(authorOf("ABCDEF2", myId, devices)).toBe("Windows (escritório)");
  });
  it("falls back to the short ID", () => {
    expect(authorOf("ZZZZZZZ", myId, devices)).toBe("dispositivo ZZZZZZZ");
    expect(authorOf("ABCDEF2", myId, [{ id: "ABCDEF2-X", name: "" }])).toBe("dispositivo ABCDEF2");
  });
});

describe("baseName", () => {
  it("returns the last path component", () => {
    expect(baseName("a/b/c.txt")).toBe("c.txt");
    expect(baseName("c.txt")).toBe("c.txt");
  });
});

describe("describeChoice", () => {
  it("explains each option in plain words", () => {
    expect(describeChoice({ kind: "off" })).toMatch(/sem histórico/i);
    expect(describeChoice({ kind: "staggered", days: 30 })).toBe("Histórico escalonado por 30 dias");
    expect(describeChoice({ kind: "staggered", days: 0 })).toBe("Histórico escalonado para sempre");
    expect(describeChoice({ kind: "trashcan", days: 1 })).toBe("Lixeira por 1 dia");
    expect(describeChoice({ kind: "trashcan", days: 0 })).toBe("Lixeira sem limpeza automática");
    expect(describeChoice({ kind: "simple", keep: 5, days: 0 })).toBe("Últimas 5 versões");
    expect(describeChoice({ kind: "simple", keep: 1, days: 7 })).toBe("Última 1 versão, por até 7 dias");
    expect(describeChoice(null)).toMatch(/personalizado/i);
  });
});

describe("totalConflicts", () => {
  it("sums across folders", () => {
    expect(totalConflicts([])).toBe(0);
    const c = { conflictPath: "", originalPath: "", deviceShortId: "", when: "", conflictSize: 0, originalSize: null };
    expect(
      totalConflicts([
        { folderId: "a", label: "A", conflicts: [c, c], truncated: false },
        { folderId: "b", label: "B", conflicts: [c], truncated: false },
      ]),
    ).toBe(3);
  });
});
