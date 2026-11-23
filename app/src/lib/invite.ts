/** Splits `note-ana [tf:ABCD-EFGH]` (an invited computer waiting, with its proof) into name and proof. */
export function parseInviteTag(name: string): { name: string; code: string | null } {
  const m = /^(.*) \[tf:([A-Z2-7]{4}-[A-Z2-7]{4})\]$/.exec(name);
  return m ? { name: m[1].trim(), code: m[2] } : { name, code: null };
}

/** "vence em 23 h" / "vence em 40 min" for an invite expiry (Unix seconds). */
export function expiresIn(expires: number, nowMs = Date.now()): string {
  const s = Math.max(0, expires - Math.floor(nowMs / 1000));
  if (s < 3600) return `vence em ${Math.max(1, Math.round(s / 60))} min`;
  return `vence em ${Math.round(s / 3600)} h`;
}

export function looksLikeInvite(text: string): boolean {
  return text.replace(/\s/g, "").startsWith("twinfolders-convite:");
}
