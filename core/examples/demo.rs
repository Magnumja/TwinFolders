//! Local demo: two hardened Syncthing instances ("mac" and "windows") on
//! this machine, paired, with folders in different states, for trying the
//! app without two computers.
//!
//!   cargo run -p twinfolders-core --example demo
//!   # then, in another terminal (prints the exact command):
//!   TWINFOLDERS_HOME=<printed path> npm --prefix app run tauri dev
//!
//! Everything lives in the system temp dir; press Enter to stop both
//! daemons. No discovery/relays/NAT (hardened config), loopback only.

use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use twinfolders_core::client::{Folder, SyncthingClient, Versioning};
use twinfolders_core::service::{self, Os, ServiceConfig};
use twinfolders_core::stignore;

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn wait<F: FnMut() -> bool>(what: &str, mut f: F) {
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(60) {
        if f() {
            return;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    panic!("timed out: {what}");
}

struct Node {
    cfg: ServiceConfig,
    client: SyncthingClient,
    id: String,
    child: Child,
}

fn start(bin: &str, root: &Path, name: &str) -> Node {
    let cfg = ServiceConfig {
        syncthing_bin: bin.into(),
        home: root.join(name).join("syncthing"),
        log_dir: root.join(name).join("logs"),
        gui_port: free_port(),
        listen_port: free_port(),
    };
    let p = service::provision_home(&cfg).expect("provision");
    let child = Command::new(bin)
        .args(cfg.serve_args(Os::current().unwrap_or(Os::MacOs)))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn syncthing");
    let client = service::client_for(&cfg, &p.gui.api_key).expect("client");
    wait("REST API", || client.ping().is_ok());
    let id = client.my_id().unwrap();
    Node {
        cfg,
        client,
        id,
        child,
    }
}

fn share(nodes: &[&Node], id: &str, label: &str, extra_device: Option<&str>) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for n in nodes {
        let path = n.cfg.home.parent().unwrap().join(label);
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join(".stignore"), stignore::render(&[])).unwrap();
        let mut ids: Vec<&str> = nodes.iter().map(|x| x.id.as_str()).collect();
        if let Some(e) = extra_device {
            ids.push(e);
        }
        let mut f = Folder::new(id, label, path.to_str().unwrap(), &ids)
            .with_versioning(Versioning::staggered_days(30));
        f.extra
            .insert("fsWatcherDelayS".into(), serde_json::json!(1));
        n.client.put_folder(&f).unwrap();
        paths.push(path);
    }
    paths
}

fn main() {
    let bin = std::env::var("TWINFOLDERS_SYNCTHING").unwrap_or_else(|_| "syncthing".into());
    let root = std::env::temp_dir().join("twinfolders-demo");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();

    println!("Iniciando dois Syncthing locais (mac e windows)...");
    let mac = start(&bin, &root, "mac");
    let win = start(&bin, &root, "windows");

    // An "old laptop" that exists but is never online: shows WaitingForPeer.
    let laptop_home = root.join("notebook-antigo");
    Command::new(&bin)
        .args([
            "generate",
            &format!("--home={}", laptop_home.display()),
            "--no-port-probing",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    let laptop_id = String::from_utf8(
        Command::new(&bin)
            .args(["device-id", &format!("--home={}", laptop_home.display())])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_string();

    mac.client
        .add_device(
            &win.id,
            "Windows (escritório)",
            &[&format!("tcp://127.0.0.1:{}", win.cfg.listen_port)],
        )
        .unwrap();
    mac.client
        .add_device(&laptop_id, "Notebook antigo", &["tcp://127.0.0.1:1"])
        .unwrap();
    win.client
        .add_device(
            &mac.id,
            "MacBook",
            &[&format!("tcp://127.0.0.1:{}", mac.cfg.listen_port)],
        )
        .unwrap();
    wait("pareamento", || {
        mac.client
            .connections()
            .map(|c| c.get(&win.id).map(|x| x.connected).unwrap_or(false))
            .unwrap_or(false)
    });

    let docs = share(&[&mac, &win], "twin-documentos", "Documentos", None);
    for (i, name) in ["contrato.pdf", "notas.md", "orcamento.xlsx"]
        .iter()
        .enumerate()
    {
        fs::write(docs[0].join(name), vec![b'x'; 40_000 * (i + 1)]).unwrap();
    }
    let fotos = share(&[&mac], "twin-fotos", "Fotos", Some(&laptop_id));
    fs::write(fotos[0].join("praia.jpg"), vec![7u8; 900_000]).unwrap();
    let proj = share(&[&mac, &win], "twin-projetos", "Projetos", None);
    mac.client.scan("twin-documentos").unwrap();
    mac.client.scan("twin-fotos").unwrap();

    // A big file keeps "Projetos" syncing for a while.
    let big = proj[0].join("video-render.mov");
    let chunk = vec![3u8; 8 * 1024 * 1024];
    let mut data = Vec::with_capacity(chunk.len() * 96);
    for i in 0..96u8 {
        let mut c = chunk.clone();
        c[0] = i; // avoid identical blocks being deduplicated
        data.extend_from_slice(&c);
    }
    fs::write(&big, &data).unwrap();
    mac.client.scan("twin-projetos").unwrap();

    println!();
    println!("Pronto. Em outro terminal, abra o app apontando para o \"mac\":");
    println!();
    println!(
        "  TWINFOLDERS_HOME='{}' npm --prefix app run tauri dev",
        mac.cfg.home.display()
    );
    println!();
    println!("Pressione Enter para encerrar os dois Syncthing.");

    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
    for mut n in [mac, win] {
        let _ = n.client.shutdown();
        let _ = n.child.wait();
    }
}
