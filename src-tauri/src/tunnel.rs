//! Túnel: roda o VaultOneRDPConnector com a URL vosession:// e espera a porta
//! local 3390 abrir (o connector fala com o bastion do JumpCloud).

use crate::connector;
use crate::error::{msg, Result};
use crate::state::{log, notice, set_stage, AppState, Stage, Which, VM_OFF};
use std::process::Stdio;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tokio::net::TcpStream;
use tokio::process::Command;

pub const PORT: u16 = 3390;

/// Sobe o connector e espera a porta. Retorna erro se não abrir a tempo.
pub async fn start(app: AppHandle, session_url: &str) -> Result<()> {
    set_stage(&app, Which::Tunnel, Stage::Connecting);

    let Some(bin) = connector::installed_path() else {
        set_stage(&app, Which::Tunnel, Stage::Error);
        return Err(msg("VaultOneRDPConnector não instalado/verificado"));
    };

    log(&app, "[túnel] iniciando VaultOneRDPConnector…");
    // O connector abre um xfreerdp3 próprio, sem senha, que pede a senha no
    // terminal. Sem terminal ele cancela na hora e o connector derruba o túnel
    // junto. No fluxo antigo isso rodava num ghostty; aqui o `script` dá um
    // pseudo-terminal, e o prompt fica esperando (sem janela) até o túnel
    // ser encerrado. Não dá para fechar esse xfreerdp3 antes: quando ele sai,
    // o connector derruba o túnel, mesmo com o nosso RDP conectado.
    // Binário e URL vão por env, sem passar por quoting.
    let mut cmd = Command::new("script");
    cmd.args(["-qfec", r#"exec "$FJR_CONNECTOR" "$FJR_URL""#, "/dev/null"])
        .env("FJR_CONNECTOR", &bin)
        .env("FJR_URL", session_url)
        .env("DOTNET_SYSTEM_NET_DISABLEIPV6", "true")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| msg(format!("falha ao iniciar o connector: {e}")))?;

    // O stdin do `script` fica aberto (e mudo) enquanto o connector viver: se
    // fechar, o `script` manda EOF ao prompt do xfreerdp3 e ele cancela.
    let stdin = child.stdin.take();

    // logs do connector (pelo pty, stdout e stderr chegam juntos aqui)
    use tokio::io::{AsyncBufReadExt, BufReader};
    if let Some(out) = child.stdout.take() {
        let a = app.clone();
        tokio::spawn(async move {
            let _stdin = stdin;
            let mut lines = BufReader::new(out).lines();
            let mut avisou = false;
            while let Ok(Some(l)) = lines.next_line().await {
                let l = strip_ansi(l.trim_end_matches('\r'));
                let l = l.as_str();
                log(&a, format!("[túnel] {l}"));
                if !avisou && l.contains("Error openning port") {
                    // O bastion aceitou, mas não alcançou a VM (desligada?).
                    avisou = true;
                    set_stage(&a, Which::Tunnel, Stage::Error);
                    notice(&a, VM_OFF);
                } else if !avisou && l.contains("ERRCONNECT_CONNECT_TRANSPORT_FAILED") {
                    // O connector abriu a porta, mas a conexão até a VM não
                    // respondeu: o mesmo sintoma de VM desligada.
                    avisou = true;
                    set_stage(&a, Which::Tunnel, Stage::Error);
                    notice(&a, VM_OFF);
                } else if l.contains("Error trying to connect to bastion") {
                    set_stage(&a, Which::Tunnel, Stage::Error);
                    log(&a, "[túnel] não alcancei o bastion — a VPN está de pé?");
                }
            }
            // stdout fechou: o connector terminou. Recolhe o processo (sem
            // zumbi); se não foi o usuário que parou, o túnel caiu.
            let child = a.state::<AppState>().tunnel_child.lock().unwrap().take();
            if let Some(mut c) = child {
                let _ = c.wait().await;
                set_stage(&a, Which::Tunnel, Stage::Idle);
                log(&a, "[túnel] connector encerrou.");
            }
        });
    }
    if let Some(err) = child.stderr.take() {
        let a = app.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(err).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                log(&a, format!("[túnel!] {l}"));
            }
        });
    }

    app.state::<AppState>()
        .tunnel_child
        .lock()
        .unwrap()
        .replace(child);

    // espera a porta 3390 por até ~30s
    if wait_port(Duration::from_secs(30)).await {
        set_stage(&app, Which::Tunnel, Stage::Up);
        log(&app, "[túnel] porta 3390 pronta.");
        Ok(())
    } else {
        set_stage(&app, Which::Tunnel, Stage::Error);
        Err(msg("o túnel não abriu a porta 3390 a tempo"))
    }
}

async fn wait_port(timeout: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;
    while tokio::time::Instant::now() < deadline {
        if TcpStream::connect(("127.0.0.1", PORT)).await.is_ok() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    false
}

pub async fn is_up() -> bool {
    TcpStream::connect(("127.0.0.1", PORT)).await.is_ok()
}

pub async fn stop(app: AppHandle) -> Result<()> {
    let child = app.state::<AppState>().tunnel_child.lock().unwrap().take();
    if let Some(mut c) = child {
        // o filho direto é o `script`: encerra antes o connector e o xfreerdp3 dele
        if let Some(pid) = c.id() {
            for d in descendants(pid) {
                let _ = Command::new("kill").arg(d.to_string()).status().await;
            }
        }
        let _ = c.kill().await;
        log(&app, "[túnel] encerrado.");
    }
    set_stage(&app, Which::Tunnel, Stage::Idle);
    Ok(())
}

/// PIDs descendentes de `root`, lendo o ppid em /proc/<pid>/stat.
fn descendants(root: u32) -> Vec<u32> {
    let mut parent = Vec::new();
    if let Ok(dir) = std::fs::read_dir("/proc") {
        for e in dir.flatten() {
            let Ok(pid) = e.file_name().to_string_lossy().parse::<u32>() else { continue };
            let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else { continue };
            // o comm vem entre parênteses e pode ter espaços: o ppid é o 2º campo após o ')'
            let Some(rest) = stat.rsplit_once(')').map(|(_, r)| r) else { continue };
            if let Some(ppid) = rest.split_whitespace().nth(1).and_then(|x| x.parse::<u32>().ok()) {
                parent.push((pid, ppid));
            }
        }
    }
    let mut out = Vec::new();
    let mut frontier = vec![root];
    while let Some(p) = frontier.pop() {
        for &(c, pp) in &parent {
            if pp == p && !out.contains(&c) {
                out.push(c);
                frontier.push(c);
            }
        }
    }
    out
}

/// Tira as sequências de escape que o pty manda junto com o texto.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c != '\x1b' {
            out.push(c);
            continue;
        }
        match it.next() {
            // CSI: ESC [ ... até a letra final
            Some('[') => {
                while let Some(&n) = it.peek() {
                    it.next();
                    if ('@'..='~').contains(&n) {
                        break;
                    }
                }
            }
            // ESC + um caractere (ex.: ESC =)
            _ => {}
        }
    }
    out
}
