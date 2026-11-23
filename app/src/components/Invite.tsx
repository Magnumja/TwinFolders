"use client";
import { createContext, useCallback, useContext, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { InvitePreview, Invites, Overview, Workspace } from "../types";
import { expiresIn, looksLikeInvite } from "../lib/invite";
import { useToast } from "./Toasts";
import { IconCopy } from "./ui";

/** Workspace (role, central, join state) and a way to reload it after changes. */
export const WorkspaceContext = createContext<{ ws: Workspace; reload: () => void }>({ ws: { role: "personal" }, reload: () => {} });
export const useWorkspace = () => useContext(WorkspaceContext);

/** Open invites and which pending requests carry one; refreshed with the overview. */
export function useInvites(overview: Overview | null, enabled: boolean): Invites {
  const [inv, setInv] = useState<Invites>({ open: [], matches: {} });
  const pendingKey = overview?.pendingDevices.map((d) => d.deviceId).join(",") ?? "";
  const load = useCallback(async () => {
    if (!enabled) return;
    try {
      setInv(await invoke<Invites>("list_invites"));
    } catch {
      // Not connected yet; nothing to match.
    }
  }, [enabled]);
  useEffect(() => {
    load();
  }, [load, pendingKey]);
  return inv;
}

/** Central: create a one-time invite for a team computer. */
export function InvitePanel({ overview }: { overview: Overview }) {
  const [folders, setFolders] = useState<string[]>([]);
  const [text, setText] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [busy, setBusy] = useState(false);
  const [open, setOpen] = useState<Invites["open"]>([]);
  const toast = useToast();

  const load = useCallback(async () => {
    try {
      setOpen((await invoke<Invites>("list_invites")).open);
    } catch {
      // Shown again on the next action.
    }
  }, []);
  useEffect(() => {
    load();
  }, [load]);

  const create = async () => {
    setBusy(true);
    try {
      setText(await invoke<string>("create_invite", { folders }));
      setCopied(false);
      load();
    } catch (e) {
      toast("error", String(e));
    } finally {
      setBusy(false);
    }
  };
  const copy = async () => {
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
    } catch {
      // The text is selectable right there.
    }
  };
  const cancel = async (code: string) => {
    try {
      await invoke("cancel_invite", { code });
      if (text?.includes(code)) setText(null);
      load();
    } catch (e) {
      toast("error", String(e));
    }
  };
  const label = (id: string) => overview.folders.find((f) => f.id === id)?.label ?? id;
  const toggle = (id: string) => setFolders((fs) => (fs.includes(id) ? fs.filter((x) => x !== id) : [...fs, id]));

  return (
    <section className="panel invite-panel">
      <div className="panel-head">
        <div>
          <h2>Convidar um computador do time</h2>
          <p className="sub">Gere um convite e mande para a pessoa (mensagem ou e-mail). Ela cola no TwinFolders do computador dela. O convite vale 24 horas e só serve uma vez.</p>
        </div>
      </div>
      <div className="invite-body">
        {overview.folders.length > 0 && (
          <div className="field">
            <span className="field-label">Pastas que esse computador vai receber (pode mudar depois)</span>
            <div className="chips">
              {overview.folders.map((f) => (
                <label key={f.id} className={folders.includes(f.id) ? "chip-check on" : "chip-check"}>
                  <input type="checkbox" checked={folders.includes(f.id)} onChange={() => toggle(f.id)} /> {f.label}
                </label>
              ))}
            </div>
          </div>
        )}
        <div className="actions start">
          <button className="btn btn-primary" onClick={create} disabled={busy}>{busy ? "Gerando…" : text ? "Gerar outro convite" : "Gerar convite"}</button>
        </div>
        {text && (
          <div className="invite-text">
            <textarea readOnly rows={3} value={text} aria-label="Convite" onFocus={(e) => e.currentTarget.select()} />
            <div className="row">
              <button className="btn" onClick={copy}><IconCopy /> {copied ? "Convite copiado" : "Copiar convite"}</button>
              <span className="sub small">Quando o pedido chegar, ele aparece em Pendências marcado como <b>convidado</b>.</span>
            </div>
          </div>
        )}
        {open.length > 0 && (
          <ul className="open-invites">
            {open.map((i) => (
              <li key={i.code}>
                <span className="grow">
                  Convite <b>{i.code}</b> · {expiresIn(i.expires)}
                  {i.folders.length > 0 && <span className="sub"> · {i.folders.map(label).join(", ")}</span>}
                </span>
                <button className="link-btn danger" onClick={() => cancel(i.code)}>Cancelar</button>
              </li>
            ))}
          </ul>
        )}
      </div>
    </section>
  );
}

/** Team computer: paste the central's invite and join. */
export function JoinInvite({ onJoined }: { onJoined: () => void }) {
  const [text, setText] = useState("");
  const [preview, setPreview] = useState<InvitePreview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const { reload } = useWorkspace();
  const toast = useToast();

  const read = async (value: string) => {
    setText(value);
    setPreview(null);
    setError(null);
    if (!looksLikeInvite(value)) return;
    try {
      setPreview(await invoke<InvitePreview>("read_invite", { text: value }));
    } catch (e) {
      setError(String(e));
    }
  };
  const join = async () => {
    setBusy(true);
    setError(null);
    try {
      const hub = await invoke<string>("join_with_invite", { text });
      toast("ok", `Pedido enviado para ${hub}. Assim que a central aprovar, este computador entra no time.`);
      reload();
      onJoined();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="join-invite">
      <label className="field">
        <span className="field-label">Convite da central</span>
        <textarea rows={3} placeholder="twinfolders-convite:…" value={text} onChange={(e) => read(e.target.value)} spellCheck={false} />
      </label>
      {preview && (
        <div className="notice notice-ok">
          <strong>Entrar no espaço de {preview.hubName || "uma central"}</strong>
          <p>
            ID da central começa com <b>{preview.hubId.split("-")[0]}</b> ({expiresIn(preview.expires)}). Este computador passa a ser do time e só se conecta a essa central.
          </p>
        </div>
      )}
      {error && <p className="error">{error}</p>}
      <div className="actions start">
        <button className="btn btn-primary" onClick={join} disabled={!preview || busy}>{busy ? "Entrando…" : "Entrar no time"}</button>
      </div>
    </div>
  );
}

/** Team computer waiting for the central's approval. */
export function JoinWaiting({ ws }: { ws: Workspace }) {
  if (!ws.joining) return null;
  return (
    <section className="panel join-waiting">
      <div className="panel-head">
        <div>
          <h2>Esperando a central aprovar</h2>
          <p className="sub">
            O pedido foi para <b>{ws.hubName || "a central"}</b>. Lá, ele aparece em Pendências como <b>{ws.joining.originalName}</b>, marcado como convidado (convite {ws.joining.code}). Esta tela muda sozinha quando for aprovado.
          </p>
        </div>
      </div>
    </section>
  );
}
