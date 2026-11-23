"use client";
import { useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import type { Overview } from "../types";
import { motionTokens, springs } from "../lib/motion-tokens";
import { useSafeMotion } from "../hooks/useSafeMotion";
import { Modal } from "./Modal";
import { AddDeviceForm, IdBlocks } from "./Guide";
import { InviteCreator } from "./Invite";
import { useRole } from "./Team";
import { IconCopy } from "./ui";

type Tab = "invite" | "id";

function CopyMyId({ id }: { id: string }) {
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(id);
      setCopied(true);
      setTimeout(() => setCopied(false), 1600);
    } catch {
      // The blocks above show the full ID.
    }
  };
  return (
    <button className="btn" onClick={copy}>
      <IconCopy /> {copied ? "ID copiado" : "Copiar meu ID"}
    </button>
  );
}

/**
 * The one place to bring in another computer: by invite (the office way)
 * or by pasting its ID (the personal way). The role picks the first tab.
 */
export function ConnectDialog({ overview, onClose, onChanged }: { overview: Overview; onClose: () => void; onChanged: () => void }) {
  const role = useRole();
  const [tab, setTab] = useState<Tab>(role === "hub" ? "invite" : "id");
  const m = useSafeMotion(8);
  const tabs: { id: Tab; label: string }[] = [
    { id: "invite", label: role === "hub" ? "Convite (recomendado)" : "Convite" },
    { id: "id", label: "Pelo ID" },
  ];
  return (
    <Modal title={role === "hub" ? "Convidar computador" : "Conectar computador"} onClose={onClose}>
      <div className="form">
        <div className="tabs" role="tablist" aria-label="Forma de conectar">
          {tabs.map((t) => (
            <button key={t.id} role="tab" aria-selected={tab === t.id} className={tab === t.id ? "tab active" : "tab"} onClick={() => setTab(t.id)}>
              {tab === t.id && <motion.span layoutId="connect-tab" className="tab-bg" transition={springs.snappy} />}
              <span>{t.label}</span>
            </button>
          ))}
        </div>
        <AnimatePresence mode="wait" initial={false}>
          <motion.div key={tab} initial={m.initial} animate={m.animate} exit={m.exit} transition={{ duration: motionTokens.duration.fast, ease: motionTokens.easing.smooth }}>
            {tab === "invite" ? (
              <InviteCreator overview={overview} onCreated={onChanged} />
            ) : (
              <div className="connect-id">
                <p className="sub">Cole o ID do outro computador. Ele recebe um pedido e aprova lá.</p>
                <AddDeviceForm
                  onDone={() => {
                    onChanged();
                    onClose();
                  }}
                />
                <div className="my-id">
                  <p className="field-label">Ou faça pelo outro lado: cole este ID lá</p>
                  <IdBlocks id={overview.myId} />
                  <CopyMyId id={overview.myId} />
                </div>
              </div>
            )}
          </motion.div>
        </AnimatePresence>
      </div>
    </Modal>
  );
}
