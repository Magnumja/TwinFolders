"use client";
import type { ReactNode } from "react";
import { AnimatePresence, motion } from "motion/react";
import type { Tone } from "../lib/health";
import { motionTokens, springs } from "../lib/motion-tokens";
import { useSafeMotion } from "../hooks/useSafeMotion";

export function Dot({ tone }: { tone: Tone }) {
  return <span className={`dot dot-${tone}`} aria-hidden="true" />;
}

/** Crossfades when the *meaning* changes, not on every % tick. */
export function SwapText({ swapKey, className, children }: { swapKey: string; className?: string; children: ReactNode }) {
  const m = useSafeMotion(motionTokens.distance.xs);
  return (
    <AnimatePresence mode="wait" initial={false}>
      <motion.span
        key={swapKey}
        className={className}
        initial={m.initial}
        animate={m.animate}
        exit={m.exit}
        transition={{ duration: motionTokens.duration.fast, ease: motionTokens.easing.smooth }}
      >
        {children}
      </motion.span>
    </AnimatePresence>
  );
}

export function PressButton(props: React.ComponentProps<typeof motion.button>) {
  const m = useSafeMotion();
  return <motion.button whileTap={m.press} transition={springs.snappy} {...props} />;
}

/** Progress via scaleX (transform only) on a non-overshooting spring. */
export function Progress({ percent, tone, thin }: { percent: number; tone: Tone; thin?: boolean }) {
  const m = useSafeMotion();
  return (
    <div
      className={`bar bar-${tone}${thin ? " bar-thin" : ""}`}
      role="progressbar"
      aria-valuenow={Math.round(percent)}
      aria-valuemin={0}
      aria-valuemax={100}
    >
      <motion.span
        initial={false}
        animate={{ scaleX: Math.max(0, Math.min(100, percent)) / 100 }}
        transition={m.reduce ? { duration: 0 } : springs.progress}
      />
    </div>
  );
}

export function Empty({ icon, title, text, action }: { icon: ReactNode; title: string; text: string; action?: ReactNode }) {
  const m = useSafeMotion(motionTokens.distance.sm);
  return (
    <motion.div className="empty" initial={m.initial} animate={m.animate} transition={m.reduce ? m.fade : springs.gentle}>
      <span className="empty-icon" aria-hidden="true">{icon}</span>
      <strong>{title}</strong>
      <p className="muted">{text}</p>
      {action}
    </motion.div>
  );
}

/* ---- Icons: 1.75px rounded strokes, matching the logo's line ---- */
type IconProps = { size?: number };
const svg = (size: number, children: ReactNode) => (
  <svg viewBox="0 0 24 24" width={size} height={size} fill="none" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    {children}
  </svg>
);
export const IconGrid = ({ size = 18 }: IconProps) => svg(size, <><rect x="4" y="4" width="6.5" height="6.5" rx="1.5" /><rect x="13.5" y="4" width="6.5" height="6.5" rx="1.5" /><rect x="4" y="13.5" width="6.5" height="6.5" rx="1.5" /><rect x="13.5" y="13.5" width="6.5" height="6.5" rx="1.5" /></>);
export const IconFolder = ({ size = 18 }: IconProps) => svg(size, <path d="M3.5 7.5v10a2 2 0 0 0 2 2h13a2 2 0 0 0 2-2v-8a2 2 0 0 0-2-2h-7l-2-2.5h-4a2 2 0 0 0-2 2z" />);
export const IconMonitor = ({ size = 18 }: IconProps) => svg(size, <><rect x="3" y="4" width="18" height="12" rx="2" /><path d="M8 20h8M12 16v4" /></>);
export const IconLaptop = ({ size = 18 }: IconProps) => svg(size, <><rect x="5" y="5" width="14" height="10" rx="1.5" /><path d="M2.5 19h19" /></>);
export const IconInbox = ({ size = 18 }: IconProps) => svg(size, <><path d="M4 13.5 6.2 6a2 2 0 0 1 1.9-1.5h7.8A2 2 0 0 1 17.8 6L20 13.5V18a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2z" /><path d="M4 13.5h4.5l1.5 2h4l1.5-2H20" /></>);
export const IconShield = ({ size = 18 }: IconProps) => svg(size, <><path d="M12 3.5 5 6v5.5c0 4.2 3 7.6 7 9 4-1.4 7-4.8 7-9V6z" /><path d="m9 12 2 2 4-4" /></>);
export const IconGear = ({ size = 18 }: IconProps) => svg(size, <><circle cx="12" cy="12" r="3" /><path d="M12 2.5v3M12 18.5v3M4.2 6.5l2.1 1.7M17.7 15.8l2.1 1.7M2.5 13.5l2.7-.8M18.8 11.3l2.7-.8M4.2 17.5l2.1-1.7M17.7 8.2l2.1-1.7" /></>);
export const IconRefresh = ({ size = 18 }: IconProps) => svg(size, <><path d="M20 12a8 8 0 0 1-14 5.3M4 12a8 8 0 0 1 14-5.3" /><path d="M18 3v4h-4M6 21v-4h4" /></>);
export const IconSearch = ({ size = 16 }: IconProps) => svg(size, <><circle cx="11" cy="11" r="6" /><path d="m20 20-4.2-4.2" /></>);
export const IconPlus = ({ size = 16 }: IconProps) => svg(size, <path d="M12 5v14M5 12h14" />);
export const IconMore = ({ size = 18 }: IconProps) => svg(size, <><circle cx="6" cy="12" r="0.8" /><circle cx="12" cy="12" r="0.8" /><circle cx="18" cy="12" r="0.8" /></>);
export const IconDown = ({ size = 16 }: IconProps) => svg(size, <path d="M12 5v14m-5-5 5 5 5-5" />);
export const IconUp = ({ size = 16 }: IconProps) => svg(size, <path d="M12 19V5m-5 5 5-5 5 5" />);
export const IconLayers = ({ size = 16 }: IconProps) => svg(size, <><path d="m12 4 8 4-8 4-8-4z" /><path d="m4 12 8 4 8-4M4 16l8 4 8-4" /></>);
export const IconSwap = ({ size = 22 }: IconProps) => svg(size, <path d="M4 9h14l-3.5-3.5M20 15H6l3.5 3.5" />);
export const IconCopy = ({ size = 14 }: IconProps) => svg(size, <><rect x="8" y="8" width="12" height="12" rx="2" /><path d="M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2" /></>);
export const IconLock = ({ size = 14 }: IconProps) => svg(size, <><rect x="5" y="11" width="14" height="9" rx="2" /><path d="M8 11V8a4 4 0 0 1 8 0v3" /></>);
export const IconHelp = ({ size = 18 }: IconProps) => svg(size, <><circle cx="12" cy="12" r="8.5" /><path d="M9.6 9.4a2.5 2.5 0 0 1 4.8.9c0 1.7-2.4 2.2-2.4 3.7M12 17h.01" /></>);
export const IconPulse = ({ size = 18 }: IconProps) => svg(size, <path d="M3 12h4l2.5-6 5 12 2.5-6h4" />);
export const IconAlert = ({ size = 18 }: IconProps) => svg(size, <><path d="M12 4 2.5 20h19z" /><path d="M12 10v4M12 17h.01" /></>);
