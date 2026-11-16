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

export interface Overview {
  myId: string;
  devices: DeviceView[];
  folders: FolderView[];
}

export type Snapshot =
  | { status: "ok"; overview: Overview }
  | { status: "notInstalled"; message: string }
  | { status: "notRunning"; message: string };
