import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/react";
import type { Overview } from "../types";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ ask: async () => true, open: async () => null }));

const { ActionMenu } = await import("./Menu");
const { ConnectDialog } = await import("./Connect");
const { RoleContext } = await import("./Team");

afterEach(() => {
  cleanup();
  invoke.mockReset();
});

describe("ActionMenu", () => {
  it("groups items, marks the destructive one and closes after a choice or Esc", () => {
    const stop = vi.fn();
    const ui = render(<ActionMenu label="Mais ações" items={[{ label: "Abrir", onSelect: () => {} }, "separator", { label: "Parar", onSelect: stop, danger: true }]} />);
    const trigger = ui.getByRole("button", { name: "Mais ações" });
    fireEvent.click(trigger);
    expect(trigger.getAttribute("aria-expanded")).toBe("true");
    expect(ui.getByRole("menu").querySelectorAll("hr")).toHaveLength(1);
    const danger = ui.getByRole("menuitem", { name: "Parar" });
    expect(danger.className).toContain("danger");
    fireEvent.click(danger);
    expect(stop).toHaveBeenCalled();
    expect(trigger.getAttribute("aria-expanded")).toBe("false");
    fireEvent.click(trigger);
    fireEvent.keyDown(window, { key: "Escape" });
    expect(trigger.getAttribute("aria-expanded")).toBe("false");
  });
});

describe("ConnectDialog", () => {
  const overview = { myId: "MACANAA-BBBBBBB", myName: "mac", devices: [], folders: [], pendingDevices: [], pendingFolders: [] } as unknown as Overview;

  it("opens on the invite tab on the central and on the ID tab otherwise", () => {
    invoke.mockResolvedValue({ open: [], matches: {} });
    const hub = render(
      <RoleContext.Provider value="hub">
        <ConnectDialog overview={overview} onClose={() => {}} onChanged={() => {}} />
      </RoleContext.Provider>,
    );
    expect(hub.getByRole("tab", { name: "Convite (recomendado)" }).getAttribute("aria-selected")).toBe("true");
    expect(hub.getByRole("button", { name: "Gerar convite" })).toBeTruthy();
    cleanup();
    const personal = render(<ConnectDialog overview={overview} onClose={() => {}} onChanged={() => {}} />);
    expect(personal.getByRole("tab", { name: "Pelo ID" }).getAttribute("aria-selected")).toBe("true");
    expect(personal.getByText("MACANAA")).toBeTruthy();
  });
});
