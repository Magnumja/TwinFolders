import { describe, expect, it } from "vitest";
import { linkState } from "./Views";
import type { DeviceView } from "../types";

const dev = (p: Partial<DeviceView>): DeviceView => ({
  id: "X", name: "", connected: false, paused: false, address: "", lastSeen: null, inRate: 0, outRate: 0, ...p,
});

describe("linkState", () => {
  it("distinguishes unpaired, offline, connected and transferring", () => {
    expect(linkState([])).toBe("none");
    expect(linkState([dev({})])).toBe("off");
    expect(linkState([dev({ connected: true, paused: true })])).toBe("off");
    expect(linkState([dev({ connected: true })])).toBe("on");
    expect(linkState([dev({ connected: true, inRate: 0.5 })])).toBe("on");
    expect(linkState([dev({}), dev({ connected: true, outRate: 2048 })])).toBe("flow");
  });
});
