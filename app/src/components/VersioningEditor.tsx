"use client";
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AnimatePresence, motion } from "motion/react";
import type { FolderView, VersioningChoice, VersioningView } from "../types";
import { describeChoice } from "../lib/conflicts";
import { useSafeMotion } from "../hooks/useSafeMotion";
import { Modal } from "./Modal";
import { useToast } from "./Toasts";

type Kind = VersioningChoice["kind"];

const KINDS: { kind: Kind; title: string; text: string }[] = [
  {
    kind: "staggered",
    title: "Histórico escalonado (recomendado)",
    text: "Guarda versões antigas espaçando-as com o tempo: várias da última hora, uma por dia no último mês, uma por semana depois.",
  },
  { kind: "simple", title: "Últimas N versões", text: "Guarda um número fixo de versões de cada arquivo." },
  { kind: "trashcan", title: "Lixeira", text: "Guarda só a última versão de arquivos apagados ou substituídos." },
  { kind: "off", title: "Desligado", text: "Arquivos substituídos ou apagados pelo outro computador somem sem cópia." },
];

const DEFAULTS: Record<Kind, VersioningChoice> = {
  staggered: { kind: "staggered", days: 30 },
  simple: { kind: "simple", keep: 5, days: 0 },
  trashcan: { kind: "trashcan", days: 30 },
  off: { kind: "off" },
};

/** Lets the field be cleared while typing; clamps only on blur. */
function NumberField({ label, value, min, onChange, help }: { label: string; value: number; min: number; onChange: (n: number) => void; help: string }) {
  const [raw, setRaw] = useState(String(value));
  useEffect(() => setRaw(String(value)), [value]);
  const clamp = (t: string) => Math.max(min, Math.min(36500, Math.floor(Number(t)) || min));
  return (
    <label className="field inline-field">
      <span className="field-label">{label}</span>
      <input
        type="number"
        inputMode="numeric"
        min={min}
        max={36500}
        value={raw}
        onChange={(e) => {
          setRaw(e.target.value);
          if (e.target.value.trim() !== "" && Number.isFinite(Number(e.target.value))) onChange(clamp(e.target.value));
        }}
        onBlur={() => {
          const n = clamp(raw);
          setRaw(String(n));
          onChange(n);
        }}
      />
      <span className="muted small">{help}</span>
    </label>
  );
}

/** Per-folder file versioning (where discarded conflict versions also go). */
export function VersioningEditor({ folder, onClose }: { folder: FolderView; onClose: () => void }) {
  const [view, setView] = useState<VersioningView | null>(null);
  const [choice, setChoice] = useState<VersioningChoice | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const toast = useToast();
  const m = useSafeMotion();

  useEffect(() => {
    invoke<VersioningView>("get_versioning", { folderId: folder.id })
      .then((v) => {
        setView(v);
        setChoice(v.choice);
      })
      .catch((e) => setError(String(e)));
  }, [folder.id]);

  const save = async () => {
    if (!choice) return;
    setBusy(true);
    try {
      await invoke("set_versioning", { folderId: folder.id, choice });
      toast("ok", `Versões de "${folder.label}": ${describeChoice(choice)}.`);
      onClose();
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  };

  const custom = view !== null && view.choice === null;

  return (
    <Modal title={`Versões — ${folder.label}`} onClose={onClose}>
      <div className="form">
        <p className="muted small">
          Quando o outro computador altera ou apaga um arquivo, a versão anterior pode ser guardada na pasta oculta{" "}
          <code>.stversions</code>. As versões descartadas ao resolver conflitos também vão para lá.
        </p>
        {view === null && !error && <p className="muted">Carregando…</p>}
        {custom && (
          <p className="notice notice-wait small">
            Esta pasta usa um tipo de versionamento configurado fora do TwinFolders ({view.kind}). Para não apagar essa configuração, o TwinFolders não a altera; mude-a no painel do Syncthing.
          </p>
        )}
        {view !== null && !custom && (
          <div className="choice-list" role="radiogroup" aria-label="Tipo de versionamento">
            {KINDS.map((k) => {
              const selected = choice?.kind === k.kind;
              return (
                <div key={k.kind} className={selected ? "choice selected" : "choice"}>
                  <label className="choice-head">
                    <input
                      type="radio"
                      name="versioning"
                      checked={selected}
                      onChange={() => setChoice(DEFAULTS[k.kind])}
                    />
                    <span>
                      <strong>{k.title}</strong>
                      <span className="muted small block">{k.text}</span>
                    </span>
                  </label>
                  <AnimatePresence initial={false}>
                    {selected && choice && choice.kind !== "off" && (
                      <motion.div
                        key="params"
                        className="choice-params"
                        initial={{ opacity: 0 }}
                        animate={{ opacity: 1 }}
                        exit={{ opacity: 0 }}
                        transition={m.fade}
                      >
                        {choice.kind === "simple" && (
                          <NumberField label="Versões" value={choice.keep} min={1} help="por arquivo" onChange={(keep) => setChoice({ ...choice, keep })} />
                        )}
                        <NumberField
                          label={choice.kind === "staggered" ? "Guardar por" : "Apagar após"}
                          value={choice.days}
                          min={0}
                          help={choice.kind === "staggered" ? "dias (0 = para sempre)" : "dias (0 = nunca)"}
                          onChange={(days) => setChoice({ ...choice, days })}
                        />
                      </motion.div>
                    )}
                  </AnimatePresence>
                </div>
              );
            })}
          </div>
        )}
        <AnimatePresence initial={false}>
          {error && (
            <motion.p key="e" className="error" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} transition={m.fade}>
              {error}
            </motion.p>
          )}
        </AnimatePresence>
        <div className="actions">
          {choice && <span className="muted small grow">{describeChoice(choice)}</span>}
          <button className="btn" onClick={onClose}>Cancelar</button>
          <button className="btn btn-primary" onClick={save} disabled={busy || !choice}>Salvar</button>
        </div>
      </div>
    </Modal>
  );
}
