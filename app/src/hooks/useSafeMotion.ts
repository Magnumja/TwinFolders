"use client";
import { useReducedMotion } from "motion/react";
import { motionTokens } from "../lib/motion-tokens";

/**
 * Enter/exit props that honor prefers-reduced-motion: with reduce on, no
 * transforms at all, only a short opacity fade (motion-foundations rule 3).
 */
export function useSafeMotion(distance: number = motionTokens.distance.md) {
  const reduce = useReducedMotion() ?? false;
  return {
    reduce,
    initial: { opacity: 0, y: reduce ? 0 : distance },
    animate: { opacity: 1, y: 0 },
    exit: { opacity: 0, y: reduce ? 0 : -distance / 2 },
    /** Use for interactive feedback (whileHover/whileTap); disabled on reduce. */
    press: reduce ? undefined : { scale: motionTokens.scale.press },
    hover: reduce ? undefined : { scale: motionTokens.scale.pop },
    fade: { duration: reduce ? motionTokens.duration.fast : motionTokens.duration.normal },
  };
}

/** Container variants for a staggered list (0.05–0.10 s between items). */
export const staggerContainer = {
  hidden: {},
  visible: {
    transition: { staggerChildren: motionTokens.stagger.list, delayChildren: motionTokens.stagger.delay },
  },
};
