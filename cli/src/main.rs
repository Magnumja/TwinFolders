//! `twinfolders` CLI. Slice 3 scope: background service management.
//!
//!   twinfolders service install [--syncthing PATH] [--gui-port N] [--listen-port N]
//!   twinfolders service status
//!   twinfolders service uninstall
//!
//! Output never contains the API key.

use std::path::PathBuf;
use std::process::ExitCode;

use twinfolders_core::service::{self, ServiceConfig};

const USAGE: &str = "uso:
  twinfolders service install [--syncthing CAMINHO] [--gui-port N] [--listen-port N]
  twinfolders service status
  twinfolders service uninstall";

#[derive(Debug, PartialEq, Eq)]
enum Cmd {
    Install {
        syncthing: Option<PathBuf>,
        gui_port: Option<u16>,
        listen_port: Option<u16>,
    },
    Status,
    Uninstall,
}

fn parse(args: &[String]) -> Result<Cmd, String> {
    let mut it = args.iter().map(String::as_str);
    if it.next() != Some("service") {
        return Err(USAGE.into());
    }
    let sub = it.next().ok_or(USAGE)?;
    let rest: Vec<&str> = it.collect();
    match sub {
        "status" | "uninstall" if !rest.is_empty() => {
            Err(format!("argumento inesperado: {}", rest[0]))
        }
        "status" => Ok(Cmd::Status),
        "uninstall" => Ok(Cmd::Uninstall),
        "install" => {
            let (mut syncthing, mut gui_port, mut listen_port) = (None, None, None);
            let mut i = 0;
            while i < rest.len() {
                let val = rest
                    .get(i + 1)
                    .ok_or_else(|| format!("{} precisa de um valor", rest[i]))?;
                match rest[i] {
                    "--syncthing" => syncthing = Some(PathBuf::from(val)),
                    "--gui-port" => gui_port = Some(port(val)?),
                    "--listen-port" => listen_port = Some(port(val)?),
                    other => return Err(format!("opção desconhecida: {other}\n{USAGE}")),
                }
                i += 2;
            }
            Ok(Cmd::Install {
                syncthing,
                gui_port,
                listen_port,
            })
        }
        _ => Err(USAGE.into()),
    }
}

fn port(v: &str) -> Result<u16, String> {
    match v.parse::<u16>() {
        Ok(p) if p >= 1024 => Ok(p),
        _ => Err(format!("porta inválida: {v} (use 1024-65535)")),
    }
}

/// Finds the Syncthing binary: explicit path, then PATH, then common
/// install locations (Homebrew, winget).
fn find_syncthing(explicit: Option<PathBuf>) -> Result<PathBuf, String> {
    if let Some(p) = explicit {
        return if service::is_executable(&p) {
            Ok(p)
        } else {
            Err(format!("Syncthing não encontrado em {}", p.display()))
        };
    }
    let exe = if cfg!(windows) {
        "syncthing.exe"
    } else {
        "syncthing"
    };
    let mut candidates: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).map(|d| d.join(exe)).collect())
        .unwrap_or_default();
    candidates.push("/opt/homebrew/bin/syncthing".into());
    candidates.push("/usr/local/bin/syncthing".into());
    if let Some(l) = std::env::var_os("LOCALAPPDATA") {
        candidates.push(PathBuf::from(l).join("Microsoft\\WinGet\\Links\\syncthing.exe"));
    }
    candidates
        .into_iter()
        .find(|p| service::is_executable(p))
        .ok_or_else(|| {
            "Syncthing não encontrado. Instale com `brew install syncthing` (Mac) ou `winget install Syncthing.Syncthing` (Windows), ou use --syncthing CAMINHO.".into()
        })
}

fn run(cmd: Cmd) -> Result<(), String> {
    match cmd {
        Cmd::Install {
            syncthing,
            gui_port,
            listen_port,
        } => {
            let bin = find_syncthing(syncthing)?;
            let mut cfg = ServiceConfig::for_current_user(bin).map_err(|e| e.to_string())?;
            if let Some(p) = gui_port {
                cfg.gui_port = p;
            }
            if let Some(p) = listen_port {
                cfg.listen_port = p;
            }
            println!("Instalando serviço em segundo plano...");
            println!("  Syncthing: {}", cfg.syncthing_bin.display());
            println!("  Pasta de dados: {}", cfg.home.display());
            let id = service::install(&cfg).map_err(|e| e.to_string())?;
            println!("✔ Serviço instalado e rodando.");
            println!("  Painel/API (só local): {}", cfg.gui_url());
            println!("  ID deste dispositivo: {id}");
            Ok(())
        }
        Cmd::Status => {
            let cfg = ServiceConfig::for_current_user(find_syncthing(None).unwrap_or_default())
                .map_err(|e| e.to_string())?;
            let s = service::status(&cfg).map_err(|e| e.to_string())?;
            println!("instalado: {}", if s.installed { "sim" } else { "não" });
            println!("rodando:   {}", if s.running { "sim" } else { "não" });
            if let Some(pid) = s.pid {
                println!("pid:       {pid}");
            }
            println!("api:       {}", s.gui_url);
            if let Some(id) = s.device_id {
                println!("device id: {id}");
            }
            Ok(())
        }
        Cmd::Uninstall => {
            let cfg = ServiceConfig::for_current_user(find_syncthing(None).unwrap_or_default())
                .map_err(|e| e.to_string())?;
            service::uninstall(&cfg).map_err(|e| e.to_string())?;
            println!(
                "✔ Serviço removido. Pastas sincronizadas e dados do Syncthing foram mantidos em:"
            );
            println!("  {}", cfg.home.display());
            Ok(())
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse(&args).and_then(run) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("erro: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn parses_commands() {
        assert_eq!(parse(&a("service status")), Ok(Cmd::Status));
        assert_eq!(parse(&a("service uninstall")), Ok(Cmd::Uninstall));
        assert_eq!(
            parse(&a(
                "service install --gui-port 9000 --syncthing /x/syncthing"
            )),
            Ok(Cmd::Install {
                syncthing: Some("/x/syncthing".into()),
                gui_port: Some(9000),
                listen_port: None
            })
        );
    }

    #[test]
    fn rejects_bad_input() {
        assert!(parse(&a("")).is_err());
        assert!(parse(&a("service")).is_err());
        assert!(parse(&a("service install --gui-port")).is_err());
        assert!(
            parse(&a("service install --gui-port 80")).is_err(),
            "privileged port"
        );
        assert!(parse(&a("service install --nope 1")).is_err());
        assert!(parse(&a("service status extra")).is_err());
    }
}
