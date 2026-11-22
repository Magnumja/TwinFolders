"use client";
import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useToast } from "./Toasts";
import { Dot, IconMonitor } from "./ui";

interface RemoteMachine {
  deviceId: string;
  name: string;
  host: string | null;
  online: boolean;
  os: string;
  user: string;
}

interface RemoteAccess {
  problem: string | null;
  machines: RemoteMachine[];
}

const USER_OK = /^[A-Za-z0-9._][A-Za-z0-9._-]{0,63}$/;

function Machine({ m, onSaved }: { m: RemoteMachine; onSaved: () => void }) {
  const [user, setUser] = useState(m.user);
  const [busy, setBusy] = useState(false);
  const toast = useToast();
  useEffect(() => setUser(m.user), [m.user]);
  const valid = USER_OK.test(user.trim());

  const save = async () => {
    if (!valid || user.trim() === m.user) return;
    try {
      await invoke("set_ssh_user", { deviceId: m.deviceId, user: user.trim() });
      onSaved();
    } catch (e) {
      toast("error", String(e));
    }
  };
  const open = async () => {
    setBusy(true);
    try {
      await save();
      await invoke("open_terminal", { deviceId: m.deviceId });
    } catch (e) {
      toast("error", String(e));
    } finally {
      setBusy(false);
    }
  };

  const status = !m.host
    ? { tone: "muted" as const, text: "Não aparece no seu Tailscale" }
    : m.online
      ? { tone: "ok" as const, text: `Pronto em ${m.host}` }
      : { tone: "wait" as const, text: `Desligado ou fora da rede (${m.host})` };

  return (
    <div className="remote-row">
      <span className="machine-icon small-icon"><IconMonitor size={20} /></span>
      <div className="grow">
        <strong>{m.name || m.deviceId.split("-")[0]}</strong>
        <span className={`machine-state state-${status.tone} block`}>
          <Dot tone={status.tone} /> {status.text}
        </span>
      </div>
      <label className="remote-user">
        <span className="field-label">Usuário lá</span>
        <input
          value={user}
          onChange={(e) => setUser(e.target.value)}
          onBlur={save}
          spellCheck={false}
          autoCapitalize="off"
          className={valid ? "" : "warn"}
          aria-invalid={!valid}
        />
      </label>
      <button className="btn btn-primary" onClick={open} disabled={busy || !m.host || !valid} title={m.host ? `ssh ${user}@${m.host}` : undefined}>
        {busy ? "Abrindo…" : "Abrir terminal"}
      </button>
    </div>
  );
}

/** "Open a terminal on the other computer" via SSH over Tailscale. */
export function RemotePanel() {
  const [data, setData] = useState<RemoteAccess | null>(null);
  const [error, setError] = useState<string | null>(null);
  const load = useCallback(async () => {
    try {
      setData(await invoke<RemoteAccess>("get_remote_access"));
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, []);
  useEffect(() => {
    load();
  }, [load]);

  return (
    <section className="panel remote">
      <div className="panel-head">
        <div>
          <h2>Acesso remoto</h2>
          <p className="sub">Abra um terminal no outro computador, de qualquer lugar, pela conexão privada do Tailscale.</p>
        </div>
        <button className="link-btn" onClick={load}>Verificar de novo</button>
      </div>
      {error && <p className="error remote-msg">{error}</p>}
      {data?.problem && (
        <p className="notice notice-wait remote-msg">
          Para usar, {data.problem.charAt(0).toLowerCase() + data.problem.slice(1)} Baixe em tailscale.com e entre com a mesma conta nos dois computadores.
        </p>
      )}
      {data && data.machines.length === 0 && !data.problem && <p className="sub remote-msg">Conecte o outro computador primeiro.</p>}
      {data && data.machines.length > 0 && (
        <div className="remote-list">
          {data.machines.map((m) => (
            <Machine key={m.deviceId} m={m} onSaved={load} />
          ))}
        </div>
      )}
      <details className="trouble remote-msg">
        <summary>Primeira vez? Prepare o SSH nos dois computadores</summary>
        <ol className="setup-steps">
          <li>
            No Mac, no Terminal, dentro da pasta do TwinFolders: <code>./scripts/ssh-setup-mac.sh</code>. Ele cria a chave do Mac e mostra uma linha que começa com <code>ssh-ed25519</code>.
          </li>
          <li>
            No Windows, no PowerShell como administrador: <code>powershell -ExecutionPolicy Bypass -File .\scripts\ssh-setup-windows.ps1</code>. Ele faz o mesmo para o PC.
          </li>
          <li>
            Rode de novo em cada um, passando a linha do outro: no Mac com a chave do PC entre aspas; no Windows com <code>-OtherKey "…"</code>. Isso autoriza a chave, desliga o login por senha e, no Windows, libera a porta só para o Tailscale.
          </li>
          <li>Volte aqui e clique em Abrir terminal. O usuário é o nome da sua conta no outro computador.</li>
        </ol>
      </details>
    </section>
  );
}
