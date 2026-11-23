//! "Segurança deste computador": a short checklist the person can act on.
//! Each check is read without administrator rights; when a check cannot be
//! read it says so instead of guessing.

use std::process::Command;

use serde::Serialize;

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub id: &'static str,
    /// `Some(true)` ok, `Some(false)` needs attention, `None` unknown.
    pub ok: Option<bool>,
    pub title: String,
    pub detail: String,
}

/// What the checks are built from.
#[derive(Default, Debug, Clone)]
pub struct Inputs {
    pub os: &'static str,
    /// FileVault / BitLocker on the system disk.
    pub disk_encrypted: Option<bool>,
    /// Syncthing GUI/API reachable only from this computer.
    pub gui_loopback: Option<bool>,
    /// GUI over HTTPS with a password.
    pub gui_protected: Option<bool>,
    /// Tailscale connected (needed away from the office network).
    pub tailscale_running: Option<bool>,
}

pub fn build(i: &Inputs) -> Vec<Check> {
    let disk = if i.os == "macos" {
        "FileVault"
    } else {
        "BitLocker"
    };
    let where_disk = if i.os == "macos" {
        "Ajustes do Sistema → Privacidade e Segurança → FileVault"
    } else {
        "Configurações → Privacidade e segurança → Criptografia do dispositivo (ou BitLocker no Painel de Controle)"
    };
    vec![
        Check {
            id: "disk",
            ok: i.disk_encrypted,
            title: format!("Disco criptografado ({disk})"),
            detail: match i.disk_encrypted {
                Some(true) => "Se o computador for perdido ou roubado, os arquivos não podem ser lidos sem a sua senha.".into(),
                Some(false) => format!("Ligue o {disk} em {where_disk}. Sem isso, quem pegar o computador lê as pastas sincronizadas."),
                None => format!("Não foi possível conferir o {disk} daqui. Confira em {where_disk}."),
            },
        },
        Check {
            id: "local-only",
            ok: i.gui_loopback,
            title: "Painel do Syncthing só neste computador".into(),
            detail: match i.gui_loopback {
                Some(true) => "O controle da sincronização só aceita conexões deste próprio computador.".into(),
                Some(false) => "O painel do Syncthing aceita conexões de fora. Religue o serviço pela tela de primeira abertura para voltar ao padrão seguro.".into(),
                None => "O serviço em segundo plano não está ligado, então não há o que conferir.".into(),
            },
        },
        Check {
            id: "gui-protected",
            ok: i.gui_protected,
            title: "Painel protegido por senha e HTTPS".into(),
            detail: match i.gui_protected {
                Some(true) => "Mesmo neste computador, o painel pede senha e usa conexão criptografada.".into(),
                Some(false) => "O painel está sem senha ou sem HTTPS. Religue o serviço para corrigir.".into(),
                None => "Não foi possível ler a configuração do serviço.".into(),
            },
        },
        Check {
            id: "tailscale",
            ok: i.tailscale_running,
            title: "Rede privada (Tailscale)".into(),
            detail: match i.tailscale_running {
                Some(true) => "Conectado: os computadores se encontram de qualquer lugar sem abrir portas na internet.".into(),
                Some(false) => "Desligado: fora da mesma rede, os computadores não se encontram. Abra o Tailscale e entre na conta.".into(),
                None => "Não instalado. Só é preciso para sincronizar fora da mesma rede ou abrir terminal no outro computador.".into(),
            },
        },
    ]
}

fn run(prog: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(prog).args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// FileVault / BitLocker status, read without admin rights.
pub fn disk_encrypted() -> Option<bool> {
    if cfg!(target_os = "macos") {
        let s = run("fdesetup", &["status"])?;
        if s.contains("FileVault is On") {
            Some(true)
        } else if s.contains("FileVault is Off") {
            Some(false)
        } else {
            None
        }
    } else if cfg!(windows) {
        // Shell property readable by a normal user: 1 = on, 3 = encrypting,
        // 5 = on (unlocked), 2 = off; anything else is unknown.
        let s = run(
            "powershell",
            &[
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "(New-Object -ComObject Shell.Application).NameSpace($env:SystemDrive + '\\').Self.ExtendedProperty('System.Volume.BitLockerProtection')",
            ],
        )?;
        match s.trim() {
            "1" | "3" | "5" => Some(true),
            "2" => Some(false),
            _ => None,
        }
    } else {
        None
    }
}

/// True when the GUI address in Syncthing's config is loopback-only.
pub fn is_loopback(address: &str) -> bool {
    let host = address
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .rsplit_once(':')
        .map(|(h, _)| h)
        .unwrap_or(address)
        .trim_matches(['[', ']']);
    matches!(host, "127.0.0.1" | "::1" | "localhost")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_addresses() {
        assert!(is_loopback("127.0.0.1:8385"));
        assert!(is_loopback("https://127.0.0.1:8385"));
        assert!(is_loopback("[::1]:8385"));
        assert!(!is_loopback("0.0.0.0:8385"));
        assert!(!is_loopback("192.168.0.10:8385"));
    }

    #[test]
    fn checks_say_what_to_do_and_admit_unknowns() {
        let c = build(&Inputs {
            os: "windows",
            disk_encrypted: Some(false),
            gui_loopback: Some(true),
            gui_protected: None,
            tailscale_running: None,
        });
        assert_eq!(c[0].ok, Some(false));
        assert!(c[0].title.contains("BitLocker"));
        assert!(c[0].detail.contains("Criptografia do dispositivo"));
        assert_eq!(c[2].ok, None);
        assert!(c[3].detail.contains("Não instalado"));
        let mac = build(&Inputs {
            os: "macos",
            disk_encrypted: Some(true),
            ..Default::default()
        });
        assert!(mac[0].title.contains("FileVault"));
    }
}
