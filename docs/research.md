# TwinFolders - Research (official docs / source)

Researched 2026-11-16. Language: English. Items I could not confirm: **UNVERIFIED**.
Short refs: `ST-docs` = https://docs.syncthing.net ; `ST-src` = https://github.com/syncthing/syncthing (main) ; `ST-docsrc` = https://github.com/syncthing/docs (rst sources, raw).

## 0. Versions found

| Item | Version | Source |
|---|---|---|
| Syncthing latest release | v2.1.7 (2026-10-21); docs written for v2.1.0 | https://api.github.com/repos/syncthing/syncthing/releases/latest |
| Tailscale latest | v1.104.7 | https://api.github.com/repos/tailscale/tailscale/releases/latest |
| tauri | 2.12.5 | https://crates.io/crates/tauri |
| tauri-build | 2.7.4 | https://crates.io/crates/tauri-build |
| tauri-plugin-autostart | 2.7.0 | https://crates.io/crates/tauri-plugin-autostart |
| tauri-plugin-notification | 2.8.1 | https://crates.io/crates/tauri-plugin-notification |
| tauri-plugin-opener | 2.7.3 | https://crates.io/crates/tauri-plugin-opener |
| tauri-plugin-shell | 2.6.0 | https://crates.io/crates/tauri-plugin-shell |
| tauri-plugin-single-instance | 2.5.5 | https://crates.io/crates/tauri-plugin-single-instance |
| keyring | 4.2.0 | https://crates.io/crates/keyring |
| reqwest | 0.13.5 | https://crates.io/crates/reqwest |
| serde / serde_json | 1.0.229 / 1.0.154 | https://crates.io/crates/serde |
| mockito / wiremock / httpmock | 1.7.2 / 0.6.5 / 0.9.0 | https://crates.io/crates/mockito , /wiremock , /httpmock |
| tokio | 1.54.1 | https://crates.io/crates/tokio |
| Tauri plugins MSRV | Rust >= 1.90 (stated on plugin pages) | https://v2.tauri.app/plugin/autostart/ |

---

## 1. Syncthing v2

### 1.1 What changed in v2 (vs v1) - https://github.com/syncthing/syncthing/releases/tag/v2.0.0

- DB backend LevelDB -> SQLite (long first-launch migration possible).
- CLI parsing modernised: **single-dash long options are gone** (`-home` -> `--home`, `-no-browser` -> `--no-browser`). All `serve` options also exist as env vars (`STxxx`). `--verbose`/`--logflags` removed; new `--log-level` (DEBUG/INFO/WARN/ERROR).
- **No default folder is created on first start** (so `STNODEFAULTFOLDER` is not needed; I found no reference to it in v2 source - UNVERIFIED that it is fully obsolete, but harmless).
- Deleted items forgotten after ~6 months (`--db-delete-retention-interval`, default 10920h; 0 = forever).
- Multiple connections between v2 devices by default (3: 1 index + 2 data).
- Conflict resolution: a *delete* can now win; the modified file then becomes a conflict copy.
- Config: `folder.group`, `device.group`, `folder.blockIndexing` added in 2.1.0 (https://docs.syncthing.net/users/config.html).
- Versioning: `fsPath`, `fsType`, `cleanupIntervalS` are top-level `versioning` fields (https://docs.syncthing.net/users/versioning.html).
- `/rest/system/config` and `/rest/system/config/insync` are deprecated -> use `/rest/config` (https://docs.syncthing.net/dev/rest.html).
- No prebuilt binaries for windows/arm etc. (windows/amd64, windows/arm64, darwin/arm64 remain).

### 1.2 Auth, config location

| Item | Value | URL |
|---|---|---|
| API key header | `X-API-Key: <key>`; alternatively `Authorization: Bearer <key>` | https://docs.syncthing.net/dev/rest.html |
| No-auth endpoint | `GET /rest/noauth/health` | same |
| API key in config | `<configuration><gui><apikey>...` (`gui.apiKey` in REST JSON) | same; ST-src lib/config/guiconfiguration.go |
| Override key | `--gui-apikey=KEY` / `STGUIAPIKEY` | https://docs.syncthing.net/users/syncthing.html |
| Default GUI/REST address | `127.0.0.1:8384` (HTTP unless `gui.tls`/`useTLS`; self-signed HTTPS needs `-k` in curl) | https://docs.syncthing.net/users/guilisten.html |
| Home dir macOS | `$HOME/Library/Application Support/Syncthing` | https://docs.syncthing.net/users/config.html ; ST-src lib/locations/locations.go |
| Home dir Windows | `%LOCALAPPDATA%\Syncthing` (fallback `%AppData%\Syncthing`) | same |
| Home dir Linux (FYI) | `$XDG_STATE_HOME/syncthing` or `~/.local/state/syncthing` (since 1.27) | same |
| Files in home | `config.xml`, `cert.pem`, `key.pem`, `https-cert.pem`, `https-key.pem`, DB (SQLite) | same |
| Pagination | some GETs accept `page`, `perpage` (default 65536) | rest.html |

Notes: REST config JSON uses Go json tags (ST-src lib/config/*.go): `deviceID`, `addresses`, `devices[].deviceID` - **not** the XML names (`id`, `address`). `gui.password` plaintext is hashed server-side.

### 1.3 REST endpoints (shape)

Base: `http://127.0.0.1:8384`, header `X-API-Key`. Index: https://docs.syncthing.net/dev/rest.html ; per-endpoint pages `https://docs.syncthing.net/rest/<name>.html`.

| Endpoint | Request | Response / key fields | URL |
|---|---|---|---|
| `GET /rest/system/status` | - | `myID` (own device ID), `startTime`, `uptime`, `alloc`, `sys`, `goroutines`, `discoveryEnabled`, `discoveryStatus{<method>:{error}}`, `connectionServiceStatus{<listener>:{error,lanAddresses,wanAddresses}}`, `pathSeparator`, `tilde`, `themes`; `cpuPercent` always 0 (deprecated); `discoveryErrors/discoveryMethods` deprecated | /rest/system-status-get.html (+ rst in ST-docsrc rest/system-status-get.rst) |
| `GET /rest/system/ping` (also POST) | - | `{"ping":"pong"}` | /rest/system-ping-get.html |
| `GET /rest/system/connections` | - | `{"connections":{<deviceID>:{address,at,clientVersion,connected,inBytesTotal,outBytesTotal,isLocal,paused,startedAt,type}},"total":{at,inBytesTotal,outBytesTotal}}`; type in tcp-client/server, relay-client/server, quic-client/server; disconnected devices have empty `address`/`type` and zero time | /rest/system-connections-get.html |
| `POST /rest/system/pause?device=ID` | `device` optional (omit = all devices) | 200 empty body; 500 text error | /rest/system-pause-post.html |
| `POST /rest/system/resume?device=ID` | same | same | /rest/system-resume-post.html (UNVERIFIED page text; symmetric with pause per index) |
| `POST /rest/db/scan` | `folder` (omit/empty = all), `sub` (path in folder), `next` (seconds to delay next auto rescan) | 200 empty; 500 text | /rest/db-scan-post.html |
| `GET /rest/db/completion` | `folder`, `device` (both optional; no device = local; no folder = aggregate) | `completion` (0-100), `globalBytes`, `needBytes`, `globalItems`, `needItems`, `needDeletes`, `remoteState` (`paused`/`notSharing`/`valid`/`unknown`, v1.20+), `sequence` | /rest/db-completion-get.html |
| `GET /rest/db/status?folder=ID` | `folder` required. **Expensive** (docs warn) | `state` (`idle`/`scanning`/`syncing`/`error`...), `stateChanged`, `globalBytes/Files/Directories/Deleted/TotalItems`, `localBytes/Files/...`, `inSyncBytes/Files`, `needBytes/Files/Directories/Deletes/TotalItems`, `receiveOnlyChanged*`, `pullErrors`, `ignorePatterns`, `sequence`; `invalid`,`version` deprecated | /rest/db-status-get.html |
| `GET /rest/cluster/pending/devices` | - | `{<deviceID>:{time,name,address}}` (since 1.13); `DELETE ?device=ID` dismisses | /rest/cluster-pending-devices-get.html |
| `GET /rest/cluster/pending/folders` | optional `device` | `{<folderID>:{offeredBy:{<deviceID>:{time,label,receiveEncrypted,remoteEncrypted}}}}`; `DELETE ?folder=&device=` | /rest/cluster-pending-folders-get.html |
| `GET /rest/events` | `since` (default 0), `events` (comma list; default all **except** LocalChangeDetected/RemoteChangeDetected), `timeout` (s, default 60; returns `[]` on timeout), `limit` (last n events; use `?since=0&limit=1` to learn the latest id) | array of events | /rest/events-get.html |
| `GET /rest/config` | - | full config (`folders`, `devices`, `gui`, `options`, `ldap`, `defaults`) | /rest/config.html |
| `GET /rest/config/devices`, `/folders` | - | array | same |
| `PUT /rest/config/devices` (or folders) | **array** (replaces/adds by ID) | 200 | same |
| `POST /rest/config/devices` (or folders) | **single object**; same ID = replaced, else added | 200 | same |
| `GET/PUT/PATCH/DELETE /rest/config/folders/{id}` (and `/devices/{id}`) | PUT = replace; PATCH = unmarshals JSON over existing (child objects replace, not merge; e.g. patching `versioning` replaces whole object, patching `listenAddresses` replaces the list) | | same |
| `GET /rest/config/defaults/folder` / `defaults/device` | template needing only a unique ID | | same |
| `GET/PUT/PATCH /rest/config/options` (also `/gui`, `/ldap`) | | | same |
| `GET /rest/config/restart-required` | | `{requiresRestart: bool}` (UNVERIFIED exact key) | same |

Posted config takes effect immediately except restart-required settings (e.g. `listenAddresses` changes need restart? - UNVERIFIED; check `restart-required`). Event IDs: `id` increases by 1 per event (gap = lost events), `globalID` >= id, `time` RFC3339, `type`, `data` (nullable).

### 1.4 Folder config (REST JSON; defaults from ST-src lib/config/folderconfiguration.go)

| Field | Type / values | Default |
|---|---|---|
| `id` | string, unique (mandatory) | - |
| `label` | string, local only | "" |
| `path` | string, local only (mandatory); trailing slash ok | - |
| `type` | `sendreceive` / `sendonly` / `receiveonly` / `receiveencrypted` (old `readwrite`/`readonly` accepted on input) | sendreceive |
| `devices` | `[{"deviceID":"...","introducedBy":"","encryptionPassword":""}]` - must include own device ID (Syncthing adds it if missing) | - |
| `rescanIntervalS` | int s, 0 disables | 3600 |
| `fsWatcherEnabled` / `fsWatcherDelayS` / `fsWatcherTimeoutS` | bool / s / s | true / 10 / 0 |
| `ignorePerms` | bool - **set true on Windows<->Mac pair? consider** | false |
| `maxConflicts` | int; 10 default in code; `-1` unlimited; `0` disables conflict copies. (Docs sample shows -1, defaults show 10: inconsistent) | 10 |
| `paused`, `markerName` (`.stfolder`), `minDiskFree` (`1 %`), `copiers`, `hashers`, `order`, `ignoreDelete` ... | | |
| `versioning` | `{type, params{k:v strings}, cleanupIntervalS, fsPath, fsType}` | type "" = off; cleanupIntervalS 3600; fsType basic |
| v2.1+: `group`, `blockIndexing` (default true) | | |

Versioning (https://docs.syncthing.net/users/versioning.html) - all `params` values are **strings**:

| type | params | notes |
|---|---|---|
| `trashcan` | `cleanoutDays` ("0" = keep forever) | |
| `simple` | `keep` (number of versions), `cleanoutDays` | |
| `staggered` | `maxAge` (docs inconsistent: days in prose vs seconds in reference - UNVERIFIED unit; ST-src lib/versioner staggered treats value as seconds in the param but GUI shows days; test before relying) | intervals fixed: 30 s/1 h, 1 h/1 day, 1 day/30 days, 1 week/until maxAge |
| `external` | `command` (`%FOLDER_PATH%`, `%FILE_PATH%`; v2.1.2+ env `$FOLDER_PATH`/`$FILE_PATH`) | cleanup fields ignored |
| shared | `fsPath` (default `.stversions`; relative to folder), `fsType`, `cleanupIntervalS` (0 disables; max 31536000) | |

Example: `{"versioning":{"type":"staggered","params":{"maxAge":"31536000"},"cleanupIntervalS":3600,"fsPath":"","fsType":"basic"}}`

### 1.5 Device config (ST-src lib/config/deviceconfiguration.go)

`deviceID`, `name`, `addresses` (array; `"dynamic"` = discovery; static e.g. `"tcp://127.0.0.1:22001"`), `compression` (`metadata`/`always`/`never`), `introducer`, `paused`, `autoAcceptFolders`, `untrusted`, `maxSendKbps`, `maxRecvKbps`, `allowedNetworks`, `ignoredFolders`, `numConnections`, `group` (2.1+).

### 1.6 Event payloads (ST-docsrc events/*.rst; https://docs.syncthing.net/events/<lowercase>.html)

| Event | `data` fields |
|---|---|
| StateChanged | `folder`, `from`, `to` (`idle`/`scanning`/`syncing`/`error`), `duration` (s in `from`) |
| FolderCompletion | `folder`, `device`, `completion`, `globalBytes`, `globalItems`, `needBytes`, `needItems`, `needDeletes`, `sequence`, `remoteState` - one event per connected remote device |
| FolderSummary | `folder`, `summary{...same as db/status...}` (`state`, `needBytes`, `errors`, `pullErrors`, `watchError`, `remoteSequence{dev:seq}` ...) (example has no `globalID`) |
| DeviceConnected | `addr`, `id` (also `deviceName`, `clientName`, `clientVersion`, `type` in practice - UNVERIFIED; docs show only addr,id) |
| DeviceDisconnected | `id`, `error` (EOF / unexpected EOF = normal peer offline) |
| PendingDevicesChanged (1.14+) | `added[{deviceID,name,address}]`, `removed[{deviceID}]` |
| PendingFoldersChanged (1.14+) | `added[{deviceID,folderID,folderLabel,receiveEncrypted,remoteEncrypted}]`, `removed[{folderID,deviceID?}]` (no deviceID = no longer pending for anyone). Docs example JSON is malformed (missing commas, bool as string) - parse leniently |
| LocalIndexUpdated | `folder`, `items`, `filenames[]`, `sequence` (`version` deprecated) |
| ItemFinished | `item`, `folder`, `type` (file...), `action` (`update`/`metadata`/`delete`), `error` (null on success) |
| FolderErrors | `folder`, `errors[{path,error}]`; list obsolete once state -> syncing |

Other useful events present in docs repo: FolderPaused/Resumed, DevicePaused/Resumed, FolderScanProgress, DownloadProgress, ConfigSaved, ListenAddressesChanged, StartupComplete, Failure.

### 1.7 CLI / headless / isolated test

Source: https://docs.syncthing.net/users/syncthing.html ; ST-src cmd/syncthing/main.go.

```
syncthing serve --home=<dir> --gui-address=127.0.0.1:8385 --gui-apikey=<key> \
  --no-browser --no-upgrade --no-restart --no-port-probing [--no-console on Windows] [--paused]
syncthing generate --home=<dir> [--gui-user=U --gui-password=P|-]   # creates keys + config.xml
syncthing device-id --home=<dir>          # print own ID
syncthing paths | version | cli
```
- `serve` is the default subcommand. `--home/-H` (env `STHOMEDIR`) or `--config/-C` + `--data/-D`.
- `--gui-address` accepts `host:port` or URL (`http`, `https`, `unix`, `unixs`); env `STGUIADDRESS`. `--gui-apikey` env `STGUIAPIKEY`.
- `--no-restart` / `STNORESTART`: don't restart on exit via API/GUI/upgrade/crash (monitor still handles log files). `--no-upgrade` / `STNOUPGRADE`. `--no-port-probing` / `STNOPORTPROBING` (don't pick random ports on first config creation - set it, otherwise 8384/22000 may be moved). `--paused` starts everything paused. `--no-console` Windows only.
- `STNODEFAULTFOLDER`: **not in v2 docs/source** (no default folder in v2).
- Graceful stop: `POST /rest/system/shutdown`.

Isolated two-daemon localhost test (no external network). In each `config.xml` (or via `PATCH /rest/config/options` right after start; edit file before first start to avoid any leak):

| Setting | JSON `options.*` | XML |
|---|---|---|
| Disable global discovery | `globalAnnounceEnabled:false` | `<globalAnnounceEnabled>` |
| Disable local discovery | `localAnnounceEnabled:false` | `<localAnnounceEnabled>` |
| Disable relays | `relaysEnabled:false` | `<relaysEnabled>` |
| Disable NAT-PMP/UPnP | `natEnabled:false` | `<natEnabled>` |
| Listen address | `listenAddresses:["tcp://127.0.0.1:22001"]` (default is `"default"` = tcp+quic 22000 + dynamic relay pool) | `<listenAddress>tcp://127.0.0.1:22001</listenAddress>` |
| Don't open browser | `startBrowser:false` | |
| Disable usage report / crash reports | `urAccepted:-1`, `crashReportingEnabled:false` | |
| Device static address | peer device `addresses:["tcp://127.0.0.1:22002"]` | `<address>` |
| Disable auto-upgrade | `autoUpgradeIntervalH:0` (also `--no-upgrade`) | |

Pick distinct GUI ports (8385/8386), listen ports (22001/22002) and `--home` dirs. API key: read from `config.xml` `gui/apikey` after `generate`, or force it with `--gui-apikey`. Wait for `GET /rest/system/ping` before issuing calls.

### 1.8 Conflicts

- Name: `<filename>.sync-conflict-<date>-<time>-<modifiedBy>.<ext>` (https://docs.syncthing.net/users/syncing.html). Source (ST-src lib/model/folder_sendrecv.go `conflictName`): `name_without_ext + time.Now().Format(".sync-conflict-20060102-150405-") + modifiedBy + ext` -> `report.sync-conflict-20261116-093012-ABCDEFG.docx`. `<date>` = YYYYMMDD, `<time>` = HHMMSS **local time of the resolving device**, `<modifiedBy>` = **short device ID, 7 chars base32** (`protocol.ShortIDStringLength = 7`, first 64 bits of device ID) of the device that last modified the losing file. Files with no ext: `name.sync-conflict-...-ABCDEFG`. Glob used by Syncthing: `.sync-conflict-????????-??????*`.
- Winner: older mtime loses (renamed); equal mtime -> device with larger first-63-bits ID loses. Delete-vs-modify: if delete wins, modified file renamed to conflict copy (v2).
- Conflict files are normal files and **sync to all devices**.
- `maxConflicts` is **per folder** (not in `options`): default 10, -1 unlimited, 0 = no conflict copies.
- Suggested regex: `^(?P<base>.+)\.sync-conflict-(?P<date>\d{8})-(?P<time>\d{6})-(?P<by>[A-Z2-7]{7})(?P<ext>\.[^.]*)?$`

### 1.9 .stignore (https://docs.syncthing.net/users/ignoring.html)

- `//` comment line (only at line start; mid-line `//` is literal).
- `!pattern` include/negate; first matching rule wins, so put `!` before broader rule.
- `(?i)` case-insensitive (macOS/Windows always insensitive); `(?d)` allow deleting these files if they block deleting a dir (e.g. `(?d).DS_Store`, `(?d)Thumbs.db`). Combine as `(?d)(?i)pattern`, **not** `(?di)`.
- Leading `/` anchors to folder root. `*` (no separator), `**` (crosses `/`), `?`, `[a-z]`, `{a,b}`. Escape `\`; on Windows use `|` or put `#escape=\` on top of file.
- `#include other.txt` (relative to folder root; error if missing or included twice).
- Default ignore patterns (since 1.19): `GET/PUT /rest/config/defaults/ignores` `{"lines":[...]}`; applied on auto-accepted folders; used to create `.stignore` if absent. Read/write per folder: `GET/POST /rest/db/ignores?folder=ID` (body `{"ignore":[...]}` - UNVERIFIED body key).
- `.stignore` itself is not synced.

### 1.10 Background run (https://docs.syncthing.net/users/autostart.html)

| OS | Official recommendation |
|---|---|
| macOS | Homebrew: `brew install syncthing`, follow printed instructions (docs mention launchctl only; `brew services start syncthing` is the usual formula route - not in docs, UNVERIFIED). Manual: copy `etc/macos-launchd/syncthing.plist` (in release tarball) to `~/Library/LaunchAgents`, replace the 4 `/Users/USERNAME` occurrences, `launchctl load ~/Library/LaunchAgents/syncthing.plist` (modern: `launchctl bootstrap gui/$(id -u) <plist>`). Disable "Start Browser". For TwinFolders: write own LaunchAgent plist with `ProgramArguments` = binary, `serve --home ... --no-browser --no-restart`. |
| Windows | **Task Scheduler** "At log on" for the user: program `syncthing.exe`, args `--no-console --no-browser`; uncheck "Stop task if it runs longer than"; "Run whether user is logged on or not" needs password. Start once interactively for the firewall prompt. Do not use "At startup" if folders are on BitLocker non-system volume. Alternative: `shell:startup` shortcut. Service (NSSM) only for headless servers (`--no-restart --no-browser --home=...`; exit codes 3,4 = restart). Ending the task only stops the monitor - use REST shutdown. |

---

## 2. Tailscale

### 2.1 `tailscale status --json`

Source: https://github.com/tailscale/tailscale/blob/main/ipn/ipnstate/ipnstate.go ; docs warn "format subject to change" (https://tailscale.com/kb/1080/cli). Flags: `--json`, `--peers` (default true), `--self` (default true).

Top level: `Version`, `TUN`, `BackendState` (`NoState`, `NeedsLogin`, `NeedsMachineAuth`, `Stopped`, `Starting`, `Running`), `HaveNodeKey`, `AuthURL`, `TailscaleIPs[]`, `Self` (PeerStatus), `Health[]`, `MagicDNSSuffix` (legacy), `CurrentTailnet{Name,MagicDNSSuffix,MagicDNSEnabled,StableID}` (null when not connected), `CertDomains`, `Peer` (map keyed by node **public key** -> PeerStatus), `User` (map).
PeerStatus: `ID`, `PublicKey`, `HostName`, `DNSName` (FQDN with trailing dot, e.g. `pc.tail1234.ts.net.`), `OS`, `UserID`, `TailscaleIPs[]`, `Tags`, `Addrs`, `CurAddr`, `Relay`, `RxBytes`, `TxBytes`, `Created`, `LastWrite`, `LastSeen` (time of last seen, only meaningful when offline; zero value `0001-01-01T00:00:00Z` when online), `LastHandshake`, `Online` (bool), `ExitNode`, `Active`, `Expired`, `KeyExpiry`, `sshHostKeys[]`.

### 2.2 Tailscale SSH server support (https://tailscale.com/kb/1193/tailscale-ssh , https://tailscale.com/kb/1065/macos-variants)

- Server component only on **Linux** and **macOS open-source `tailscale`+`tailscaled` CLI** variant. **Not Windows** (Windows is client-only; page never lists it as a server). Confirms the assumption.
- macOS table: App Store - cannot be SSH server, no `tailscale ssh` command; Standalone - cannot be SSH server (but can be client); open-source tailscaled - can be server. All can be clients.
- Consequence: for the Mac GUI app (App Store/Standalone) use **Remote Login + regular OpenSSH** over the tailnet IP/MagicDNS name; Windows -> OpenSSH Server. Tailscale SSH is not an option for either in TwinFolders.

### 2.3 Wake-on-LAN

- No `tailscale wol` command exists in CLI source (404 on cmd/tailscale/cli/wol.go). Tracking issue "Wake-on-LAN packet support" tailscale/tailscale#306 is **open** (checked via API). Tailscale's own guidance (blog 2025-08-19) is to run a WoL relay (e.g. UpSnap) on the target LAN: https://tailscale.com/blog/wake-on-lan-tailscale-upsnap . A `hostinfo/wol.go` exists in a mirror (peer WoL MAC advertisement) - status UNVERIFIED. Treat WoL as **not supported** natively. Also: https://github.com/tailscale/tailscale/issues/306

### 2.4 CLI paths

| OS | Path | URL |
|---|---|---|
| macOS App Store | `/Applications/Tailscale.app/Contents/MacOS/Tailscale <cmd>` | https://tailscale.com/kb/1080/cli |
| macOS Standalone | "Install CLI integration" in settings -> launcher at `/usr/local/bin/tailscale` (macOS 13+) | same |
| macOS Homebrew/open-source | `tailscale` + `tailscaled` in PATH | https://tailscale.com/kb/1065/macos-variants |
| Windows | `tailscale.exe` run from cmd/PowerShell. Install dir `C:\Program Files\Tailscale\tailscale.exe` is the conventional location but **UNVERIFIED** (not on fetched KB pages) | https://tailscale.com/kb/1080/cli |

Windows unattended mode (device reachable after reboot without login): https://tailscale.com/kb/1095 text says enable "Unattended mode" else a user must log in after each reboot (page content partially retrieved).

---

## 3. Tauri v2 (tauri 2.12.5)

### 3.1 Tray (https://v2.tauri.app/learn/system-tray/)
- Cargo: `tauri = { version = "2", features = ["tray-icon"] }`.
- Rust: `use tauri::tray::{TrayIconBuilder, TrayIconEvent, MouseButton, MouseButtonState}; use tauri::menu::{Menu, MenuItem};`
```rust
let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
let menu = Menu::with_items(app, &[&quit])?;
TrayIconBuilder::new()
  .icon(app.default_window_icon().unwrap().clone())
  .menu(&menu)
  .show_menu_on_left_click(false)
  .on_menu_event(|app, ev| match ev.id.as_ref() { "quit" => app.exit(0), _ => {} })
  .on_tray_icon_event(|tray, ev| if let TrayIconEvent::Click{button:MouseButton::Left, button_state:MouseButtonState::Up, ..} = ev { /* show+focus window */ })
  .build(app)?;
```
- Menu shows on left+right click by default. **On Linux tray events are not emitted** (irrelevant for Win/Mac). JS: `TrayIcon.new`, `menuOnLeftClick`.
- Keep app alive with no windows: handle `WindowEvent::CloseRequested` -> `api.prevent_close(); window.hide()` (standard pattern; not on that page).

### 3.2 Plugins

| Plugin | Setup | Notes / URL |
|---|---|---|
| autostart | `cargo add tauri-plugin-autostart --target 'cfg(any(target_os="macos",windows,target_os="linux"))'`; in `setup` under `#[cfg(desktop)]`: `app.handle().plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec!["--minimized"])))`; Rust `use tauri_plugin_autostart::ManagerExt; app.autolaunch().enable()/is_enabled()/disable()` | `MacosLauncher::{LaunchAgent (default; plist in ~/Library/LaunchAgents), AppleScript (login item via System Events)}` (source plugins-workspace/plugins/autostart/src/lib.rs). Perms: `autostart:allow-enable`, `allow-disable`, `allow-is-enabled` (`autostart:default` = all three). https://v2.tauri.app/plugin/autostart/ |
| notification | `cargo add tauri-plugin-notification`; `.plugin(tauri_plugin_notification::init())`; `use tauri_plugin_notification::NotificationExt; app.notification().builder().title("..").body("..").show()?` | Windows: only works for **installed** apps (dev shows PowerShell name/icon). `notification:default`. JS has isPermissionGranted/requestPermission; Rust example has no permission check. https://v2.tauri.app/plugin/notification/ |
| opener (replaces shell `open`) | `cargo add tauri-plugin-opener`; `.plugin(tauri_plugin_opener::init())`; `use tauri_plugin_opener::OpenerExt; app.opener().open_path(p, None::<&str>)`, `.open_url(url, None::<&str>)` | Perms: `opener:allow-open-path`, `allow-open-url`, `allow-reveal-item-in-dir`, `allow-default-urls`; scopes `{"path":..}`/`{"url":..}` globs. https://v2.tauri.app/plugin/opener/ |
| shell | `cargo add tauri-plugin-shell`; `.plugin(tauri_plugin_shell::init())`; Rust `app.shell().command("prog").args([..]).output().await` / `.spawn()` | Everything blocked by default; JS scope in capability: `{"identifier":"shell:allow-execute","allow":[{"name":"x","cmd":"sh","args":["-c",{"validator":"\\S+"}],"sidecar":false}]}`; also `shell:allow-spawn/kill/stdin-write`. `shell:default` only allows `open`. https://v2.tauri.app/plugin/shell/ |
| single-instance | `cargo add tauri-plugin-single-instance` (desktop target); `tauri_plugin_single_instance::init(|app, args, cwd| { focus main window })` | **Must be the first plugin registered.** No JS API, no capability needed. Linux snap/flatpak need DBus perms. https://v2.tauri.app/plugin/single-instance/ |

Launching a terminal: Rust-side `std::process::Command` needs no Tauri capability (capabilities only gate webview->command IPC). macOS: `open -a Terminal` / `osascript -e 'tell application "Terminal" to do script "ssh ..."'`; Windows: `wt.exe` or `cmd /c start`. UNVERIFIED in official docs (my own suggestion). If invoked from JS via shell plugin you need an explicit scope.

### 3.3 Capabilities (https://v2.tauri.app/security/capabilities/)
- Files in `src-tauri/capabilities/*.json|toml`, all enabled by default; if `app.security.capabilities` lists some in `tauri.conf.json`, only those are used.
- Fields: `identifier`, `description`, `windows: ["main"]`, `webviews`, `permissions: ["core:default", "autostart:default", {identifier, allow:[scope]}]`, `platforms: ["macOS","windows"]`, `remote`. `$schema` -> `../gen/schemas/desktop-schema.json`.
- Plugin perms are `plugin:allow-cmd` / `deny-cmd` / `default`; core uses `core:window:allow-set-title` etc. Multiple capabilities on one window merge. Registered app commands are callable from all windows unless restricted via `build.rs` `AppManifest::commands`.
- Capabilities guard the **webview IPC only**; Rust code can do anything.

---

## 4. Windows OpenSSH Server (https://learn.microsoft.com/en-us/windows-server/administration/openssh/openssh_install_firstuse)

Requires Windows 10 build 1809+/Server 2019+, PowerShell 5.1+, elevated admin.
```powershell
Get-WindowsCapability -Online | Where-Object Name -like 'OpenSSH*'
Add-WindowsCapability -Online -Name OpenSSH.Server~~~~0.0.1.0     # client: OpenSSH.Client~~~~0.0.1.0
Start-Service sshd
Set-Service -Name sshd -StartupType 'Automatic'
Get-NetFirewallRule -Name "OpenSSH-Server-In-TCP"    # created automatically on install, TCP 22
```
Config (https://learn.microsoft.com/en-us/windows-server/administration/openssh/openssh-server-configuration):
- `sshd_config`: `%programdata%\ssh\sshd_config` (= `C:\ProgramData\ssh\sshd_config`); generated on first service start if missing; **restart sshd after edit** (`Restart-Service sshd`). Host keys in `C:\ProgramData\ssh`.
- Admin users' keys: `%programdata%\ssh\administrators_authorized_keys` is used **instead of** `~\.ssh\authorized_keys` when user is in Administrators (current docs say this under `AuthorizedKeysFile`; older default config has a `Match Group administrators` block with `AuthorizedKeysFile __PROGRAMDATA__/ssh/administrators_authorized_keys`). Standard users: `C:\Users\<user>\.ssh\authorized_keys`.
- ACL (exact, from docs): `icacls.exe "C:\ProgramData\ssh\administrators_authorized_keys" /inheritance:r /grant "Administrators:F" /grant "SYSTEM:F"` (use SIDs for localized Windows: `"*S-1-5-32-544:F"`). Only SYSTEM and Administrators may have ACEs (https://learn.microsoft.com/en-us/windows-server/administration/openssh/openssh_keymanagement).
- Only auth methods available: `password`, `publickey`. Disable passwords: `PasswordAuthentication no` in sshd_config. `StrictModes` is not supported on Windows. Entra ID accounts: no key auth.
- Default shell: `New-ItemProperty -Path "HKLM:\SOFTWARE\OpenSSH" -Name DefaultShell -Value "C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe" -PropertyType String -Force` (initial default is cmd.exe).
- Windows 11 **Home**: MS page does not state edition restrictions; third-party guides say it installs on Home/Pro/Enterprise (https://geekrewind.com/how-to-install-openssh-server-on-windows-11/). **UNVERIFIED officially** - treat as available, test on Home.

## 5. macOS Remote Login

- GUI: System Settings > General > Sharing > Remote Login (Info button: "Allow full disk access for remote users", allowed-users list). https://support.apple.com/guide/mac-help/allow-a-remote-computer-to-access-your-mac-mchlp1066/mac
- CLI: `sudo systemsetup -setremotelogin on` / `-getremotelogin`. Since Catalina the **parent process (Terminal / the app launching it) needs Full Disk Access** else it fails with a "requires Full Disk Access privileges" error (https://support.apple.com/en-us/HT210595 ; https://discussions.apple.com/thread/251833298). A Tauri app calling this via sudo would need FDA itself: **not realistic -> guide the user to enable via System Settings** and verify with `nc -z localhost 22` / `sudo systemsetup -getremotelogin` (also needs FDA).
- launchctl alternative: `sudo launchctl load -w /System/Library/LaunchDaemons/ssh.plist` (file exists on this Mac, macOS 27.2.1); whether it avoids FDA is **UNVERIFIED** (only a non-authoritative blog claims it). On modern macOS prefer `launchctl enable/bootstrap system/com.openssh.sshd`-style - UNVERIFIED.
- `sshd_config`: `/etc/ssh/sshd_config` (verified locally) contains `Include /etc/ssh/sshd_config.d/*` at the top (line 19) and `/etc/ssh/sshd_config.d/100-macos.conf` (`UsePAM yes`, `AcceptEnv LANG LC_*`, `Include /etc/ssh/crypto.conf`). Drop-in for key-only: `/etc/ssh/sshd_config.d/050-twinfolders.conf` with `PasswordAuthentication no` + `KbdInteractiveAuthentication no` (+ `PubkeyAuthentication yes`). sshd uses first obtained value and Include comes before the main file's directives, so drop-in wins; name it to sort before `100-macos.conf`. Writing to /etc needs root (sudo prompt). Note `UsePAM yes` + KbdInteractive can still allow passwords - hence set both to `no`. Per-user keys: `~/.ssh/authorized_keys` (mode 600, dir 700). Verify with `sudo sshd -T | grep -i passwordauth`.
- Note: macOS OS updates may reset /etc/ssh/sshd_config but `sshd_config.d` drop-ins persist (UNVERIFIED).

---

## 6. Rust crates

**keyring 4.2.0** (https://docs.rs/keyring/latest/keyring/ , https://github.com/open-source-cooperative/keyring-rs): default feature `v1` keeps the old API and bundles macOS Keychain (`apple-native-keyring-store`), Windows Credential Manager (`windows-native-keyring-store`), Linux secret-service. API (examples/v1/main.rs):
```rust
let e = keyring::Entry::new("twinfolders", "syncthing-api-key")?;
e.set_password("...")?; let p = e.get_password()?; e.delete_credential()?;   // also set_secret/get_secret for bytes
```
Error type `keyring::Error::NoEntry` when missing. Maintainers recommend `keyring-core` + individual store crates for full control (v4 split) - the `v1` default is fine for us. Windows limit: ~2.5 KB secret.

**reqwest 0.13.5** (changelog https://github.com/seanmonstar/reqwest/blob/master/CHANGELOG.md): 0.13 breaking: **rustls is default TLS** (aws-lc provider; feature renamed `rustls`), `query`/`form` features now **off by default**; `json` and `blocking` features are opt-in. Cargo: `reqwest = { version = "0.13", default-features = false, features = ["json","blocking"] }` (we only talk to `http://127.0.0.1`, so TLS not required; keep default `rustls` if HTTPS to Syncthing GUI is ever used - self-signed needs `.danger_accept_invalid_certs(true)`). Basic: `reqwest::Client::new().get(url).header("X-API-Key", key).timeout(..).send().await?.error_for_status()?.json::<T>().await?`; blocking: `reqwest::blocking::Client`. For long-poll events set per-request timeout > `timeout` query (e.g. 70 s).

**serde 1.0.229 / serde_json 1.0.154**: `#[derive(Serialize, Deserialize)] #[serde(rename_all = "camelCase")]` fits Syncthing JSON; use `#[serde(default)]` and avoid `deny_unknown_fields`; use `serde_json::Value` for `data` of events and `Option<>` for nullable. When PUT/POSTing a folder/device preserve unknown fields (round-trip via `Value` or `#[serde(flatten)] extra: Map`).

**Mock HTTP** (all fine; pick one):
| crate | API | Notes |
|---|---|---|
| mockito 1.7.2 | `let mut s = mockito::Server::new_async().await; let m = s.mock("GET","/rest/system/ping").match_header("x-api-key","k").with_status(200).with_body(r#"{"ping":"pong"}"#).create_async().await; s.url(); m.assert();` | Simple, sync + async. https://github.com/lipanski/mockito |
| httpmock 0.9.0 | `let s = MockServer::start(); let m = s.mock(\|when, then\| { when.method(GET).path("/p").header("X-API-Key","k"); then.status(200).json_body(json!({..})); }); m.assert();` | Nice DSL, newest. https://github.com/httpmock/httpmock |
| wiremock 0.6.5 | `let s = MockServer::start().await; Mock::given(method("GET")).and(path("/p")).respond_with(ResponseTemplate::new(200).set_body_json(..)).mount(&s).await;` | async-only, tokio; unmatched = 404; last release 2025-08. https://github.com/LukeMathWalker/wiremock-rs |
Recommendation: **mockito** or **httpmock** for reqwest unit tests; real isolated Syncthing daemons (section 1.7) for integration.

---

## Gotchas for implementation

1. Header `X-API-Key` is correct (also Bearer). REST is on `127.0.0.1:8384` by default but with `--no-port-probing` omitted Syncthing may pick another port on first config; always pass `--gui-address` and `--no-port-probing`, and read apikey from `config.xml` or force `--gui-apikey`.
2. v2 CLI flags are double-dash only; no default folder is created; handle `Include` of old single-dash flags errors.
3. Config JSON naming differs from XML: `deviceID`/`addresses` (REST) vs `id`/`address` (XML). `PUT` on collections takes an array, `POST` a single object, `PATCH` replaces child objects wholesale (don't PATCH `devices` of a folder with a partial list).
4. `GET /rest/events` long-polls (default 60 s). Persist the last `id`; handle gaps (missed events) by re-reading `db/completion`/`db/status`; first call `?since=0&limit=1`.
5. `db/status` is expensive - prefer FolderSummary/FolderCompletion events; `FolderCompletion` fires per remote device; `completion` can be 100 while `remoteState` is `paused`/`notSharing`/`unknown`.
6. PendingFoldersChanged docs example is malformed JSON; tolerate string booleans. Auto-accept folders only if wanted; path of accepted folders comes from `defaults.folder.path`.
7. Conflict short ID is 7 chars (base32 A-Z2-7), time is local to the resolver; conflict copies sync to the other side; `maxConflicts` is per folder.
8. `(?d)` is the key for `.DS_Store`/`Thumbs.db`/`desktop.ini` ignores so directory deletes don't get stuck; macOS/Windows are case-insensitive anyway. Consider `ignorePerms: true` for Win<->Mac pair.
9. Windows: use Task Scheduler "at log on" with `--no-console --no-browser`; stopping the task only stops the monitor - shut down via `POST /rest/system/shutdown`. Windows Firewall prompt on first run.
10. Tailscale SSH is not available on Windows (client only) nor on the macOS App Store/Standalone apps: use real OpenSSH over the tailnet. WoL is not natively supported (issue #306 open); needs an on-LAN relay. `tailscale status --json` format is unstable: parse defensively (peers keyed by public key, `DNSName` has trailing dot, `LastSeen` is zero-time when online).
11. Windows admin accounts use `C:\ProgramData\ssh\administrators_authorized_keys` with exact ACL (SYSTEM + Administrators only) - wrong ACL silently fails key auth. Restart sshd after editing `sshd_config`. Windows sshd lacks `StrictModes`.
12. macOS `systemsetup -setremotelogin` needs Full Disk Access for the calling app: do not automate; deep-link the user to System Settings > Sharing. Key-only auth via `/etc/ssh/sshd_config.d/050-*.conf` (needs sudo).
13. Tauri: register single-instance first; notifications on Windows only work for installed (MSIX/NSIS) builds; tray events not on Linux; capabilities only gate JS IPC, Rust-side `Command` is unrestricted; plugin MSRV Rust 1.90.
14. reqwest 0.13: `json`/`blocking`/`query` features are opt-in; default TLS is rustls (aws-lc needs cmake/NASM on some Windows setups - UNVERIFIED; use `default-features=false` since only localhost HTTP is needed).
15. keyring 4.x default `v1` feature = old API; keep entries under service `twinfolders`; Linux not a target.
