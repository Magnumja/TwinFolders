import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { FolderView, Ignores } from "../types";
import { Modal } from "./Modal";

/** Edits a folder's .stignore through Syncthing, which validates it. */
export function IgnoreEditor({ folder, onClose }: { folder: FolderView; onClose: () => void }) {
  const [text, setText] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    invoke<Ignores>("get_ignores", { folderId: folder.id })
      .then((ig) => setText(ig.lines.join("\n")))
      .catch((e) => setError(String(e)));
  }, [folder.id]);

  const save = async () => {
    setBusy(true);
    setSaved(false);
    try {
      const r = await invoke<Ignores>("set_ignores", { folderId: folder.id, text: text ?? "" });
      setError(r.error);
      setSaved(!r.error);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal title={`Exclusões — ${folder.label}`} onClose={onClose}>
      <div className="form">
        <p className="muted small">
          Um padrão por linha. Ex.: <code>*.tmp</code>, <code>Cache/</code>, <code>!importante.log</code> (exceção).
          A primeira regra que combinar vale. Linhas com <code>//</code> são comentários.
        </p>
        {text === null && !error ? (
          <p className="muted">Carregando…</p>
        ) : (
          <textarea className="mono editor" rows={14} value={text ?? ""} onChange={(e) => { setText(e.target.value); setSaved(false); }} spellCheck={false} aria-label="Regras de exclusão" />
        )}
        {error && <p className="error">Syncthing recusou as regras: {error}</p>}
        {saved && <p className="ok-text">✓ Salvo e aplicado.</p>}
        <div className="actions">
          <button className="btn" onClick={onClose}>Fechar</button>
          <button className="btn btn-primary" onClick={save} disabled={busy || text === null}>Salvar</button>
        </div>
      </div>
    </Modal>
  );
}
