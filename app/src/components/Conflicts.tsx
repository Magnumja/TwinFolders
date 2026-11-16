"use client";
import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AnimatePresence, motion } from "motion/react";
import type { ConflictEntry, FolderConflicts, Overview, Resolution, ResolveOutcome } from "../types";
import { authorOf, baseName, totalConflicts, versionTag } from "../lib/conflicts";
import { formatBytes } from "../lib/format";
import { motionTokens, springs } from "../lib/motion-tokens";
import { staggerContainer, useSafeMotion } from "../hooks/useSafeMotion";
import { useToast } from "./Toasts";

const OPTIONS: { r: Resolution; label: string; hint: string; primary?: boolean }[] = [
  { r: "keepMine", label: "Manter a atual", hint: "A cópia em conflito vai para o histórico de versões." },
  { r: "keepOther", label: "Usar a cópia", hint: "A versão atual vai para o histórico e a cópia toma o lugar dela." },
  { r: "keepBoth", label: "Manter as duas", hint: "A cópia ganha um nome legível ao lado do arquivo.", primary: true },
];

function ConflictCard({
  folder,
  c,
  overview,
  onResolved,
}: {
  folder: FolderConflicts;
  c: ConflictEntry;
  overview: Overview;
  onResolved: () => Promise<void> | void;
}) {
  const m = useSafeMotion(motionTokens.distance.md);
  const toast = useToast();
  const [busy, setBusy] = useState<Resolution | null>(null);
  const author = authorOf(c.deviceShortId, overview.myId, overview.devices);

  const resolve = async (r: Resolution) => {
    setBusy(r);
    try {
      const out = await invoke<ResolveOutcome>("resolve_conflict", {
        folderId: folder.folderId,
        conflictPath: c.conflictPath,
        resolution: r,
        tag: versionTag(),
      });
      const name = baseName(c.originalPath);
      if (r === "keepBoth") toast("ok", `Mantidas as duas: "${baseName(out.renamedTo[0] ?? name)}" está ao lado de "${name}".`);
      else toast("ok", `"${name}" resolvido. A outra versão está guardada no histórico (.stversions).`);
      await onResolved();
    } catch (e) {
      toast("error", `Não foi possível resolver "${baseName(c.originalPath)}": ${e}`);
    } finally {
      // If the card is still listed (refresh failed), it must stay usable.
      setBusy(null);
    }
  };

  return (
    <motion.article
      className="card conflict"
      variants={{ hidden: m.initial, visible: { ...m.animate, transition: m.reduce ? m.fade : springs.snappy } }}
      exit={{ opacity: 0, x: m.reduce ? 0 : motionTokens.distance.lg, transition: { duration: motionTokens.duration.fast } }}
    >
      <div className="row">
        <span className="conflict-icon" aria-hidden="true">⚠︎</span>
        <div className="grow">
          <strong className="ellipsis" title={c.originalPath}>{baseName(c.originalPath)}</strong>
          <p className="muted small mono path" title={`${folder.label}/${c.originalPath}`}>{folder.label} / {c.originalPath}</p>
        </div>
      </div>
      <div className="versions">
        <div className="version">
          <span className="field-label">Versão atual</span>
          <span>{c.originalSize === null ? "apagada" : formatBytes(c.originalSize)}</span>
        </div>
        <div className="version">
          <span className="field-label">Cópia em conflito</span>
          <span>{formatBytes(c.conflictSize)} · de {author}</span>
          <span className="muted small">alterada em {c.when}</span>
        </div>
      </div>
      <div className="conflict-actions" role="group" aria-label={`Resolver ${baseName(c.originalPath)}`}>
        {OPTIONS.map((o) => (
          <motion.button
            key={o.r}
            className={o.primary ? "btn btn-primary" : "btn"}
            title={o.hint}
            disabled={busy !== null || (o.r === "keepOther" && c.originalSize === null)}
            whileTap={m.press}
            transition={springs.snappy}
            onClick={() => resolve(o.r)}
          >
            {busy === o.r ? "Resolvendo…" : o.label}
          </motion.button>
        ))}
      </div>
    </motion.article>
  );
}

/** Pending conflict copies, shown above the folders when there are any. */
export function Conflicts({ all, overview, onChange }: { all: FolderConflicts[]; overview: Overview; onChange: () => Promise<void> | void }) {
  const total = totalConflicts(all);
  const truncated = all.filter((f) => f.truncated).map((f) => f.label);
  return (
    <>
      {truncated.length > 0 && (
        <p className="notice notice-wait small">
          Pastas grandes demais para procurar conflitos por inteiro: {truncated.join(", ")}. Conflitos nelas podem não aparecer aqui.
        </p>
      )}
      {/* No initial={false} here: it would propagate to the staggered list
          below and leave the cards stuck in their "hidden" variant. */}
      <AnimatePresence>
        {total > 0 && (
          <motion.section
            key="conflicts"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: motionTokens.duration.fast }}
            aria-live="polite"
          >
            <h2>
              Conflitos <span className="count count-wait">{total}</span>
            </h2>
            <p className="muted small intro">
              O mesmo arquivo mudou nos dois computadores antes de sincronizar. Nada foi perdido: escolha o que fazer com cada um.
              A versão descartada fica no histórico de versões da pasta.
            </p>
            <motion.div className="list" variants={staggerContainer} initial="hidden" animate="visible">
              <AnimatePresence mode="popLayout">
                {all.flatMap((f) =>
                  f.conflicts.map((c) => (
                    <ConflictCard key={`${f.folderId}/${c.conflictPath}`} folder={f} c={c} overview={overview} onResolved={onChange} />
                  )),
                )}
              </AnimatePresence>
            </motion.div>
          </motion.section>
        )}
      </AnimatePresence>
    </>
  );
}
