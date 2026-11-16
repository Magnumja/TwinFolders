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
      if (!r.error) toast("ok", `Arquivos ignorados de "${folder.label}" salvos.`);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal title={`Arquivos ignorados em ${folder.label}`} onClose={onClose}>
      <div className="form">
        <p className="modal-intro">
          O que combinar com estas regras fica só neste computador: não vai para o outro nem é apagado lá.
        </p>
        <ul className="rule-examples">
          <li><code>*.tmp</code> ignora todo arquivo terminado em .tmp</li>
          <li><code>Cache/</code> ignora a pasta Cache inteira</li>
          <li><code>!importante.log</code> abre uma exceção para esse arquivo</li>
          <li>Uma regra por linha. A primeira que combinar vale. <code>//</code> começa um comentário.</li>
        </ul>
        {text === null && !error ? (
          <p className="muted">Carregando…</p>
        ) : (
          <textarea className="mono editor" rows={14} value={text ?? ""} onChange={(e) => { setText(e.target.value); setSaved(false); }} spellCheck={false} aria-label="Regras de exclusão" />
        )}
        <AnimatePresence mode="wait" initial={false}>
          {error && (
            <motion.p key="e" className="error" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} transition={m.fade}>
              Uma regra não foi aceita: {error}
            </motion.p>
          )}
          {saved && (
            <motion.p key="s" className="ok-text" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} transition={m.fade}>
              Salvo. Já vale para os próximos arquivos.
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
