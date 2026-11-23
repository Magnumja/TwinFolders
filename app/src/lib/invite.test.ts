import { describe, expect, it } from "vitest";
import { expiresIn, looksLikeInvite, parseInviteTag } from "./invite";

describe("invite helpers", () => {
  it("splits the invite tag from a waiting computer's name", () => {
    expect(parseInviteTag("note-ana [tf:K7Q2-9XPM]")).toEqual({ name: "note-ana", code: "K7Q2-9XPM" });
    expect(parseInviteTag("note-ana")).toEqual({ name: "note-ana", code: null });
    expect(parseInviteTag("x [tf:K7Q2-9XP0]")).toEqual({ name: "x [tf:K7Q2-9XP0]", code: null });
  });

  it("words the expiry", () => {
    const now = 1_000_000_000_000;
    expect(expiresIn(now / 1000 + 23 * 3600, now)).toBe("vence em 23 h");
    expect(expiresIn(now / 1000 + 600, now)).toBe("vence em 10 min");
    expect(expiresIn(now / 1000 - 5, now)).toBe("vence em 1 min");
  });

  it("recognises invite text even with chat line breaks", () => {
    expect(looksLikeInvite(" twinfolders-\nconvite:ABC")).toBe(true);
    expect(looksLikeInvite("MFZWI3D-BONSGYC")).toBe(false);
  });
});
