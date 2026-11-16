//! Unit tests for the Syncthing REST client against a mock HTTP server.
//! Response bodies are trimmed copies of real Syncthing v2.1 responses.

use mockito::{Matcher, Server};
use twinfolders_core::client::{
    Error, Folder, FolderDevice, PendingFolder, SyncthingClient, Versioning,
};
use twinfolders_core::pairing;

const KEY: &str = "test-api-key";

fn client(server: &Server) -> SyncthingClient {
    SyncthingClient::new(&server.url(), KEY).unwrap()
}

#[test]
fn ping_sends_api_key_header() {
    let mut s = Server::new();
    let m = s
        .mock("GET", "/rest/system/ping")
        .match_header("X-API-Key", KEY)
        .with_body(r#"{"ping":"pong"}"#)
        .create();
    client(&s).ping().unwrap();
    m.assert();
}

#[test]
fn forbidden_maps_to_unauthorized() {
    let mut s = Server::new();
    s.mock("GET", "/rest/system/ping")
        .with_status(403)
        .with_body("CSRF Error")
        .create();
    assert!(matches!(client(&s).ping(), Err(Error::Unauthorized)));
}

#[test]
fn other_http_errors_keep_status_and_body() {
    let mut s = Server::new();
    s.mock("GET", "/rest/system/status")
        .with_status(500)
        .with_body("boom")
        .create();
    match client(&s).my_id() {
        Err(Error::Http { status, body }) => {
            assert_eq!(status, 500);
            assert_eq!(body, "boom");
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn transport_error_when_server_down() {
    let c = SyncthingClient::new("http://127.0.0.1:1", KEY).unwrap();
    assert!(matches!(c.ping(), Err(Error::Transport(_))));
}

#[test]
fn debug_output_never_contains_api_key() {
    let c = SyncthingClient::new("http://127.0.0.1:8384", "super-secret-key").unwrap();
    assert!(!format!("{c:?}").contains("super-secret-key"));
}

#[test]
fn my_id_reads_system_status() {
    let mut s = Server::new();
    s.mock("GET", "/rest/system/status")
        .with_body(r#"{"alloc":1,"myID":"AAAAAAA-BBBBBBB-CCCCCCC-DDDDDDD-EEEEEEE-FFFFFFF-GGGGGGG-HHHHHHH","uptime":3}"#)
        .create();
    assert_eq!(
        client(&s).my_id().unwrap(),
        "AAAAAAA-BBBBBBB-CCCCCCC-DDDDDDD-EEEEEEE-FFFFFFF-GGGGGGG-HHHHHHH"
    );
}

#[test]
fn devices_parse_and_preserve_unknown_fields() {
    let mut s = Server::new();
    s.mock("GET", "/rest/config/devices")
        .with_body(r#"[{"deviceID":"ID1","name":"mac","addresses":["dynamic"],"paused":false,"autoAcceptFolders":false,"compression":"metadata"}]"#)
        .create();
    let d = client(&s).devices().unwrap();
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].device_id, "ID1");
    assert_eq!(d[0].name, "mac");
    assert_eq!(d[0].extra.get("compression").unwrap(), "metadata");
}

#[test]
fn add_device_posts_json() {
    let mut s = Server::new();
    let m = s
        .mock("POST", "/rest/config/devices")
        .match_header("X-API-Key", KEY)
        .match_body(Matcher::PartialJsonString(
            r#"{"deviceID":"PEER","name":"windows","addresses":["tcp://100.64.0.2:22000"],"autoAcceptFolders":false}"#.into(),
        ))
        .create();
    client(&s)
        .add_device("PEER", "windows", &["tcp://100.64.0.2:22000"])
        .unwrap();
    m.assert();
}

#[test]
fn put_folder_sends_full_object_to_folder_url() {
    let mut s = Server::new();
    let m = s
        .mock("PUT", "/rest/config/folders/twin-docs")
        .match_body(Matcher::PartialJsonString(
            r#"{"id":"twin-docs","label":"Docs","path":"/x/Docs","type":"sendreceive","devices":[{"deviceID":"A"},{"deviceID":"B"}],"versioning":{"type":"staggered","params":{"maxAge":"2592000"}}}"#.into(),
        ))
        .create();
    let folder = Folder::new("twin-docs", "Docs", "/x/Docs", &["A", "B"])
        .with_versioning(Versioning::staggered_days(30));
    client(&s).put_folder(&folder).unwrap();
    m.assert();
}

#[test]
fn folder_devices_roundtrip() {
    let f = Folder::new("id", "l", "/p", &["A"]);
    assert_eq!(f.devices, vec![FolderDevice::new("A")]);
}

#[test]
fn pending_devices_parse_map() {
    let mut s = Server::new();
    s.mock("GET", "/rest/cluster/pending/devices")
        .with_body(r#"{"PEERID":{"time":"2026-11-16T01:00:00Z","name":"windows-pc","address":"192.168.1.5:22000"}}"#)
        .create();
    let p = client(&s).pending_devices().unwrap();
    assert_eq!(p.len(), 1);
    assert_eq!(p[0].device_id, "PEERID");
    assert_eq!(p[0].name, "windows-pc");
}

#[test]
fn pending_folders_parse_nested_map() {
    let mut s = Server::new();
    s.mock("GET", "/rest/cluster/pending/folders")
        .with_body(r#"{"twin-docs":{"offeredBy":{"PEERID":{"time":"2026-11-16T01:00:00Z","label":"Docs"}}}}"#)
        .create();
    let p = client(&s).pending_folders().unwrap();
    assert_eq!(p.len(), 1);
    assert_eq!(p[0].folder_id, "twin-docs");
    assert_eq!(p[0].offered_by, "PEERID");
    assert_eq!(p[0].label, "Docs");
}

#[test]
fn completion_passes_query_params() {
    let mut s = Server::new();
    s.mock("GET", "/rest/db/completion")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("folder".into(), "twin-docs".into()),
            Matcher::UrlEncoded("device".into(), "PEER".into()),
        ]))
        .with_body(r#"{"completion":42.5,"globalBytes":100,"needBytes":57,"globalItems":3,"needItems":1,"needDeletes":0,"remoteState":"valid","sequence":7}"#)
        .create();
    let c = client(&s).completion("twin-docs", Some("PEER")).unwrap();
    assert_eq!(c.completion, 42.5);
    assert_eq!(c.need_bytes, 57);
    assert_eq!(c.remote_state.as_deref(), Some("valid"));
}

#[test]
fn folder_status_parses_state() {
    let mut s = Server::new();
    s.mock("GET", "/rest/db/status")
        .match_query(Matcher::UrlEncoded("folder".into(), "twin-docs".into()))
        .with_body(r#"{"state":"syncing","needBytes":10,"globalBytes":20,"inSyncBytes":10,"errors":0,"pullErrors":0,"stateChanged":"2026-11-16T01:00:00Z"}"#)
        .create();
    let st = client(&s).folder_status("twin-docs").unwrap();
    assert_eq!(st.state, "syncing");
    assert_eq!(st.need_bytes, 10);
}

#[test]
fn events_long_poll_params() {
    let mut s = Server::new();
    s.mock("GET", "/rest/events")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("since".into(), "5".into()),
            Matcher::UrlEncoded("timeout".into(), "30".into()),
            Matcher::UrlEncoded("events".into(), "StateChanged,FolderCompletion".into()),
        ]))
        .with_body(r#"[{"id":6,"globalID":6,"time":"2026-11-16T01:00:00Z","type":"StateChanged","data":{"folder":"twin-docs","from":"idle","to":"syncing"}}]"#)
        .create();
    let ev = client(&s)
        .events(5, 30, &["StateChanged", "FolderCompletion"])
        .unwrap();
    assert_eq!(ev[0].id, 6);
    assert_eq!(ev[0].kind, "StateChanged");
    assert_eq!(ev[0].data["to"], "syncing");
}

#[test]
fn pause_and_resume_device() {
    let mut s = Server::new();
    let p = s
        .mock("POST", "/rest/system/pause")
        .match_query(Matcher::UrlEncoded("device".into(), "PEER".into()))
        .create();
    let r = s
        .mock("POST", "/rest/system/resume")
        .match_query(Matcher::UrlEncoded("device".into(), "PEER".into()))
        .create();
    let c = client(&s);
    c.pause_device("PEER").unwrap();
    c.resume_device("PEER").unwrap();
    p.assert();
    r.assert();
}

#[test]
fn connections_report_connected_flag() {
    let mut s = Server::new();
    s.mock("GET", "/rest/system/connections")
        .with_body(r#"{"connections":{"PEER":{"connected":true,"paused":false,"address":"100.64.0.2:22000","inBytesTotal":10,"outBytesTotal":20}},"total":{"inBytesTotal":10,"outBytesTotal":20}}"#)
        .create();
    let c = client(&s).connections().unwrap();
    assert!(c["PEER"].connected);
    assert_eq!(c["PEER"].in_bytes_total, 10);
}

#[test]
fn base_url_trailing_slash_is_tolerated() {
    let mut s = Server::new();
    let m = s
        .mock("GET", "/rest/system/ping")
        .with_body(r#"{"ping":"pong"}"#)
        .create();
    SyncthingClient::new(&format!("{}/", s.url()), KEY)
        .unwrap()
        .ping()
        .unwrap();
    m.assert();
}

// ---------- security regressions (audit 2026-11-16) ----------

#[test]
fn redirects_are_never_followed_so_the_key_cannot_leak() {
    let mut s = Server::new();
    let elsewhere = s.mock("GET", "/elsewhere").expect(0).create();
    s.mock("GET", "/rest/system/ping")
        .with_status(307)
        .with_header("Location", &format!("{}/elsewhere", s.url()))
        .create();
    assert!(matches!(
        client(&s).ping(),
        Err(Error::Http { status: 307, .. })
    ));
    elsewhere.assert();
}

#[test]
fn only_loopback_base_urls_are_accepted() {
    for ok in [
        "http://127.0.0.1:8384",
        "http://localhost:8384",
        "http://[::1]:8384",
        "https://127.0.0.1:8384/",
    ] {
        assert!(SyncthingClient::new(ok, KEY).is_ok(), "{ok}");
    }
    for bad in [
        "http://192.168.1.5:8384",
        "http://example.com",
        "http://127.0.0.1.evil.com:8384",
        "ftp://127.0.0.1",
        "127.0.0.1:8384",
        "http://user@evil.com@127.0.0.1",
    ] {
        assert!(
            matches!(SyncthingClient::new(bad, KEY), Err(Error::NotLoopback(_))),
            "{bad}"
        );
    }
}

#[test]
fn hostile_folder_ids_never_reach_the_url() {
    let mut s = Server::new();
    let any_put = s.mock("PUT", Matcher::Any).expect(0).create();
    for id in ["a/../options", "x?a=b", "x#", "", "a b", &"x".repeat(65)] {
        let f = Folder::new(id, "l", "/p", &["A"]);
        assert!(
            matches!(client(&s).put_folder(&f), Err(Error::InvalidFolderId(_))),
            "{id:?}"
        );
    }
    any_put.assert();
}

#[test]
fn error_bodies_are_truncated_and_stripped_of_control_chars() {
    let mut s = Server::new();
    let body = format!("\u{1b}[31mred{}", "x".repeat(5000));
    s.mock("GET", "/rest/system/status")
        .with_status(500)
        .with_body(body)
        .create();
    match client(&s).my_id() {
        Err(Error::Http { body, .. }) => {
            assert!(body.len() <= 520, "len {}", body.len());
            assert!(!body.chars().any(|c| c.is_control()));
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn accepting_an_offer_never_overwrites_an_existing_folder() {
    let mut s = Server::new();
    s.mock("GET", "/rest/config/folders")
        .with_body(r#"[{"id":"twin-docs","label":"Docs","path":"/x/Docs","type":"sendreceive","devices":[{"deviceID":"A"}]}]"#)
        .create();
    let put = s.mock("PUT", Matcher::Any).expect(0).create();
    let offer = PendingFolder {
        folder_id: "twin-docs".into(),
        offered_by: "EVIL".into(),
        label: "Docs".into(),
        time: String::new(),
    };
    let dir = std::env::temp_dir().join("twinfolders-unit-accept-existing");
    let r = pairing::accept_folder(&client(&s), &offer, "ME", &dir);
    assert!(matches!(r, Err(pairing::Error::AlreadyExists(_))), "{r:?}");
    put.assert();
    let _ = std::fs::remove_dir_all(dir);
}
