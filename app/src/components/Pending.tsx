"use client";
import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { Overview } from "../types";
import { shortId } from "../lib/format";
import { AnimatePresence, motion } from "motion/react";
import { motionTokens, springs } from "../lib/motion-tokens";
import { useSafeMotion } from "../hooks/useSafeMotion";
import { useToast } from "./Toasts";
import { IdBlocks } from "./Guide";
import { connectionKind } from "../lib/space";
import { parseInviteTag } from "../lib/invite";
import { useInvites } from "./Invite";
import { useRole } from "./Team";

/** Pairing requests and folder offers: nothing is accepted automatically. */
export function Pending({ overview, onChange }: { overview: Overview; onChange: () => void }) {
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const toast = useToast();
  const m = useSafeMotion(motionTokens.distance.sm);
  const { pendingDevices, pendingFolders } = overview;
  const empty = pendingDevices.length === 0 && pendingFolders.length === 0;
  const item = {
    initial: m.initial,
    animate: m.animate,
    exit: { opacity: 0, x: m.reduce ? 0 : motionTokens.distance.lg },
    transition: m.reduce ? m.fade : springs.snappy,
  };

  const name = (id: string) => overview.devices.find((d) => d.id === id)?.name || shortId(id);
  const role = useRole();
  const invites = useInvites(overview, pendingDevices.length > 0);

  const approve = async (deviceId: string, devName: string) => {
    setBusy(deviceId);
    setError(null);
    try {
      await invoke("approve_device", { deviceId, name: devName || shortId(deviceId) });
      const clean = parseInviteTag(devName).name;
      toast("ok", `${clean || shortId(deviceId)} aprovado.${invites.matches[deviceId] ? " As pastas do convite foram oferecidas a ele." : " Agora vocês podem compartilhar pastas."}`);
      onChange();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  const accept = async (folderId: string, offeredBy: string, label: string) => {
    const path = await open({ directory: true, multiple: false, title: `Onde guardar "${label || folderId}" neste computador?` });
    if (typeof path !== "string") return;
    setBusy(folderId);
    setError(null);
    try {
      await invoke("accept_folder", { folderId, offeredBy, label, path });
      toast("ok", `"${label || folderId}" aceita. A sincronização começou.`);
      onChange();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  return (
    <AnimatePresence initial={false}>
      {!empty && (
    <motion.section key="pending" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} transition={m.fade}>
      <h2>Pedidos <span className="count count-wait">{pendingDevices.length + pendingFolders.length}</span></h2>
      <div className="list">
        <AnimatePresence mode="popLayout" initial={false}>
        {pendingDevices.map((d) => {
          const tag = parseInviteTag(d.name);
          const invited = !!invites.matches[d.deviceId];
          return (
          <motion.article key={d.deviceId} className="card row pending" {...item}>
            <div className="grow">
              <strong>{tag.name || "Um computador sem nome"}</strong> quer se conectar
              {invited && <span className="badge badge-ok">Convidado · {invites.matches[d.deviceId]}</span>}
              {!invited && tag.code && <span className="badge badge-warn">Convite vencido ou já usado</span>}
              {!invited && !tag.code && role === "hub" && <span className="badge badge-warn">Sem convite</span>}
              {d.address && <span className="sub"> {({ Tailscale: "pelo Tailscale", "Rede local": "pela rede local", Internet: "pela internet" } as Record<string, string>)[connectionKind(d.address)] ?? ""}</span>}
              <IdBlocks id={d.deviceId} />
              <p className="sub">
                {invited ? "Veio com um convite que você gerou. " : role === "hub" ? "Não veio de um convite: aprove só se você reconhece este computador. " : "Aprove só se for o seu outro computador. "}Para conferir, abra o TwinFolders nele, vá em Computadores e veja se o ID começa com{" "}
                <b>{d.deviceId.split("-")[0]}</b>.
              </p>
            </div>
            <motion.button className="btn btn-primary" disabled={busy === d.deviceId} onClick={() => approve(d.deviceId, d.name)} whileTap={m.press}>Aprovar</motion.button>
          </motion.article>
          );
        })}
        {pendingFolders.map((f) => (
          <motion.article key={`${f.folderId}-${f.offeredBy}`} className="card row pending" {...item}>
            <div className="grow">
              <strong>{name(f.offeredBy)}</strong> quer compartilhar a pasta <strong>{f.label || f.folderId}</strong>
              <p className="sub">Ao aceitar, você escolhe onde guardar neste computador. A partir daí, a pasta fica igual nos computadores que a compartilham.</p>
            </div>
            <motion.button className="btn btn-primary" disabled={busy === f.folderId} onClick={() => accept(f.folderId, f.offeredBy, f.label)} whileTap={m.press}>Escolher onde guardar…</motion.button>
          </motion.article>
        ))}
        </AnimatePresence>
      </div>
      {error && <p className="error">{error}</p>}
    </motion.section>
      )}
    </AnimatePresence>
  );
}
