"use client";
import { useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AnimatePresence, motion } from "motion/react";
import type { Overview, View } from "../types";
import { idGroups, looksLikeId, pairingStage, type PairingStage } from "../lib/pairing";
import { shortId } from "../lib/format";
import { motionTokens } from "../lib/motion-tokens";
import { useToast } from "./Toasts";
import { IconCopy, IconPlus } from "./ui";

function CopyId({ id, label = "Copiar o ID deste computador" }: { id: string; label?: string }) {
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(id);
      setCopied(true);
      setTimeout(() => setCopied(false), 1800);
    } catch {
      // Clipboard unavailable: the ID is shown in full right next to it.
    }
  };
  return (
    <button className="btn" onClick={copy}>
      <IconCopy /> {copied ? "ID copiado" : label}
    </button>
  );
}

/** The ID in its 8 blocks, first one highlighted: what people compare. */
export function IdBlocks({ id }: { id: string }) {
  return (
    <span className="id-blocks" aria-label={`ID ${id}`}>
      {idGroups(id).map((g, i) => (
        <span key={i} className={i === 0 ? "first" : undefined}>{g}</span>
      ))}
    </span>
  );
}

const ID_HINT: Record<ReturnType<typeof looksLikeId>, string> = {
  empty: "São 8 blocos de 7 letras e números. Pode colar com ou sem traços.",
  short: "Parece incompleto. Copie o ID inteiro de novo.",
  long: "Tem caracteres a mais. Copie só o ID.",
  invalid: "IDs só têm letras e os números 2 a 7. Confira o que foi colado.",
  ok: "Formato certo.",
};

/** Paste the other computer's ID (and, across networks, its address). */
export function AddDeviceForm({ onDone }: { onDone: () => void }) {
  const [id, setId] = useState("");
  const [name, setName] = useState("");
  const [address, setAddress] = useState("");
  const [farAway, setFarAway] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const toast = useToast();
  const shape = looksLikeId(id);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (shape !== "ok" || busy) return;
    setBusy(true);
    setError(null);
    try {
      const canonical = await invoke<string>("add_device", { deviceId: id, name, address: farAway ? address : "" });
      toast("ok", `Pedido enviado para ${name.trim() || shortId(canonical)}. Agora aprove no outro computador.`);
      setId("");
      setName("");
      setAddress("");
      onDone();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <form className="add-device" onSubmit={submit}>
      <label className="field">
        <span className="field-label">ID do outro computador</span>
        <input
          value={id}
          onChange={(e) => {
            setId(e.target.value);
            setError(null);
          }}
          placeholder="XXXXXXX-XXXXXXX-XXXXXXX-…"
          spellCheck={false}
          autoCapitalize="characters"
          autoComplete="off"
          aria-describedby="id-hint"
          className={shape === "ok" ? "valid" : shape === "empty" ? "" : "warn"}
        />
        <span id="id-hint" className={`hint hint-${shape}`}>{ID_HINT[shape]}</span>
      </label>
      <label className="field">
        <span className="field-label">Como chamar esse computador (opcional)</span>
        <input value={name} onChange={(e) => setName(e.target.value)} placeholder="Ex.: PC do escritório" maxLength={60} />
      </label>
      <label className="check">
        <input type="checkbox" checked={farAway} onChange={(e) => setFarAway(e.target.checked)} />
        <span>
          Os dois estão em redes diferentes
          <span className="hint block">Na mesma casa ou escritório, deixe desmarcado: eles se encontram sozinhos.</span>
        </span>
      </label>
      <AnimatePresence initial={false}>
        {farAway && (
          <motion.label
            className="field"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: motionTokens.duration.fast }}
          >
            <span className="field-label">Nome do outro computador no Tailscale</span>
            <input value={address} onChange={(e) => setAddress(e.target.value)} placeholder="Ex.: pc-escritorio ou 100.92.8.12" spellCheck={false} />
            <span className="hint">
              Instale o Tailscale nos dois e entre com a mesma conta. O nome aparece no app do Tailscale, na lista de máquinas.
            </span>
          </motion.label>
        )}
      </AnimatePresence>
      {error && <p className="error">{error}</p>}
      <div className="actions">
        <button className="btn btn-primary" type="submit" disabled={shape !== "ok" || busy}>
          {busy ? "Enviando pedido…" : "Conectar"}
        </button>
      </div>
    </form>
  );
}

type StepState = "done" | "current" | "next";

function Step({ n, state, title, children }: { n: number; state: StepState; title: string; children?: ReactNode }) {
  return (
    <li className={`step step-${state}`} aria-current={state === "current" ? "step" : undefined}>
      <span className="step-mark" aria-hidden="true">{state === "done" ? "✓" : n}</span>
      <div className="step-body">
        <strong>{title}</strong>
        {state === "current" && children}
      </div>
    </li>
  );
}

const ORDER: PairingStage[] = ["start", "waitingOther", "noFolders", "done"];

/**
 * Step-by-step connection guide, driven by the real pairing state. Shown
 * until both computers are connected and share at least one folder.
 */
export function ConnectGuide({ overview, onChanged, onAddFolder, go }: { overview: Overview; onChanged: () => void; onAddFolder: () => void; go: (v: View) => void }) {
  const stage = pairingStage(overview);
  if (stage === "done" || stage === "offline") return null;
  // approveHere sits at the same point as waitingOther (step 3).
  const at = ORDER.indexOf(stage === "approveHere" ? "waitingOther" : stage);
  const s = (i: number): StepState => (i < at ? "done" : i === at ? "current" : "next");
  const me = overview.myName || "este computador";
  return (
    <section className="panel guide" aria-label="Conectar os dois computadores">
      <div className="panel-head">
        <div>
          <h2>Conectar os dois computadores</h2>
          <p className="sub">Você faz isso uma vez. Depois, tudo sincroniza sozinho sempre que os dois estiverem ligados.</p>
        </div>
      </div>
      <ol className="steps-guide">
        <Step n={1} state="done" title="Abra o TwinFolders nos dois computadores" />
        <Step n={2} state={s(0)} title="Cole aqui o ID do outro computador">
          <p className="step-text">
            No outro computador, abra <b>Computadores</b> e clique em <b>Copiar meu ID</b>. Mande para você mesmo (mensagem, e-mail ou AirDrop) e cole abaixo.
          </p>
          <AddDeviceForm onDone={onChanged} />
          <div className="alt">
            <p className="step-text">Prefere fazer pelo outro lado? Copie o ID daqui e cole lá, no mesmo lugar.</p>
            <IdBlocks id={overview.myId} />
            <CopyId id={overview.myId} />
          </div>
        </Step>
        <Step n={3} state={s(1)} title={stage === "approveHere" ? "Aprove o pedido aqui" : "Aprove o pedido no outro computador"}>
          {stage === "approveHere" ? (
            <>
              <p className="step-text">
                {overview.pendingDevices[0].name || "Um computador"} pediu para se conectar. Confira se o ID começa com{" "}
                <b>{shortId(overview.pendingDevices[0].deviceId)}</b> (no outro computador, em Computadores) e aprove.
              </p>
              <button className="btn btn-primary" onClick={() => go("pending")}>Ver o pedido</button>
            </>
          ) : (
            <>
              <p className="step-text">
                No outro computador aparece <b>“{me} quer se conectar”</b> em Pendências. Confira se o ID começa com <b>{shortId(overview.myId)}</b> e aprove. Esta tela atualiza sozinha.
              </p>
              <details className="trouble">
                <summary>Não apareceu nada no outro?</summary>
                <ul>
                  <li>Os dois precisam estar ligados, com o TwinFolders aberto.</li>
                  <li>Em redes diferentes, use o Tailscale nos dois e informe o nome do outro computador ao conectar.</li>
                  <li>Confira se o ID foi colado inteiro, do computador certo.</li>
                </ul>
              </details>
            </>
          )}
        </Step>
        <Step n={4} state={s(2)} title="Escolha a primeira pasta">
          <p className="step-text">
            Escolha uma pasta deste computador. O outro recebe um pedido em Pendências e decide onde guardar. A partir daí, a pasta fica igual nos dois.
          </p>
          <button className="btn btn-primary" onClick={onAddFolder}><IconPlus /> Adicionar pasta</button>
        </Step>
      </ol>
    </section>
  );
}

/* ---------- "Como funciona" ---------- */

const FAQ: { q: string; a: ReactNode }[] = [
  {
    q: "Os dois computadores precisam estar ligados?",
    a: "Sim. Os arquivos vão direto de um para o outro, sem nuvem no meio. Se um estiver desligado, as mudanças esperam e seguem sozinhas quando ele voltar.",
  },
  {
    q: "E se eu mudar o mesmo arquivo nos dois?",
    a: "Nada se perde. O TwinFolders guarda as duas versões e pergunta em Pendências qual manter: a atual, a cópia ou as duas.",
  },
  {
    q: "Apagar aqui apaga lá?",
    a: "Sim, a pasta fica igual nos dois. Com o histórico de versões ligado (Pastas, menu ⋯, Histórico de versões), o arquivo apagado fica guardado por um tempo na pasta oculta .stversions.",
  },
  {
    q: "Isso é um backup?",
    a: "Não. É um espelho: um erro feito aqui chega lá. Para backup, mantenha uma cópia separada (um HD externo, por exemplo).",
  },
  {
    q: "Quem mais consegue ver meus arquivos?",
    a: "Ninguém. Só computadores que você aprovou se conectam, e a conexão é criptografada de ponta a ponta.",
  },
  {
    q: "Como deixo uma pasta de fora por um tempo?",
    a: "Em Pastas, no menu ⋯ da pasta, escolha Pausar sincronização. Nada muda até você retomar.",
  },
];

export function HelpView({ go }: { go: (v: View) => void }) {
  return (
    <div className="view help">
      <header className="hero">
        <div className="grow">
          <h1>Como funciona</h1>
          <p className="hero-sub">O TwinFolders mantém as mesmas pastas iguais em dois computadores seus, sem passar por nenhuma nuvem.</p>
        </div>
      </header>

      <section className="panel help-flow">
        <h2>Em três passos</h2>
        <ol className="flow">
          <li>
            <strong>Conecte os dois computadores</strong>
            <span>Cole o ID de um no outro e aprove o pedido. Só uma vez.</span>
            <button className="inline-link" onClick={() => go("computers")}>Ir para Computadores</button>
          </li>
          <li>
            <strong>Escolha as pastas</strong>
            <span>Adicione uma pasta aqui; o outro computador aceita e escolhe onde guardar.</span>
            <button className="inline-link" onClick={() => go("folders")}>Ir para Pastas</button>
          </li>
          <li>
            <strong>Use normalmente</strong>
            <span>Edite, crie e apague arquivos como sempre. As mudanças aparecem no outro em segundos.</span>
          </li>
        </ol>
      </section>

      <section className="panel faq">
        <h2>Dúvidas comuns</h2>
        <dl>
          {FAQ.map((f) => (
            <div key={f.q}>
              <dt>{f.q}</dt>
              <dd>{f.a}</dd>
            </div>
          ))}
        </dl>
      </section>

      <section className="panel keys">
        <h2>Atalhos de teclado</h2>
        <dl className="key-list">
          <div><dt><kbd>1</kbd>–<kbd>5</kbd></dt><dd>Trocar de tela</dd></div>
          <div><dt><kbd>N</kbd></dt><dd>Adicionar pasta</dd></div>
          <div><dt><kbd>R</kbd></dt><dd>Atualizar agora</dd></div>
          <div><dt><kbd>Esc</kbd></dt><dd>Fechar janelas e menus</dd></div>
        </dl>
      </section>
    </div>
  );
}
