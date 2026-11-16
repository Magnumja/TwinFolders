const nf = new Intl.NumberFormat("pt-BR", { maximumFractionDigits: 1 });
const UNITS = ["B", "KB", "MB", "GB", "TB"];

/** Binary units (1 KB = 1024 B), pt-BR decimal comma. */
export function formatBytes(n: number): string {
  let v = Math.max(0, n);
  let i = 0;
  while (v >= 1024 && i < UNITS.length - 1) {
    v /= 1024;
    i++;
  }
  return `${nf.format(i === 0 ? Math.round(v) : v)} ${UNITS[i]}`;
}

/** Transfer rate; "—" when effectively idle. */
export function formatRate(bytesPerSecond: number): string {
  return bytesPerSecond < 1 ? "—" : `${formatBytes(bytesPerSecond)}/s`;
}

/** "agora", "há 5 min", "há 2 h", "há 3 dias"; "nunca" when missing. */
export function relativeTime(iso: string | null, now: Date = new Date()): string {
  if (!iso) return "nunca";
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return "nunca";
  const s = Math.max(0, (now.getTime() - t) / 1000);
  if (s < 60) return "agora";
  const min = Math.floor(s / 60);
  if (min < 60) return `há ${min} min`;
  const h = Math.floor(min / 60);
  if (h < 24) return `há ${h} h`;
  const d = Math.floor(h / 24);
  return d === 1 ? "há 1 dia" : `há ${d} dias`;
}

/** First block of a Syncthing device ID (what Syncthing itself shows). */
export function shortId(deviceId: string): string {
  return deviceId.split("-")[0] ?? deviceId;
}
