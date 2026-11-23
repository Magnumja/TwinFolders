"use client";
import { useEffect, useRef, useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import { motionTokens, springs } from "../lib/motion-tokens";
import { IconMore } from "./ui";

export type MenuItem = { label: string; onSelect: () => void; danger?: boolean } | "separator";

/**
 * The "⋯" actions menu used by folder rows and computer cards: one trigger,
 * items grouped by topic with separators, destructive ones last and red.
 * Opens from the trigger's corner with a quick spring; Esc or a click
 * outside closes it.
 */
export function ActionMenu({ label, items }: { label: string; items: MenuItem[] }) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent | KeyboardEvent) => {
      if (e instanceof KeyboardEvent ? e.key === "Escape" : !ref.current?.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener("mousedown", close);
    window.addEventListener("keydown", close);
    return () => {
      window.removeEventListener("mousedown", close);
      window.removeEventListener("keydown", close);
    };
  }, [open]);
  return (
    <div className="row-menu" ref={ref}>
      <button className="icon-btn" aria-label={label} aria-haspopup="menu" aria-expanded={open} onClick={() => setOpen((o) => !o)}>
        <IconMore />
      </button>
      <AnimatePresence>
        {open && (
          <motion.div
            className="menu"
            role="menu"
            initial={{ opacity: 0, scale: motionTokens.scale.subtle, y: -4 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: motionTokens.scale.subtle, y: -4 }}
            transition={springs.instant}
          >
            {items.map((it, i) =>
              it === "separator" ? (
                <hr key={`sep-${i}`} />
              ) : (
                <button
                  key={it.label}
                  role="menuitem"
                  className={it.danger ? "danger" : undefined}
                  onClick={() => {
                    setOpen(false);
                    it.onSelect();
                  }}
                >
                  {it.label}
                </button>
              ),
            )}
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
