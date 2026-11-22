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

export type View = "overview" | "folders" | "computers" | "pending" | "help";
