import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import type { SetupInfo } from "../types";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));

const { SetupView } = await import("./Setup");

const base: SetupInfo = {
  os: "macos",
  syncthing: null,
  packageManager: "brew",
  installCommand: "/opt/homebrew/bin/brew install syncthing",
  tailscaleProblem: "o Tailscale não está instalado neste computador.",
};

function answer(info: SetupInfo) {
  invoke.mockImplementation(async (cmd: string) => {
    if (cmd === "get_setup") return info;
    if (cmd === "install_service") return "AAAAAAA-BBBBBBB";
    return undefined;
  });
}

afterEach(() => {
  cleanup();
  invoke.mockReset();
});

describe("SetupView", () => {
  it("offers to install Syncthing with Homebrew when it is missing", async () => {
    answer(base);
    const ui = render(<SetupView onReady={() => {}} />);
    const btn = await ui.findByRole("button", { name: "Instalar com o Homebrew" });
    fireEvent.click(btn);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("install_syncthing"));
    expect(ui.getByText("/opt/homebrew/bin/brew install syncthing")).toBeTruthy();
    expect(ui.queryByRole("button", { name: "Ligar agora" })).toBeNull();
  });

  it("turns the service on once Syncthing is there, then reports ready", async () => {
    answer({ ...base, syncthing: "/opt/homebrew/bin/syncthing", tailscaleProblem: null });
    const onReady = vi.fn();
    const ui = render(<SetupView onReady={onReady} />);
    fireEvent.click(await ui.findByRole("button", { name: "Ligar agora" }));
    await waitFor(() => expect(onReady).toHaveBeenCalled());
    expect(invoke).toHaveBeenCalledWith("install_service");
    expect(ui.getByText("O Tailscale está conectado neste computador.")).toBeTruthy();
  });

  it("explains what is missing when there is no package manager", async () => {
    answer({ ...base, os: "windows", packageManager: null, installCommand: null });
    const ui = render(<SetupView onReady={() => {}} />);
    expect(await ui.findByText(/Instalador de Aplicativo/)).toBeTruthy();
  });
});
