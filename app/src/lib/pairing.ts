import type { Overview } from "../types";

/**
 * Where the person is in connecting the two computers:
 * - start: nothing yet (add the other computer, or send it this ID)
 * - approveHere: the other computer asked to connect; approve it here
 * - waitingOther: added here, but the other side never connected
 *   (it still has to approve, or it is unreachable)
 * - offline: paired before, just not connected right now
 * - noFolders: connected, but nothing is shared yet
 * - done: connected and sharing
 */
export type PairingStage = "start" | "approveHere" | "waitingOther" | "offline" | "noFolders" | "done";

export function pairingStage(o: Overview): PairingStage {
  if (o.pendingDevices.length > 0 && !o.devices.some((d) => d.connected)) return "approveHere";
  if (o.devices.length === 0) return "start";
  if (o.devices.some((d) => d.connected && !d.paused)) return o.folders.length === 0 ? "noFolders" : "done";
  return o.devices.some((d) => d.lastSeen) ? "offline" : "waitingOther";
}

export function idGroups(id: string): string[] {
  return id.split("-").filter(Boolean);
}

/** Shape check for instant feedback; the backend also verifies the check digits. */
export function looksLikeId(input: string): "empty" | "short" | "long" | "invalid" | "ok" {
  const raw = input.replace(/[\s-]/g, "").toUpperCase();
  if (!raw) return "empty";
  if (/[^A-Z2-7]/.test(raw)) return "invalid";
  if (raw.length < 56) return "short";
  if (raw.length > 56) return "long";
  return "ok";
}
