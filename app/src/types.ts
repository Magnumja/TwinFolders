// Mirrors core::overview (serde camelCase) and the app's Snapshot enum.

export type FolderHealth =
  | { kind: "upToDate" }
  | { kind: "syncing"; percent: number }
  | { kind: "waitingForPeer" }
  | { kind: "scanning" }
  | { kind: "paused" }
  | { kind: "error"; message: string };

export interface PeerProgress {
  deviceId: string;
  name: string;
  connected: boolean;
  completion: number;
}

export interface FolderView {
  id: string;
  label: string;
  path: string;
  health: FolderHealth;
  needBytes: number;
  globalBytes: number;
  peers: PeerProgress[];
  lastSync: string | null;
}

export interface DeviceView {
  id: string;
  name: string;
  connected: boolean;
  paused: boolean;
  address: string;
  lastSeen: string | null;
  inRate: number;
  outRate: number;
}

export interface PendingDevice {
  deviceId: string;
  name: string;
  address: string;
  time: string;
}

export interface PendingFolder {
  folderId: string;
  offeredBy: string;
  label: string;
  time: string;
}

export interface Overview {
  myId: string;
  myName: string;
  devices: DeviceView[];
  folders: FolderView[];
  pendingDevices: PendingDevice[];
  pendingFolders: PendingFolder[];
}

export type CloudProvider = "iCloud" | "oneDrive" | "dropbox" | "googleDrive" | "other";

/** core::preflight::Report */
export interface PreflightReport {
  entriesScanned: number;
  truncated: boolean;
  invalidNames: { path: string; problem: string }[];
  invalidNamesTotal: number;
  caseCollisions: string[][];
  cloud: CloudProvider | null;
  cloudOnlyFiles: number;
  symlinks: number;
}

export interface Ignores {
  lines: string[];
  error: string | null;
}

/** main.rs Setup (first run). */
export interface SetupInfo {
  os: string;
  syncthing: string | null;
  packageManager: "brew" | "winget" | null;
  installCommand: string | null;
  tailscaleProblem: string | null;
}

export type Snapshot =
  | { status: "ok"; overview: Overview }
  | { status: "notInstalled"; message: string }
  | { status: "notRunning"; message: string };

/** core::resolve::ConflictEntry */
export interface ConflictEntry {
  conflictPath: string;
  originalPath: string;
  deviceShortId: string;
  when: string;
  conflictSize: number;
  originalSize: number | null;
}

export interface FolderConflicts {
  folderId: string;
  label: string;
  conflicts: ConflictEntry[];
  truncated: boolean;
}

export type Resolution = "keepMine" | "keepOther" | "keepBoth";

export interface ResolveOutcome {
  archived: string[];
  renamedTo: string[];
}

/** core::versioning::Choice */
export type VersioningChoice =
  | { kind: "off" }
  | { kind: "trashcan"; days: number }
  | { kind: "simple"; keep: number; days: number }
  | { kind: "staggered"; days: number };

export interface VersioningView {
  choice: VersioningChoice | null;
  kind: string;
}

/** get_profile: local account info for the personal touches in the shell. */
export interface Profile {
  user: string;
  os: string;
  version: string;
}

/** How this computer is used (main.rs Role). */
export type Role = "personal" | "hub" | "member";

export interface Workspace {
  role: Role;
  /** Team computer: the central it belongs to. */
  hubId?: string;
  hubName?: string;
  /** Team computer waiting for the central to approve it. */
  joining?: { code: string; originalName: string };
}

/** office invites on the central (main.rs Invites). */
export interface Invites {
  open: { code: string; expires: number; folders: string[] }[];
  /** Pending device ID -> invite code it carries. */
  matches: Record<string, string>;
}

export interface InvitePreview {
  hubId: string;
  hubName: string;
  expires: number;
}

export type View = "overview" | "folders" | "computers" | "activity" | "pending" | "help";

/** core::activity::Entry — a change in a synced folder. */
export interface ActivityEntry {
  time: string;
  folderId: string;
  folderLabel: string;
  path: string;
  action: "added" | "modified" | "deleted" | string;
  kind: string;
  /** Short device ID of the computer that changed it; absent = this computer. */
  by?: string;
}

/** audit::AuditEntry — something done through the app on this computer. */
export interface AuditEntry {
  at: number;
  action: string;
  detail: string;
}

/** security::Check */
export interface SecurityCheck {
  id: string;
  ok: boolean | null;
  title: string;
  detail: string;
}
