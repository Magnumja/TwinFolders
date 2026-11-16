"use client";
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { FolderView, Ignores } from "../types";
import { Modal } from "./Modal";
import { AnimatePresence, motion } from "motion/react";
import { useSafeMotion } from "../hooks/useSafeMotion";
import { useToast } from "./Toasts";

/** Edits a folder's .stignore through Syncthing, which validates it. */
export function IgnoreEditor({ folder, onClose }: { folder: FolderView; onClose: () => void }) {
  const [text, setText] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const [busy, setBusy] = useState(false);
  const toast = useToast();
  const m = useSafeMotion();

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
      if (!r.error) toast("ok", `Exclusões de "${folder.label}" salvas e aplicadas.`);
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
        <AnimatePresence mode="wait" initial={false}>
          {error && (
            <motion.p key="e" className="error" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} transition={m.fade}>
              Syncthing recusou as regras: {error}
            </motion.p>
          )}
          {saved && (
            <motion.p key="s" className="ok-text" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} transition={m.fade}>
              ✓ Salvo e aplicado.
            </motion.p>
          )}
        </AnimatePresence>
        <div className="actions">
          <button className="btn" onClick={onClose}>Fechar</button>
          <button className="btn btn-primary" onClick={save} disabled={busy || text === null}>Salvar</button>
        </div>
      </div>
    </Modal>
  );
}
