import { describe, expect, it } from "vitest";
import { formatBytes, formatRate, relativeTime, shortId } from "./format";

describe("formatBytes", () => {
  it("uses binary units and pt-BR decimals", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1536)).toBe("1,5 KB");
    expect(formatBytes(5 * 1024 * 1024)).toBe("5 MB");
    expect(formatBytes(2.25 * 1024 ** 3)).toBe("2,3 GB");
  });
});

describe("formatRate", () => {
  it("shows a dash when idle", () => {
    expect(formatRate(0)).toBe("—");
    expect(formatRate(0.4)).toBe("—");
  });
  it("adds /s", () => {
    expect(formatRate(2048)).toBe("2 KB/s");
  });
});

describe("relativeTime", () => {
  const now = new Date("2026-11-16T12:00:00Z");
  it("handles missing values", () => {
    expect(relativeTime(null, now)).toBe("nunca");
  });
  it("buckets by age", () => {
    expect(relativeTime("2026-11-16T11:59:40Z", now)).toBe("agora");
    expect(relativeTime("2026-11-16T11:55:00Z", now)).toBe("há 5 min");
    expect(relativeTime("2026-11-16T10:00:00Z", now)).toBe("há 2 h");
    expect(relativeTime("2026-11-13T12:00:00Z", now)).toBe("há 3 dias");
    expect(relativeTime("2026-11-15T12:00:00Z", now)).toBe("há 1 dia");
  });
  it("treats future timestamps (clock skew) as now", () => {
    expect(relativeTime("2026-11-16T12:05:00Z", now)).toBe("agora");
  });
  it("rejects garbage", () => {
    expect(relativeTime("not a date", now)).toBe("nunca");
  });
});

describe("shortId", () => {
  it("keeps the first block of a device ID", () => {
    expect(shortId("ABCDEF2-GHIJKL3-XXXXXXX")).toBe("ABCDEF2");
  });
});
