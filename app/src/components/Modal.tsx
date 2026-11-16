"use client";
import { useEffect, useRef, type ReactNode } from "react";
import { motion } from "motion/react";
import { motionTokens, springs } from "../lib/motion-tokens";
import { useSafeMotion } from "../hooks/useSafeMotion";

const FOCUSABLE = 'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

/**
 * Accessible animated dialog. Render inside <AnimatePresence> at the call
 * site so the exit animation runs. Esc closes, Tab is trapped, page scroll
 * is locked, focus returns to the opener.
 */
export function Modal({ title, onClose, children }: { title: string; onClose: () => void; children: ReactNode }) {
  const ref = useRef<HTMLDivElement>(null);
  const { reduce } = useSafeMotion();

  useEffect(() => {
    const prev = document.activeElement as HTMLElement | null;
    const prevOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    ref.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        onClose();
        return;
      }
      if (e.key !== "Tab" || !ref.current) return;
      const items = Array.from(ref.current.querySelectorAll<HTMLElement>(FOCUSABLE));
      if (items.length === 0) return;
      const first = items[0];
      const last = items[items.length - 1];
      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
      document.body.style.overflow = prevOverflow;
      prev?.focus();
    };
  }, [onClose]);

  const panelFrom = reduce
    ? { opacity: 0 }
    : { opacity: 0, scale: motionTokens.scale.press, y: motionTokens.distance.sm };

  return (
    <motion.div
      className="overlay"
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: motionTokens.duration.fast, ease: motionTokens.easing.smooth }}
      onMouseDown={(e) => e.target === e.currentTarget && onClose()}
    >
      <motion.div
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="modal-title"
        tabIndex={-1}
        ref={ref}
        initial={panelFrom}
        animate={{ opacity: 1, scale: 1, y: 0 }}
        exit={panelFrom}
        transition={reduce ? { duration: motionTokens.duration.fast } : springs.snappy}
      >
        <div className="modal-head">
          <h2 id="modal-title">{title}</h2>
          <button className="icon-btn" onClick={onClose} aria-label="Fechar">✕</button>
        </div>
        {children}
      </motion.div>
    </motion.div>
  );
}
