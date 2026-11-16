"use client";
import { createContext, useCallback, useContext, useRef, useState, type ReactNode } from "react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { motionTokens, springs } from "../lib/motion-tokens";

type Kind = "ok" | "error" | "info";
interface Toast {
  id: number;
  kind: Kind;
  text: string;
}

const ToastCtx = createContext<(kind: Kind, text: string) => void>(() => {});

/** Feedback for actions (shared, removed, approved, errors). */
export function useToast() {
  return useContext(ToastCtx);
}

const ICON: Record<Kind, string> = { ok: "✓", error: "!", info: "i" };

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const next = useRef(1);
  const reduce = useReducedMotion() ?? false;

  const dismiss = useCallback((id: number) => setToasts((t) => t.filter((x) => x.id !== id)), []);
  const push = useCallback(
    (kind: Kind, text: string) => {
      const id = next.current++;
      setToasts((t) => [...t.slice(-3), { id, kind, text }]);
      setTimeout(() => dismiss(id), kind === "error" ? 8000 : 4000);
    },
    [dismiss],
  );

  const x = reduce ? 0 : motionTokens.distance.xl;
  return (
    <ToastCtx.Provider value={push}>
      {children}
      <div className="toasts" role="status" aria-live="polite">
        <AnimatePresence mode="popLayout">
          {toasts.map((t) => (
            <motion.div
              key={t.id}
              layout={!reduce}
              className={`toast toast-${t.kind}`}
              initial={{ opacity: 0, x, scale: reduce ? 1 : motionTokens.scale.subtle }}
              animate={{ opacity: 1, x: 0, scale: 1 }}
              exit={{ opacity: 0, x, scale: reduce ? 1 : motionTokens.scale.subtle }}
              transition={springs.snappy}
            >
              <span className="toast-icon" aria-hidden="true">{ICON[t.kind]}</span>
              <span className="grow">{t.text}</span>
              <button className="icon-btn" onClick={() => dismiss(t.id)} aria-label="Dispensar">✕</button>
            </motion.div>
          ))}
        </AnimatePresence>
      </div>
    </ToastCtx.Provider>
  );
}
