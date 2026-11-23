//! Minimal, typed client for the Syncthing v2 REST API.
//!
//! Endpoint shapes were checked against a live Syncthing v2.1.7 daemon and
//! <https://docs.syncthing.net/dev/rest.html>. Only the fields TwinFolders
//! needs are typed; config objects keep every other field in `extra` so a
//! read-modify-write never drops settings we don't know about.
//!
//! The API key is sent as the `X-API-Key` header and is never printed by
//! `Debug`.

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Errors returned by [`SyncthingClient`].
#[derive(Debug)]
pub enum Error {
    /// 401/403: missing or wrong API key.
    Unauthorized,
    /// Any other non-2xx status.
    Http { status: u16, body: String },
    /// Connection refused, timeout, DNS, etc. (daemon not running).
    Transport(String),
    /// Response body was not the JSON we expected.
    Decode(String),
    /// Base URL is not a loopback address (the API key must never leave
    /// this machine).
    NotLoopback(String),
    /// Folder ID unsafe to place in a URL (`[A-Za-z0-9._-]{1,64}`, not `.`/`..`).
    InvalidFolderId(String),
    /// TLS setup problem (bad pinned certificate, non-https URL).
    Tls(String),
    /// Request timed out. For writes the outcome is unknown: Syncthing may
    /// or may not have applied it, so re-read before retrying.
    Timeout,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Unauthorized => write!(f, "Syncthing rejected the API key"),
            Error::Http { status, body } => write!(f, "Syncthing HTTP {status}: {body}"),
            Error::Transport(e) => write!(f, "cannot reach Syncthing: {e}"),
            Error::Decode(e) => write!(f, "unexpected Syncthing response: {e}"),
            Error::NotLoopback(u) => write!(f, "refusing non-loopback Syncthing address: {u}"),
            Error::InvalidFolderId(id) => write!(f, "invalid folder ID: {id:?}"),
            Error::Tls(e) => write!(f, "TLS: {e}"),
            Error::Timeout => write!(f, "Syncthing did not answer in time"),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

// ---------- config types ----------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    #[serde(rename = "deviceID")]
    pub device_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub addresses: Vec<String>,
    #[serde(default)]
    pub paused: bool,
    #[serde(default)]
    pub auto_accept_folders: bool,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FolderDevice {
    #[serde(rename = "deviceID")]
    pub device_id: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl FolderDevice {
    pub fn new(device_id: &str) -> Self {
        Self {
            device_id: device_id.to_string(),
            extra: Map::new(),
        }
    }
}

/// Folder versioning ("lixeira"). `kind` is one of `trashcan`, `simple`,
/// `staggered`, `external`, or empty for none. Param values are strings in
/// Syncthing's config.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Versioning {
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub params: BTreeMap<String, String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Versioning {
    /// Staggered versioning keeping versions for `days` days.
    pub fn staggered_days(days: u32) -> Self {
        let mut params = BTreeMap::new();
        params.insert("maxAge".into(), (u64::from(days) * 86_400).to_string());
        Self {
            kind: "staggered".into(),
            params,
            extra: Map::new(),
        }
    }

    /// Trash can versioning, deleting old versions after `days` days (0 = never).
    pub fn trashcan_days(days: u32) -> Self {
        let mut params = BTreeMap::new();
        params.insert("cleanoutDays".into(), days.to_string());
        Self {
            kind: "trashcan".into(),
            params,
            extra: Map::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    pub id: String,
    #[serde(default)]
    pub label: String,
    pub path: String,
    #[serde(rename = "type", default = "default_folder_type")]
    pub kind: String,
    #[serde(default)]
    pub devices: Vec<FolderDevice>,
    #[serde(default)]
    pub paused: bool,
    #[serde(default)]
    pub versioning: Versioning,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

fn default_folder_type() -> String {
    "sendreceive".into()
}

impl Folder {
    /// A send-receive folder shared with `device_ids` (include the local ID).
    pub fn new(id: &str, label: &str, path: &str, device_ids: &[&str]) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            path: path.into(),
            kind: default_folder_type(),
            devices: device_ids.iter().map(|d| FolderDevice::new(d)).collect(),
            paused: false,
            versioning: Versioning::default(),
            extra: Map::new(),
        }
    }

    pub fn with_versioning(mut self, v: Versioning) -> Self {
        self.versioning = v;
        self
    }
}

// ---------- status types ----------

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingDevice {
    pub device_id: String,
    pub name: String,
    pub address: String,
    pub time: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingFolder {
    pub folder_id: String,
    pub offered_by: String,
    pub label: String,
    pub time: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Completion {
    pub completion: f64,
    #[serde(default)]
    pub global_bytes: u64,
    #[serde(default)]
    pub need_bytes: u64,
    #[serde(default)]
    pub need_items: u64,
    #[serde(default)]
    pub need_deletes: u64,
    #[serde(default)]
    pub remote_state: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderStatus {
    /// idle, scanning, scan-waiting, syncing, sync-waiting, sync-preparing, cleaning, error, ...
    pub state: String,
    #[serde(default)]
    pub need_bytes: u64,
    #[serde(default)]
    pub global_bytes: u64,
    #[serde(default)]
    pub in_sync_bytes: u64,
    #[serde(default)]
    pub errors: u64,
    #[serde(default)]
    pub pull_errors: u64,
    #[serde(default)]
    pub state_changed: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Event {
    pub id: u64,
    #[serde(rename = "type")]
    pub kind: String,
    pub time: String,
    #[serde(default)]
    pub data: Value,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    #[serde(default)]
    pub connected: bool,
    #[serde(default)]
    pub paused: bool,
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub in_bytes_total: u64,
    #[serde(default)]
    pub out_bytes_total: u64,
}

// ---------- client ----------

/// Blocking client. Cheap to clone; safe to use from a worker thread.
#[derive(Clone)]
pub struct SyncthingClient {
    base: String,
    api_key: String,
    agent: ureq::Agent,
}

impl fmt::Debug for SyncthingClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SyncthingClient")
            .field("base", &self.base)
            .field("api_key", &"<redacted>")
            .finish()
    }
}

/// Extra time allowed on top of a long-poll timeout before giving up.
const LONG_POLL_SLACK: Duration = Duration::from_secs(10);
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

type Response = ureq::http::Response<ureq::Body>;

/// Distinguishes timeouts (outcome unknown for writes) from other
/// transport failures (daemon not reachable).
fn transport(e: ureq::Error) -> Error {
    match e {
        ureq::Error::Timeout(_) => Error::Timeout,
        other => Error::Transport(other.to_string()),
    }
}

/// `.stignore` content as Syncthing sees it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Ignores {
    #[serde(rename = "ignore", default)]
    pub lines: Vec<String>,
    /// Syncthing's parse error for the patterns, if any.
    #[serde(default)]
    pub error: Option<String>,
}

/// Max bytes of an error body kept for messages/logs.
const MAX_ERROR_BODY: usize = 512;

/// True only for `http(s)://{127.0.0.1|localhost|[::1]}[:port][/...]`.
fn is_loopback_url(url: &str) -> bool {
    let rest = match url.split_once("://") {
        Some(("http" | "https", rest)) => rest,
        _ => return false,
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.contains('@') {
        return false;
    }
    let host = if let Some(v6) = authority.strip_prefix('[') {
        match v6.split_once(']') {
            Some((h, tail)) if tail.is_empty() || tail.starts_with(':') => return h == "::1",
            _ => return false,
        }
    } else {
        authority.split(':').next().unwrap_or("")
    };
    matches!(host, "127.0.0.1" | "localhost")
}

/// Folder IDs are interpolated into URL paths, so only a safe charset is
/// allowed. Syncthing's own generated IDs (`abcde-12345`) satisfy this.
pub fn validate_folder_id(id: &str) -> Result<()> {
    let ok = (1..=64).contains(&id.len())
        && id != "."
        && id != ".."
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
    if ok {
        Ok(())
    } else {
        Err(Error::InvalidFolderId(id.to_string()))
    }
}

/// Device IDs also go into URL paths: letters, digits and dashes only.
fn validate_device_id(id: &str) -> Result<()> {
    if (1..=72).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
        Ok(())
    } else {
        Err(Error::InvalidFolderId(id.to_string()))
    }
}

fn sanitize_body(body: &str) -> String {
    let clean: String = body.chars().filter(|c| !c.is_control()).collect();
    if clean.len() <= MAX_ERROR_BODY {
        return clean;
    }
    let mut cut = MAX_ERROR_BODY;
    while !clean.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}…", &clean[..cut])
}

impl SyncthingClient {
    /// `base_url` like `http://127.0.0.1:8384`. Only loopback addresses are
    /// accepted, and redirects are never followed: ureq keeps custom headers
    /// such as `X-API-Key` across redirects, which would leak the key.
    pub fn new(base_url: &str, api_key: &str) -> Result<Self> {
        Self::build(base_url, api_key, None)
    }

    /// HTTPS client that trusts **only** the given CA (PEM), so the API key
    /// is sent only to a server holding a certificate signed by it (see
    /// [`crate::guitls`]). Requires an `https://` loopback URL.
    pub fn with_pinned_ca(base_url: &str, api_key: &str, ca_pem: &[u8]) -> Result<Self> {
        if !base_url.starts_with("https://") {
            return Err(Error::Tls("pinned client requires an https:// URL".into()));
        }
        let ca = ureq::tls::Certificate::from_pem(ca_pem).map_err(|e| Error::Tls(e.to_string()))?;
        Self::build(base_url, api_key, Some(ca))
    }

    fn build(
        base_url: &str,
        api_key: &str,
        pinned: Option<ureq::tls::Certificate<'static>>,
    ) -> Result<Self> {
        if !is_loopback_url(base_url) {
            return Err(Error::NotLoopback(base_url.to_string()));
        }
        let mut builder = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .timeout_connect(Some(Duration::from_secs(3)));
        if let Some(ca) = pinned {
            let tls = ureq::tls::TlsConfig::builder()
                .root_certs(ureq::tls::RootCerts::Specific(std::sync::Arc::new(vec![
                    ca,
                ])))
                .build();
            builder = builder.tls_config(tls);
        }
        Ok(Self {
            base: base_url.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
            agent: ureq::Agent::new_with_config(builder.build()),
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    fn check(mut resp: Response) -> Result<Response> {
        let status = resp.status().as_u16();
        match status {
            200..=299 => Ok(resp),
            401 | 403 => Err(Error::Unauthorized),
            _ => {
                let body = resp.body_mut().read_to_string().unwrap_or_default();
                Err(Error::Http {
                    status,
                    body: sanitize_body(&body),
                })
            }
        }
    }

    fn get_json<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, &str)],
        timeout: Duration,
    ) -> Result<T> {
        let mut req = self
            .agent
            .get(self.url(path))
            .header("X-API-Key", &self.api_key)
            .config()
            .timeout_global(Some(timeout))
            .build();
        for (k, v) in query {
            req = req.query(*k, *v);
        }
        let resp = req.call().map_err(transport)?;
        let mut resp = Self::check(resp)?;
        resp.body_mut()
            .read_json::<T>()
            .map_err(|e| Error::Decode(e.to_string()))
    }

    fn send_json(&self, method: &str, path: &str, body: &impl Serialize) -> Result<()> {
        let url = self.url(path);
        let req = match method {
            "POST" => self.agent.post(url),
            "PUT" => self.agent.put(url),
            "PATCH" => self.agent.patch(url),
            _ => unreachable!("unsupported method {method}"),
        };
        let resp = req
            .header("X-API-Key", &self.api_key)
            .config()
            .timeout_global(Some(DEFAULT_TIMEOUT))
            .build()
            .send_json(body)
            .map_err(transport)?;
        Self::check(resp).map(|_| ())
    }

    fn post_empty(&self, path: &str, query: &[(&str, &str)]) -> Result<()> {
        let mut req = self
            .agent
            .post(self.url(path))
            .header("X-API-Key", &self.api_key)
            .config()
            .timeout_global(Some(DEFAULT_TIMEOUT))
            .build();
        for (k, v) in query {
            req = req.query(*k, *v);
        }
        let resp = req.send_empty().map_err(transport)?;
        Self::check(resp).map(|_| ())
    }

    // ----- system -----

    pub fn ping(&self) -> Result<()> {
        self.get_json::<Value>("/rest/system/ping", &[], DEFAULT_TIMEOUT)
            .map(|_| ())
    }

    /// This device's ID.
    pub fn my_id(&self) -> Result<String> {
        #[derive(Deserialize)]
        struct S {
            #[serde(rename = "myID")]
            my_id: String,
        }
        Ok(self
            .get_json::<S>("/rest/system/status", &[], DEFAULT_TIMEOUT)?
            .my_id)
    }

    pub fn connections(&self) -> Result<HashMap<String, Connection>> {
        #[derive(Deserialize)]
        struct C {
            connections: HashMap<String, Connection>,
        }
        Ok(self
            .get_json::<C>("/rest/system/connections", &[], DEFAULT_TIMEOUT)?
            .connections)
    }

    /// GET any `/rest/...` endpoint as raw JSON (for fields not typed here).
    pub fn get_value(&self, path: &str) -> Result<Value> {
        if !path.starts_with("/rest/") {
            return Err(Error::Http {
                status: 0,
                body: format!("refusing non-REST path {path}"),
            });
        }
        self.get_json(path, &[], DEFAULT_TIMEOUT)
    }

    /// Removes a folder from Syncthing's config. Files on disk are NOT
    /// deleted (Syncthing never deletes them on folder removal).
    pub fn remove_folder(&self, id: &str) -> Result<()> {
        validate_folder_id(id)?;
        let resp = self
            .agent
            .delete(self.url(&format!("/rest/config/folders/{id}")))
            .header("X-API-Key", &self.api_key)
            .config()
            .timeout_global(Some(DEFAULT_TIMEOUT))
            .build()
            .call()
            .map_err(transport)?;
        Self::check(resp).map(|_| ())
    }

    /// Current `.stignore` lines of a folder (via Syncthing, which also
    /// reports pattern errors).
    pub fn ignores(&self, folder: &str) -> Result<Ignores> {
        validate_folder_id(folder)?;
        self.get_json("/rest/db/ignores", &[("folder", folder)], DEFAULT_TIMEOUT)
    }

    /// Replaces a folder's `.stignore`; Syncthing writes the file and
    /// applies it immediately. Check `error` in the result.
    pub fn set_ignores(&self, folder: &str, lines: &[&str]) -> Result<Ignores> {
        validate_folder_id(folder)?;
        let resp = self
            .agent
            .post(self.url("/rest/db/ignores"))
            .header("X-API-Key", &self.api_key)
            .query("folder", folder)
            .config()
            .timeout_global(Some(DEFAULT_TIMEOUT))
            .build()
            .send_json(serde_json::json!({ "ignore": lines }))
            .map_err(transport)?;
        let mut resp = Self::check(resp)?;
        resp.body_mut()
            .read_json::<Ignores>()
            .map_err(|e| Error::Decode(e.to_string()))
    }

    /// Asks Syncthing to exit cleanly (exit code 0).
    pub fn shutdown(&self) -> Result<()> {
        self.post_empty("/rest/system/shutdown", &[])
    }

    pub fn pause_device(&self, device_id: &str) -> Result<()> {
        self.post_empty("/rest/system/pause", &[("device", device_id)])
    }

    pub fn resume_device(&self, device_id: &str) -> Result<()> {
        self.post_empty("/rest/system/resume", &[("device", device_id)])
    }

    // ----- config -----

    pub fn devices(&self) -> Result<Vec<Device>> {
        self.get_json("/rest/config/devices", &[], DEFAULT_TIMEOUT)
    }

    /// Adds (or replaces) a device. Never enables auto-accept: folders from
    /// the peer must be approved explicitly.
    pub fn add_device(&self, device_id: &str, name: &str, addresses: &[&str]) -> Result<()> {
        let addrs: Vec<&str> = if addresses.is_empty() {
            vec!["dynamic"]
        } else {
            addresses.to_vec()
        };
        let body = serde_json::json!({
            "deviceID": device_id,
            "name": name,
            "addresses": addrs,
            "autoAcceptFolders": false,
        });
        self.send_json("POST", "/rest/config/devices", &body)
    }

    pub fn folders(&self) -> Result<Vec<Folder>> {
        self.get_json("/rest/config/folders", &[], DEFAULT_TIMEOUT)
    }

    /// Creates or replaces a folder by ID.
    pub fn put_folder(&self, folder: &Folder) -> Result<()> {
        validate_folder_id(&folder.id)?;
        self.send_json(
            "PUT",
            &format!("/rest/config/folders/{}", folder.id),
            folder,
        )
    }

    /// One folder's config.
    pub fn folder(&self, id: &str) -> Result<Folder> {
        validate_folder_id(id)?;
        self.get_json(&format!("/rest/config/folders/{id}"), &[], DEFAULT_TIMEOUT)
    }

    /// Replaces a folder's versioning object (PATCH replaces child objects
    /// whole, so pass a complete `Versioning`).
    pub fn set_folder_versioning(&self, id: &str, v: &Versioning) -> Result<()> {
        validate_folder_id(id)?;
        self.send_json(
            "PATCH",
            &format!("/rest/config/folders/{id}"),
            &serde_json::json!({ "versioning": v }),
        )
    }

    /// Replaces the list of devices a folder is shared with (include the
    /// local device). Other folder settings are untouched.
    pub fn set_folder_devices(&self, id: &str, devices: &[FolderDevice]) -> Result<()> {
        validate_folder_id(id)?;
        self.send_json(
            "PATCH",
            &format!("/rest/config/folders/{id}"),
            &serde_json::json!({ "devices": devices }),
        )
    }

    /// Renames a device in the config (also this computer's own entry,
    /// which is the name the other side sees).
    pub fn set_device_name(&self, device_id: &str, name: &str) -> Result<()> {
        validate_device_id(device_id)?;
        self.send_json(
            "PATCH",
            &format!("/rest/config/devices/{device_id}"),
            &serde_json::json!({ "name": name }),
        )
    }

    /// Removes a device from the config. Callers unshare its folders first
    /// (see `pairing::remove_device`).
    pub fn remove_device(&self, device_id: &str) -> Result<()> {
        validate_device_id(device_id)?;
        let resp = self
            .agent
            .delete(self.url(&format!("/rest/config/devices/{device_id}")))
            .header("X-API-Key", &self.api_key)
            .config()
            .timeout_global(Some(DEFAULT_TIMEOUT))
            .build()
            .call()
            .map_err(transport)?;
        Self::check(resp).map(|_| ())
    }

    /// Pauses or resumes one folder (other settings untouched).
    pub fn set_folder_paused(&self, id: &str, paused: bool) -> Result<()> {
        validate_folder_id(id)?;
        self.send_json(
            "PATCH",
            &format!("/rest/config/folders/{id}"),
            &serde_json::json!({ "paused": paused }),
        )
    }

    /// Archived versions Syncthing knows about: `{ "rel/path": [{versionTime,
    /// modTime, size}] }`.
    pub fn folder_versions(&self, id: &str) -> Result<Value> {
        validate_folder_id(id)?;
        self.get_json("/rest/folder/versions", &[("folder", id)], DEFAULT_TIMEOUT)
    }

    /// PATCH global options, e.g. `{"relaysEnabled": false}`.
    pub fn patch_options(&self, patch: &Value) -> Result<()> {
        self.send_json("PATCH", "/rest/config/options", patch)
    }

    // ----- cluster -----

    /// Devices that tried to connect but aren't configured yet.
    pub fn pending_devices(&self) -> Result<Vec<PendingDevice>> {
        #[derive(Deserialize)]
        struct P {
            #[serde(default)]
            name: String,
            #[serde(default)]
            address: String,
            #[serde(default)]
            time: String,
        }
        let map: BTreeMap<String, P> =
            self.get_json("/rest/cluster/pending/devices", &[], DEFAULT_TIMEOUT)?;
        Ok(map
            .into_iter()
            .map(|(device_id, p)| PendingDevice {
                device_id,
                name: p.name,
                address: p.address,
                time: p.time,
            })
            .collect())
    }

    /// Folders offered by connected peers that we haven't added yet.
    pub fn pending_folders(&self) -> Result<Vec<PendingFolder>> {
        #[derive(Deserialize)]
        struct Offer {
            #[serde(default)]
            label: String,
            #[serde(default)]
            time: String,
        }
        #[derive(Deserialize)]
        struct F {
            #[serde(rename = "offeredBy", default)]
            offered_by: BTreeMap<String, Offer>,
        }
        let map: BTreeMap<String, F> =
            self.get_json("/rest/cluster/pending/folders", &[], DEFAULT_TIMEOUT)?;
        let mut out = Vec::new();
        for (folder_id, f) in map {
            for (offered_by, o) in f.offered_by {
                out.push(PendingFolder {
                    folder_id: folder_id.clone(),
                    offered_by,
                    label: o.label,
                    time: o.time,
                });
            }
        }
        Ok(out)
    }

    // ----- db -----

    /// Sync completion of `folder` for a remote `device` (or the local
    /// device when `None`).
    pub fn completion(&self, folder: &str, device: Option<&str>) -> Result<Completion> {
        let mut q = vec![("folder", folder)];
        if let Some(d) = device {
            q.push(("device", d));
        }
        self.get_json("/rest/db/completion", &q, DEFAULT_TIMEOUT)
    }

    pub fn folder_status(&self, folder: &str) -> Result<FolderStatus> {
        self.get_json("/rest/db/status", &[("folder", folder)], DEFAULT_TIMEOUT)
    }

    /// Request an immediate rescan of a folder.
    pub fn scan(&self, folder: &str) -> Result<()> {
        self.post_empty("/rest/db/scan", &[("folder", folder)])
    }

    // ----- events -----

    /// Long-poll for events with id > `since`. Returns an empty list when
    /// `timeout_s` elapses with no events. `types` filters by event type.
    pub fn events(&self, since: u64, timeout_s: u64, types: &[&str]) -> Result<Vec<Event>> {
        let since_s = since.to_string();
        // 0 would mean "server default (60s)" and trip our client timeout.
        let timeout_s = timeout_s.max(1);
        let timeout_str = timeout_s.to_string();
        let joined = types.join(",");
        let mut q = vec![
            ("since", since_s.as_str()),
            ("timeout", timeout_str.as_str()),
        ];
        if !types.is_empty() {
            q.push(("events", joined.as_str()));
        }
        self.get_json(
            "/rest/events",
            &q,
            Duration::from_secs(timeout_s) + LONG_POLL_SLACK,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeouts_are_distinguished_from_unreachable() {
        assert!(matches!(
            transport(ureq::Error::Timeout(ureq::Timeout::Global)),
            Error::Timeout
        ));
        assert!(matches!(
            transport(ureq::Error::ConnectionFailed),
            Error::Transport(_)
        ));
    }
}
