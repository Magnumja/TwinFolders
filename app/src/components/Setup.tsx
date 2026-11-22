"use client";
import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { SetupInfo } from "../types";
import { Step } from "./Guide";
import { useToast } from "./Toasts";
import { IconCopy } from "./ui";

/** Next first-run step: install Syncthing, or turn the service on. */
const setupStage = (info: SetupInfo) => (info.syncthing ? "service" : "syncthing");

/** While waiting for Syncthing to be installed in the terminal. */
const RECHECK_MS = 4000;

function CopyCommand({ command }: { command: string }) {
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(command);
      setCopied(true);
      setTimeout(() => setCopied(false), 1800);
    } catch {
      // Clipboard unavailable: the command is visible right next to it.
    }
  };
  return (
    <div className="command">
      <code>{command}</code>
      <button className="btn" onClick={copy} aria-label="Copiar comando">
        <IconCopy /> {copied ? "Copiado" : "Copiar"}
      </button>
    </div>
  );
}

/**
 * First run on this computer: install Syncthing (via Homebrew/winget, in
 * a visible terminal), then turn on the background service with one click.
 * Shown instead of the panel until the service answers.
 */
export function SetupView({ onReady }: { onReady: () => void }) {
  const [info, setInfo] = useState<SetupInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<"terminal" | "service" | null>(null);
  const toast = useToast();

  const load = useCallback(async () => {
    try {
      setInfo(await invoke<SetupInfo>("get_setup"));
    } catch (e) {
      setError(String(e));
    }
  }, []);
  useEffect(() => {
    load();
  }, [load]);

  const stage = info ? setupStage(info) : null;
  // Re-check on a timer only while Syncthing is missing (it is being
  // installed in another window).
  useEffect(() => {
    if (stage !== "syncthing") return;
    const t = setInterval(load, RECHECK_MS);
    return () => clearInterval(t);
  }, [stage, load]);

  const openInstaller = async () => {
    setBusy("terminal");
    setError(null);
    try {
      await invoke("install_syncthing");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  const turnOn = async () => {
    setBusy("service");
    setError(null);
    try {
      await invoke<string>("install_service");
      toast("ok", "Pronto. O TwinFolders agora roda sozinho sempre que você entrar no computador.");
      onReady();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  const mac = info?.os === "macos";
  const pmName = mac ? "Homebrew" : "winget";

  return (
    <div className="view">
      <header className="hero">
        <div className="grow">
          <h1>Vamos preparar este computador</h1>
          <p className="hero-sub">
            São dois passos, uma vez só. Depois disso a sincronização roda em segundo plano, mesmo com esta janela fechada.
          </p>
        </div>
      </header>

      <section className="panel guide" aria-label="Preparar este computador">
        <ol className="steps-guide">
          <Step n={1} state={stage === "service" ? "done" : "current"} title="Instalar o Syncthing, o motor da sincronização">
            {!info && <p className="step-text">Procurando…</p>}
            {info && info.packageManager && info.installCommand && (
              <>
                <p className="step-text">
                  O TwinFolders usa o Syncthing, um programa livre e aberto, para levar os arquivos de um computador ao outro. O botão abre uma janela do{" "}
                  {mac ? "Terminal" : "Prompt de Comando"} instalando pelo {pmName}.{" "}
                  {mac ? "Se pedir a senha do Mac, digite lá. Se o Homebrew sugerir “brew services start”, ignore: o passo 2 cuida disso." : "Se o Windows pedir permissão, aceite."} Esta tela percebe sozinha quando terminar.
                </p>
                <div className="actions">
                  <button className="btn btn-primary" onClick={openInstaller} disabled={busy !== null}>
                    {busy === "terminal" ? "Abrindo…" : `Instalar com o ${pmName}`}
                  </button>
                </div>
                <details className="trouble">
                  <summary>Prefere digitar você mesmo?</summary>
                  <CopyCommand command={info.installCommand} />
                </details>
              </>
            )}
            {info && !info.packageManager && (
              <p className="step-text">
                {mac ? (
                  <>
                    Instale primeiro o <b>Homebrew</b> (instruções em brew.sh) e volte aqui. Esta tela percebe sozinha quando o Syncthing aparecer.
                  </>
                ) : (
                  <>
                    Falta o <b>winget</b>. Instale o “Instalador de Aplicativo” pela Microsoft Store e volte aqui. Esta tela percebe sozinha.
                  </>
                )}
              </p>
            )}
          </Step>

          <Step n={2} state={stage === "service" ? "current" : "next"} title="Ligar a sincronização em segundo plano">
            <p className="step-text">
              O Syncthing passa a iniciar sozinho quando você entra no computador. Fica só na sua conta, sem senha de administrador, e o painel dele só aceita conexões deste
              computador. Encontrado em <b>{info?.syncthing}</b>.
            </p>
            <div className="actions">
              <button className="btn btn-primary" onClick={turnOn} disabled={busy !== null}>
                {busy === "service" ? "Ligando… (leva alguns segundos)" : "Ligar agora"}
              </button>
            </div>
          </Step>
        </ol>
        {error && <p className="error setup-error">{error}</p>}
      </section>

      <section className="panel remote">
        <div className="panel-head">
          <div>
            <h2>Opcional: Tailscale</h2>
            <p className="sub">
              Na mesma rede (casa ou escritório) os dois computadores se encontram sozinhos. Para sincronizar de longe e abrir um terminal no outro, instale o Tailscale nos dois e entre
              com a mesma conta.
            </p>
          </div>
          <button className="link-btn" onClick={load}>Verificar de novo</button>
        </div>
        {info && (
          <p className={`remote-msg ${info.tailscaleProblem ? "sub" : "state-ok"}`}>
            {info.tailscaleProblem ? `Agora: ${lower(info.tailscaleProblem)}` : "O Tailscale está conectado neste computador."}
          </p>
        )}
      </section>
    </div>
  );
}

function lower(s: string) {
  return s.charAt(0).toLowerCase() + s.slice(1);
}
