//! Estado compartilhado do app: estágios do fluxo e processos filhos.

use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};
use tokio::process::Child;

#[derive(Serialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Stage {
    Idle,
    Connecting,
    Up,
    Error,
}

#[derive(Serialize, Clone, Copy)]
pub struct Status {
    pub vpn: Stage,
    pub tunnel: Stage,
    pub rdp: Stage,
}

impl Default for Status {
    fn default() -> Self {
        Status {
            vpn: Stage::Idle,
            tunnel: Stage::Idle,
            rdp: Stage::Idle,
        }
    }
}

#[derive(Default)]
pub struct AppState {
    pub status: Mutex<Status>,
    pub vpn_child: Mutex<Option<Child>>,
    pub tunnel_child: Mutex<Option<Child>>,
    pub rdp_child: Mutex<Option<Child>>,
    /// URL vosession:// capturada da webview do portal, aguardando uso.
    pub pending_session: Mutex<Option<String>>,
}

/// Fonte de verdade das mudanças de estado: atualiza e avisa o frontend.
pub fn set_stage(app: &AppHandle, which: Which, stage: Stage) {
    let state = tauri::Manager::state::<AppState>(app);
    {
        let mut s = state.status.lock().unwrap();
        match which {
            Which::Vpn => s.vpn = stage,
            Which::Tunnel => s.tunnel = stage,
            Which::Rdp => s.rdp = stage,
        }
    }
    let snapshot = *state.status.lock().unwrap();
    let _ = app.emit("status", snapshot);
}

#[derive(Clone, Copy)]
pub enum Which {
    Vpn,
    Tunnel,
    Rdp,
}

/// Linha de log para a aba de logs do app.
pub fn log(app: &AppHandle, line: impl Into<String>) {
    let line = line.into();
    append_log_file(&line);
    let _ = app.emit("log", line);
}

/// Aviso para o usuário: vai para o log e aparece em destaque no Painel.
pub fn notice(app: &AppHandle, text: impl Into<String>) {
    let text = text.into();
    log(app, format!("[aviso] {text}"));
    let _ = app.emit("notice", text);
}

/// O que dizer quando o bastion não consegue chegar na VM.
pub const VM_OFF: &str = "Não deu para falar com a VM: provavelmente ela está desligada (ou \
    reiniciando). Ligue-a e tente de novo.";

/// Cópia dos logs em ~/.local/share/fast-jump-rdp/app.log, para dar para ler
/// fora do app (diagnóstico). Recomeça quando passa de 2 MB.
fn append_log_file(line: &str) {
    use std::io::Write;
    // várias tasks logam ao mesmo tempo: uma linha por vez
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _g = LOCK.lock().unwrap();
    let Ok(dir) = crate::config::data_dir() else { return };
    let p = dir.join("app.log");
    if std::fs::metadata(&p).map(|m| m.len() > 2_000_000).unwrap_or(false) {
        let _ = std::fs::rename(&p, dir.join("app.log.old"));
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&p) {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let _ = writeln!(f, "{ts} {line}");
    }
}
