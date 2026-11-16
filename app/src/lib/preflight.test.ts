import { describe, expect, it } from "vitest";
import { warningsFor } from "./preflight";
import type { PreflightReport } from "../types";

const clean: PreflightReport = {
  entriesScanned: 10, truncated: false, invalidNames: [], invalidNamesTotal: 0,
  caseCollisions: [], cloud: null, cloudOnlyFiles: 0, symlinks: 0,
};

describe("warningsFor", () => {
  it("returns nothing for a clean folder", () => {
    expect(warningsFor(clean)).toEqual([]);
  });

  it("flags Windows-invalid names as blocking, with examples and total", () => {
    const w = warningsFor({
      ...clean,
      invalidNames: [{ path: "a/bad?.txt", problem: 'caractere "?" não é permitido no Windows' }],
      invalidNamesTotal: 7,
    });
    expect(w).toHaveLength(1);
    expect(w[0].blocking).toBe(true);
    expect(w[0].title).toBe("7 nomes que o Windows não aceita");
    expect(w[0].items[0]).toBe('a/bad?.txt — caractere "?" não é permitido no Windows');
  });

  it("explains case collisions", () => {
    const w = warningsFor({ ...clean, caseCollisions: [["Foto.jpg", "foto.jpg"]] });
    expect(w[0].title).toBe("1 conflito de maiúsculas/minúsculas");
    expect(w[0].items).toEqual(["Foto.jpg ↔ foto.jpg"]);
    expect(w[0].blocking).toBe(true);
  });

  it("warns (non-blocking) about cloud folders and placeholders", () => {
    const w = warningsFor({ ...clean, cloud: "oneDrive", cloudOnlyFiles: 3 });
    expect(w[0].blocking).toBe(false);
    expect(w[0].title).toBe("Pasta dentro do OneDrive");
    expect(w[0].text).toContain("3 arquivo(s)");
  });

  it("mentions symlinks and truncation", () => {
    const w = warningsFor({ ...clean, symlinks: 2, truncated: true });
    expect(w.map((x) => x.title)).toEqual(["2 links simbólicos", "Pasta muito grande: verificação parcial"]);
    expect(w.every((x) => !x.blocking)).toBe(true);
  });
});
