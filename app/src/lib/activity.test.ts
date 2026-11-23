import { describe, expect, it } from "vitest";
import type { ActivityEntry, DeviceView } from "../types";
import { filterActivity, groupByDay, lastByWho, verb, whoName } from "./activity";

const now = new Date("2026-11-22T15:00:00");
const e = (time: string, path: string, by?: string, action = "modified", folderId = "proj"): ActivityEntry => ({ time, folderId, folderLabel: folderId === "proj" ? "Projetos" : "Financeiro", path, action, kind: "file", by });
const list = [
  e("2026-11-22T14:50:00", "orcamento.xlsx", "NOTE1AA"),
  e("2026-11-22T09:00:00", "ata.docx"),
  e("2026-11-21T18:00:00", "velho.txt", "NOTE2AA", "deleted", "fin"),
  e("2026-10-01T10:00:00", "antigo.pdf", "NOTE1AA"),
];
const devices = [{ id: "NOTE1AA-BBBBBBB", name: "note-ana" }, { id: "NOTE2AA-BBBBBBB", name: "note-bruno" }] as DeviceView[];

describe("activity helpers", () => {
  it("names who changed it and words the action", () => {
    expect(whoName(list[0], devices, "mac-ana")).toBe("note-ana");
    expect(whoName(list[1], devices, "mac-ana")).toBe("mac-ana");
    expect(whoName(e("t", "x", "ZZZZZZZ"), devices, "m")).toBe("ZZZZZZZ");
    expect(verb(list[0])).toBe("alterou");
    expect(verb(list[2])).toBe("apagou");
  });

  it("filters by computer, folder, period and text", () => {
    const base = { who: "all", folder: "all", period: "all" as const, query: "" };
    expect(filterActivity(list, { ...base, period: "today" }, now).map((x) => x.path)).toEqual(["orcamento.xlsx", "ata.docx"]);
    expect(filterActivity(list, { ...base, period: "week" }, now)).toHaveLength(3);
    expect(filterActivity(list, { ...base, who: "local" }, now).map((x) => x.path)).toEqual(["ata.docx"]);
    expect(filterActivity(list, { ...base, who: "NOTE1AA-BBBBBBB" }, now)).toHaveLength(2);
    expect(filterActivity(list, { ...base, folder: "fin" }, now)).toHaveLength(1);
    expect(filterActivity(list, { ...base, query: "ORÇ".replace("Ç", "C") }, now).map((x) => x.path)).toEqual(["orcamento.xlsx"]);
  });

  it("groups by day and finds each computer's latest change", () => {
    const g = groupByDay(list, now);
    expect(g.map((x) => x.label)).toEqual(["Hoje", "Ontem", "1 de outubro"]);
    expect(g[0].items).toHaveLength(2);
    const last = lastByWho(list);
    expect(last.get("NOTE1AA")?.path).toBe("orcamento.xlsx");
    expect(last.get("local")?.path).toBe("ata.docx");
  });
});
