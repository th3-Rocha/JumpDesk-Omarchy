//! VPN: gera o TOTP, monta usuário/senha+código e sobe o openvpn via helper
//! root (pkexec). As credenciais vão por stdin do helper, nunca em argv.
//!
//! O helper `/usr/local/libexec/fast-jump-rdp-vpn` é instalado uma vez (ver
//! packaging/). Ele lê 2 linhas no stdin (usuário, senha+código) e executa o
//! openvpn com as flags/rotas passadas como argumentos.

use crate::config::Config;
use crate::error::{msg, Result};
use crate::secrets::{self, Key};
use crate::state::{log, set_stage, AppState, Stage, Which};
use crate::totp;
use std::process::Stdio;
use tauri::{AppHandle, Manager};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

pub const HELPER: &str = "/usr/local/libexec/fast-jump-rdp-vpn";

/// Monta os argumentos do openvpn.
///
/// trust_server = true  → aceita rotas e DNS que o servidor empurra (como o
///                        colega; "mudou lá, pego sozinho").
/// trust_server = false → ignora o que o servidor manda (`--route-nopull`),
///                        usa a lista manual de rotas e bloqueia o DNS deles.
fn openvpn_args(cfg: &Config) -> Vec<String> {
    let mut a = vec![
        "--config".into(),
        cfg.vpn.ovpn_path.clone(),
        "--auth-user-pass".into(),
        "/dev/stdin".into(),
    ];

    if !cfg.vpn.trust_server {
        a.push("--pull-filter".into());
        a.push("ignore".into());
        a.push("dhcp-option DNS".into());
        a.push("--route-nopull".into());
        for r in &cfg.vpn.routes {
            a.push("--route".into());
            // cada rota é "rede máscara gateway" — expandimos em tokens
            for tok in r.split_whitespace() {
                a.push(tok.into());
            }
        }
    }

    a.push("--tun-mtu".into());
    a.push(cfg.vpn.tun_mtu.to_string());
    a.push("--mssfix".into());
    a.push(cfg.vpn.mssfix.to_string());
    a
}

/// Há um openvpn nosso rodando (deste app ou de uma execução anterior)?
fn stray_openvpn() -> bool {
    let Ok(dir) = std::fs::read_dir("/proc") else {
        return false;
    };
    dir.flatten().any(|e| {
        std::fs::read(e.path().join("cmdline")).is_ok_and(|c| {
            let c = String::from_utf8_lossy(&c);
            c.contains("openvpn") && c.contains("fast-jump-rdp-auth")
        })
    })
}

pub async fn connect(app: AppHandle, cfg: &Config) -> Result<()> {
    // Já subiu por este app e o openvpn segue vivo: não abre uma segunda VPN
    // (duas na mesma sub-rede bagunçam as rotas e o túnel fica lento).
    let running = app
        .state::<AppState>()
        .vpn_child
        .lock()
        .unwrap()
        .as_mut()
        .is_some_and(|c| c.try_wait().ok().flatten().is_none());
    if running {
        log(&app, "[vpn] já está de pé; não abri outra.");
        return Ok(());
    }
    if stray_openvpn() {
        log(&app, "[vpn] encerrando o openvpn que sobrou de antes…");
        let _ = Command::new("pkexec").arg(HELPER).arg("--stop").output().await;
    }

    set_stage(&app, Which::Vpn, Stage::Connecting);

    let user = cfg.vpn.username.clone();
    let pass = secrets::get(Key::VpnPassword)?
        .filter(|p| !p.is_empty())
        .ok_or_else(|| msg("senha da VPN não configurada (Perfis RDP → Senha da VPN)"))?;
    let seed = secrets::get(Key::VpnSeed)?
        .ok_or_else(|| msg("seed do MFA da VPN não configurado"))?;
    let code = totp::generate(&seed)?;
    let full = format!("{pass}{code}");

    if !std::path::Path::new(HELPER).exists() {
        set_stage(&app, Which::Vpn, Stage::Error);
        return Err(msg(format!(
            "helper da VPN não instalado em {HELPER}. Rode a instalação do assistente."
        )));
    }

    log(&app, "[vpn] gerando código MFA e iniciando openvpn…");

    // pkexec HELPER <args do openvpn>  — credenciais vão pelo stdin (3 linhas:
    // usuário, senha+código, e uma linha em branco de guarda).
    let mut cmd = Command::new("pkexec");
    cmd.arg(HELPER);
    for a in openvpn_args(cfg) {
        cmd.arg(a);
    }
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            set_stage(&app, Which::Vpn, Stage::Error);
            return Err(msg(format!("falha ao iniciar pkexec: {e}")));
        }
    };

    if let Some(mut stdin) = child.stdin.take() {
        let payload = format!("{user}\n{full}\n");
        stdin.write_all(payload.as_bytes()).await?;
        stdin.flush().await?;
        // fecha o stdin
        drop(stdin);
    }

    // O estágio fica "conectando" até o openvpn dizer que subiu (ver o leitor).
    spawn_log_reader(app.clone(), &mut child);

    let state = app.state::<AppState>();
    *state.vpn_child.lock().unwrap() = Some(child);
    Ok(())
}

fn spawn_log_reader(app: AppHandle, child: &mut tokio::process::Child) {
    use tokio::io::{AsyncBufReadExt, BufReader};
    if let Some(out) = child.stdout.take() {
        let app2 = app.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(out).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                if l.contains("Initialization Sequence Completed") {
                    set_stage(&app2, Which::Vpn, Stage::Up);
                } else if l.contains("AUTH_FAILED") {
                    log(&app2, "[vpn] servidor recusou usuário/senha+código (AUTH_FAILED).");
                }
                log(&app2, format!("[vpn] {l}"));
            }
            // stdout fechou: o openvpn terminou. Se não foi o usuário que
            // desconectou, é erro — e o painel tem que mostrar isso.
            let state = app2.state::<AppState>();
            let child = state.vpn_child.lock().unwrap().take();
            if let Some(mut c) = child {
                let code = c.wait().await.ok().and_then(|s| s.code());
                set_stage(&app2, Which::Vpn, Stage::Error);
                log(&app2, format!("[vpn] openvpn encerrou (código {code:?}); veja as linhas acima."));
            }
        });
    }
    if let Some(err) = child.stderr.take() {
        tokio::spawn(async move {
            let mut lines = BufReader::new(err).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                log(&app, format!("[vpn!] {l}"));
            }
        });
    }
}

pub async fn disconnect(app: AppHandle) -> Result<()> {
    let state = app.state::<AppState>();
    let child = state.vpn_child.lock().unwrap().take();
    // O openvpn roda como root: o kill() do app não o alcança. Quem encerra é
    // o helper (`--stop`), que também pega openvpn solto de tentativas antigas.
    let out = Command::new("pkexec").arg(HELPER).arg("--stop").output().await;
    if let Some(mut c) = child {
        let _ = c.wait().await;
    }
    set_stage(&app, Which::Vpn, Stage::Idle);
    match out {
        Ok(o) if o.status.success() => {
            log(&app, "[vpn] desconectada.");
            Ok(())
        }
        Ok(o) => Err(msg(format!(
            "não consegui encerrar o openvpn ({}). Reinstale o helper: \
             sudo packaging/install-system.sh",
            String::from_utf8_lossy(&o.stderr).trim()
        ))),
        Err(e) => Err(msg(format!("falha ao rodar pkexec: {e}"))),
    }
}
