//! What the status panel shows: one snapshot of devices and folders built
//! from several Syncthing REST calls, with the health rules kept here (pure,
//! tested) so the UI only renders.
//!
//! Health per folder (SPEC R3/R4):
//! - `Paused`, `Error`, `Scanning` come straight from Syncthing;
//! - `Syncing` when this side still needs data, or a connected peer does;
//! - `WaitingForPeer` when this side is complete but a sharing peer is
//!   offline ("aguardando o outro computador");
//! - `UpToDate` otherwise.

use std::collections::HashMap;
use std::time::Instant;

use serde::Serialize;
use serde_json::Value;

use crate::client::{
    Completion, Connection, FolderStatus, PendingDevice, PendingFolder, Result, SyncthingClient,
};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FolderHealth {
    UpToDate,
    /// `percent` of the slowest side (0-100).
    Syncing {
        percent: f64,
    },
    WaitingForPeer,
    Scanning,
    Paused,
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerProgress {
    pub device_id: String,
    pub name: String,
    pub connected: bool,
    pub completion: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderView {
    pub id: String,
    pub label: String,
    pub path: String,
    pub health: FolderHealth,
    pub need_bytes: u64,
    pub global_bytes: u64,
    pub peers: Vec<PeerProgress>,
    /// RFC3339 time of the last file synced in this folder, if any.
    pub last_sync: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceView {
    pub id: String,
    pub name: String,
    pub connected: bool,
    pub paused: bool,
    pub address: String,
    pub last_seen: Option<String>,
    /// Bytes per second since the previous snapshot.
    pub in_rate: f64,
    pub out_rate: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub my_id: String,
    pub devices: Vec<DeviceView>,
    pub folders: Vec<FolderView>,
    /// Devices asking to pair (need explicit approval).
    pub pending_devices: Vec<PendingDevice>,
    /// Folders offered by a paired device (need explicit acceptance).
    pub pending_folders: Vec<PendingFolder>,
}

/// Byte counters from the previous snapshot, for transfer rates.
#[derive(Debug, Clone)]
pub struct RateSample {
    at: Instant,
    totals: HashMap<String, (u64, u64)>,
}

/// Rule for one folder; see module docs.
pub fn folder_health(status: &FolderStatus, paused: bool, peers: &[PeerProgress]) -> FolderHealth {
    if paused {
        return FolderHealth::Paused;
    }
    if status.state == "error" || status.errors > 0 || status.pull_errors > 0 {
        let message = status
            .error
            .clone()
            .filter(|e| !e.is_empty())
            .unwrap_or_else(|| {
                format!(
                    "{} arquivo(s) com erro",
                    status.errors.max(status.pull_errors)
                )
            });
        return FolderHealth::Error { message };
    }
    if status.state.starts_with("scan") {
        return FolderHealth::Scanning;
    }
    let local_pct = if status.global_bytes == 0 {
        100.0
    } else {
        (status.in_sync_bytes as f64 / status.global_bytes as f64 * 100.0).min(100.0)
    };
    let local_busy = status.need_bytes > 0 || status.state.starts_with("sync");
    let connected_min = peers
        .iter()
        .filter(|p| p.connected)
        .map(|p| p.completion)
        .fold(100.0_f64, f64::min);
    if local_busy || connected_min < 100.0 {
        return FolderHealth::Syncing {
            percent: round1(local_pct.min(connected_min)),
        };
    }
    if peers.iter().any(|p| !p.connected) {
        return FolderHealth::WaitingForPeer;
    }
    FolderHealth::UpToDate
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

/// Bytes/s between two counter readings; 0 on first sample, counter reset
/// (daemon restart) or zero elapsed time.
pub fn rate(prev: Option<(u64, u64)>, now: (u64, u64), elapsed_s: f64) -> (f64, f64) {
    match prev {
        Some((pi, po)) if elapsed_s > 0.0 && now.0 >= pi && now.1 >= po => (
            (now.0 - pi) as f64 / elapsed_s,
            (now.1 - po) as f64 / elapsed_s,
        ),
        _ => (0.0, 0.0),
    }
}

/// Seconds since the Unix epoch for an RFC3339 timestamp
/// (`YYYY-MM-DDTHH:MM:SS[.frac](Z|±HH:MM)`), as Syncthing emits them.
fn to_unix(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() < 20
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
    {
        return None;
    }
    let num = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (y, mo, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (h, mi, se) = (num(11..13)?, num(14..16)?, num(17..19)?);
    let mut i = 19;
    if b.get(i) == Some(&b'.') {
        i += 1;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
    }
    let offset = match b.get(i)? {
        b'Z' => 0,
        sign @ (b'+' | b'-') => {
            let o = num(i + 1..i + 3)? * 3600 + num(i + 4..i + 6)? * 60;
            if *sign == b'+' {
                o
            } else {
                -o
            }
        }
        _ => return None,
    };
    // Days from civil (Howard Hinnant's algorithm).
    let (y2, m2) = if mo <= 2 {
        (y - 1, mo + 9)
    } else {
        (y, mo - 3)
    };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let doy = (153 * m2 + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + h * 3600 + mi * 60 + se - offset)
}

/// Syncthing uses 1970-01-01 / 0001-01-01 for "never": anything before
/// 2000 counts as missing.
fn valid_time(s: &str) -> Option<&str> {
    to_unix(s).filter(|t| *t >= 946_684_800).map(|_| s)
}

/// The more recent of two timestamps.
fn newest(a: Option<&str>, b: Option<&str>) -> Option<String> {
    match (a.and_then(valid_time), b.and_then(valid_time)) {
        (Some(x), Some(y)) => Some(if to_unix(y) > to_unix(x) { y } else { x }.to_string()),
        (x, y) => x.or(y).map(String::from),
    }
}

fn str_at<'a>(v: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut cur = v;
    for k in path {
        cur = cur.get(*k)?;
    }
    cur.as_str().and_then(valid_time)
}

/// Fetches everything and builds an [`Overview`]. Pass the previous
/// [`RateSample`] to get transfer rates.
pub fn collect(c: &SyncthingClient, prev: Option<&RateSample>) -> Result<(Overview, RateSample)> {
    let my_id = c.my_id()?;
    let devices_cfg = c.devices()?;
    let folders_cfg = c.folders()?;
    let conns: HashMap<String, Connection> = c.connections()?;
    let folder_stats = c.get_value("/rest/stats/folder").unwrap_or(Value::Null);
    let device_stats = c.get_value("/rest/stats/device").unwrap_or(Value::Null);

    let now = Instant::now();
    let elapsed = prev
        .map(|p| now.duration_since(p.at).as_secs_f64())
        .unwrap_or(0.0);
    let mut totals = HashMap::new();
    let names: HashMap<&str, &str> = devices_cfg
        .iter()
        .map(|d| (d.device_id.as_str(), d.name.as_str()))
        .collect();

    let devices: Vec<DeviceView> = devices_cfg
        .iter()
        .filter(|d| d.device_id != my_id)
        .map(|d| {
            let conn = conns.get(&d.device_id);
            let counters = conn
                .map(|c| (c.in_bytes_total, c.out_bytes_total))
                .unwrap_or((0, 0));
            totals.insert(d.device_id.clone(), counters);
            let (in_rate, out_rate) = rate(
                prev.and_then(|p| p.totals.get(&d.device_id).copied()),
                counters,
                elapsed,
            );
            DeviceView {
                id: d.device_id.clone(),
                name: d.name.clone(),
                connected: conn.map(|c| c.connected).unwrap_or(false),
                paused: d.paused,
                address: conn.map(|c| c.address.clone()).unwrap_or_default(),
                last_seen: str_at(&device_stats, &[&d.device_id, "lastSeen"]).map(String::from),
                in_rate,
                out_rate,
            }
        })
        .collect();

    let mut folders = Vec::new();
    for f in &folders_cfg {
        let status = c.folder_status(&f.id)?;
        let mut peers = Vec::new();
        for fd in f.devices.iter().filter(|fd| fd.device_id != my_id) {
            let connected = conns
                .get(&fd.device_id)
                .map(|c| c.connected)
                .unwrap_or(false);
            let completion = c
                .completion(&f.id, Some(&fd.device_id))
                .map(|x: Completion| x.completion)
                .unwrap_or(0.0);
            peers.push(PeerProgress {
                device_id: fd.device_id.clone(),
                name: names.get(fd.device_id.as_str()).unwrap_or(&"").to_string(),
                connected,
                completion: round1(completion),
            });
        }
        let health = folder_health(&status, f.paused, &peers);
        folders.push(FolderView {
            id: f.id.clone(),
            label: if f.label.is_empty() {
                f.id.clone()
            } else {
                f.label.clone()
            },
            path: f.path.clone(),
            health,
            need_bytes: status.need_bytes,
            global_bytes: status.global_bytes,
            peers,
            // Newest of "last file received" and "reached current state": on
            // the side that created the files nothing is ever received.
            last_sync: newest(
                str_at(&folder_stats, &[&f.id, "lastFile", "at"]),
                status.state_changed.as_deref(),
            ),
        });
    }
    folders.sort_by_key(|f| f.label.to_lowercase());

    let pending_devices = c.pending_devices().unwrap_or_default();
    let pending_folders = c.pending_folders().unwrap_or_default();
    Ok((
        Overview {
            my_id,
            devices,
            folders,
            pending_devices,
            pending_folders,
        },
        RateSample { at: now, totals },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(state: &str, need: u64, global: u64, in_sync: u64) -> FolderStatus {
        FolderStatus {
            state: state.into(),
            need_bytes: need,
            global_bytes: global,
            in_sync_bytes: in_sync,
            errors: 0,
            pull_errors: 0,
            state_changed: None,
            error: None,
        }
    }

    fn peer(connected: bool, completion: f64) -> PeerProgress {
        PeerProgress {
            device_id: "P".into(),
            name: "win".into(),
            connected,
            completion,
        }
    }

    #[test]
    fn up_to_date_when_both_sides_complete() {
        assert_eq!(
            folder_health(&st("idle", 0, 100, 100), false, &[peer(true, 100.0)]),
            FolderHealth::UpToDate
        );
    }

    #[test]
    fn waiting_for_peer_when_peer_offline() {
        assert_eq!(
            folder_health(&st("idle", 0, 100, 100), false, &[peer(false, 40.0)]),
            FolderHealth::WaitingForPeer
        );
    }

    #[test]
    fn syncing_reports_slowest_side() {
        assert_eq!(
            folder_health(&st("syncing", 50, 200, 150), false, &[peer(true, 100.0)]),
            FolderHealth::Syncing { percent: 75.0 }
        );
        assert_eq!(
            folder_health(&st("idle", 0, 100, 100), false, &[peer(true, 33.33)]),
            FolderHealth::Syncing { percent: 33.3 }
        );
    }

    #[test]
    fn paused_error_and_scanning_take_priority() {
        assert_eq!(
            folder_health(&st("syncing", 9, 9, 0), true, &[]),
            FolderHealth::Paused
        );
        let mut e = st("error", 0, 0, 0);
        e.error = Some("folder path missing".into());
        assert_eq!(
            folder_health(&e, false, &[]),
            FolderHealth::Error {
                message: "folder path missing".into()
            }
        );
        let mut pe = st("idle", 0, 1, 1);
        pe.pull_errors = 2;
        assert_eq!(
            folder_health(&pe, false, &[]),
            FolderHealth::Error {
                message: "2 arquivo(s) com erro".into()
            }
        );
        assert_eq!(
            folder_health(&st("scanning", 0, 1, 1), false, &[]),
            FolderHealth::Scanning
        );
    }

    #[test]
    fn empty_folder_is_up_to_date() {
        assert_eq!(
            folder_health(&st("idle", 0, 0, 0), false, &[peer(true, 100.0)]),
            FolderHealth::UpToDate
        );
    }

    #[test]
    fn rates() {
        assert_eq!(rate(None, (100, 100), 1.0), (0.0, 0.0));
        assert_eq!(rate(Some((100, 50)), (300, 150), 2.0), (100.0, 50.0));
        assert_eq!(
            rate(Some((500, 500)), (10, 10), 1.0),
            (0.0, 0.0),
            "counter reset"
        );
        assert_eq!(rate(Some((1, 1)), (2, 2), 0.0), (0.0, 0.0));
    }

    #[test]
    fn health_serializes_for_the_ui() {
        let j = serde_json::to_value(FolderHealth::Syncing { percent: 50.0 }).unwrap();
        assert_eq!(j, serde_json::json!({"kind": "syncing", "percent": 50.0}));
        let j = serde_json::to_value(FolderHealth::WaitingForPeer).unwrap();
        assert_eq!(j, serde_json::json!({"kind": "waitingForPeer"}));
    }

    #[test]
    fn placeholder_timestamps_mean_never() {
        assert_eq!(
            valid_time("1970-01-01T00:00:00Z"),
            None,
            "Syncthing's 'never'"
        );
        assert_eq!(valid_time("0001-01-01T00:00:00Z"), None);
        assert_eq!(valid_time(""), None);
        assert_eq!(
            valid_time("2026-11-16T10:00:00Z"),
            Some("2026-11-16T10:00:00Z")
        );
    }

    #[test]
    fn last_sync_is_newest_of_received_file_and_state_change() {
        assert_eq!(
            newest(
                Some("2026-11-16T10:00:00Z"),
                Some("2026-11-16T11:30:00.123456-03:00")
            ),
            Some("2026-11-16T11:30:00.123456-03:00".to_string())
        );
        assert_eq!(
            newest(Some("2026-11-16T10:00:00Z"), None),
            Some("2026-11-16T10:00:00Z".to_string())
        );
        assert_eq!(newest(None, None), None);
    }
}
