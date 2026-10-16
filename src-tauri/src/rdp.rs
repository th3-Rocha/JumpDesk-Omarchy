//! RDP: monta o comando xfreerdp3 a partir do perfil ativo e o executa
//! contra o túnel local (127.0.0.1:3390).

use crate::config::RdpProfile;
use crate::error::{msg, Result};
use crate::secrets::{self, Key};
use crate::state::{log, notice, set_stage, AppState, Stage, Which, VM_OFF};
use crate::tunnel;
use std::process::Stdio;
use tauri::{AppHandle, Manager};
use tokio::process::Command;

/// Constrói a lista de argumentos do xfreerdp3 para um perfil.
pub fn build_args(p: &RdpProfile, windows_password: Option<&str>) -> Vec<String> {
    let mut a: Vec<String> = vec![
        format!("/v:127.0.0.1:{}", tunnel::PORT),
        format!("/d:{}", p.domain),
        format!("/u:{}", p.username),
    ];

    // senha: real se o usuário salvou; senão "-" (força a tela de login do Windows)
    match windows_password {
        Some(pw) if !pw.is_empty() => a.push(format!("/p:{pw}")),
        _ => a.push("/p:-".into()),
    }

    a.push("/client-hostname:VaultOne".into());
    a.push("/sec:tls".into());
    a.push("/cert:ignore".into());

    // janela / monitores
    match p.window_mode.as_str() {
        "fullscreen" => a.push("/f".into()),
        "dynamic" => a.push("/dynamic-resolution".into()),
        _ => a.push(format!("/size:{}x{}", p.width, p.height)),
    }
    match p.monitor_mode.as_str() {
        "all" => a.push("/multimon".into()),
        "list" if !p.monitor_ids.is_empty() => a.push(format!("/monitors:{}", p.monitor_ids)),
        _ => {}
    }

    // qualidade
    a.push("/network:wan".into());
    a.push("/bpp:32".into());
    match p.quality.as_str() {
        "fast" => {
            a.push("/gfx:AVC420".into());
            a.push("-wallpaper".into());
            a.push("-themes".into());
            a.push("-menu-anims".into());
            a.push("-window-drag".into());
            a.push("+fonts".into());
        }
        "max" => {
            a.push("/gfx:AVC444".into());
            a.push("+wallpaper".into());
            a.push("+themes".into());
            a.push("+fonts".into());
        }
        _ => {
            // balanced (igual ao fluxo antigo)
            a.push("/gfx:AVC444".into());
            a.push("+fonts".into());
            a.push("-wallpaper".into());
            a.push("-window-drag".into());
            a.push("-menu-anims".into());
            a.push("-themes".into());
        }
    }

    if p.clipboard {
        a.push("/clipboard".into());
    }
    if p.sound {
        a.push("/sound:sys:pulse".into());
    }
    if p.microphone {
        a.push("/microphone:sys:pulse".into());
    }
    if !p.shared_folder.is_empty() {
        a.push(format!("/drive:compartilhada,{}", p.shared_folder));
    }
    for tok in p.extra_flags.split_whitespace() {
        a.push(tok.into());
    }
    a
}

pub async fn open(app: AppHandle, p: &RdpProfile) -> Result<()> {
    set_stage(&app, Which::Rdp, Stage::Connecting);

    if !tunnel::is_up().await {
        set_stage(&app, Which::Rdp, Stage::Error);
        return Err(msg("túnel não está de pé; conecte o túnel antes do RDP"));
    }

    let pw = secrets::get(Key::WindowsPassword).ok().flatten();
    let args = build_args(p, pw.as_deref());
    log(&app, format!("[rdp] xfreerdp3 {}", redact(&args)));

    let mut child = Command::new("xfreerdp3")
        .args(&args)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| msg(format!("falha ao iniciar o xfreerdp3: {e}")))?;

    use tokio::io::{AsyncBufReadExt, BufReader};
    if let Some(err) = child.stderr.take() {
        let a = app.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(err).lines();
            let mut transporte = false;
            while let Ok(Some(l)) = lines.next_line().await {
                // gdi_init_ex: a sessão gráfica abriu (a tela da VM apareceu).
                if l.contains("gdi_init_ex") {
                    set_stage(&a, Which::Rdp, Stage::Up);
                } else if l.contains("ERRCONNECT_") {
                    transporte |= l.contains("ERRCONNECT_CONNECT_TRANSPORT_FAILED");
                    set_stage(&a, Which::Rdp, Stage::Error);
                }
                log(&a, format!("[rdp] {l}"));
            }
            // stderr fechou: o xfreerdp3 terminou. Recolhe (sem zumbi); se não
            // foi o usuário que fechou pelo app, mostra como terminou.
            let child = a.state::<AppState>().rdp_child.lock().unwrap().take();
            if let Some(mut c) = child {
                let ok = c.wait().await.map(|s| s.success()).unwrap_or(false);
                let st = a.state::<AppState>().status.lock().unwrap().rdp;
                if ok && st == Stage::Up {
                    set_stage(&a, Which::Rdp, Stage::Idle);
                    log(&a, "[rdp] sessão encerrada.");
                } else {
                    set_stage(&a, Which::Rdp, Stage::Error);
                    log(&a, "[rdp] o xfreerdp3 desistiu de conectar; veja as linhas acima.");
                    // Caiu no transporte antes da tela chegar: a VM não respondeu.
                    // (O túnel normalmente já avisou; repetir não faz mal.)
                    if transporte && st != Stage::Up {
                        notice(&a, VM_OFF);
                    }
                }
            }
        });
    }

    if let Some(pid) = child.id() {
        tokio::spawn(watch_screen(app.clone(), pid));
        if !p.workspace.is_empty() {
            tokio::spawn(crate::hypr::place_rdp_window(app.clone(), pid, p.workspace.clone()));
        }
    }
    app.state::<AppState>()
        .rdp_child
        .lock()
        .unwrap()
        .replace(child);
    // Fica "conectando" até o xfreerdp3 abrir a tela (ver watch_screen).
    Ok(())
}

/// Quantos bytes o xfreerdp3 precisa ter recebido pelo túnel para contarmos
/// que a tela abriu. O handshake (TLS + negociação) fica bem abaixo disso;
/// a primeira tela, mesmo a de login, passa com folga.
const SCREEN_BYTES: u64 = 150_000;

/// Marca o RDP como conectado quando a tela da VM chega. O gdi_init_ex só sai
/// no log em INFO, e o WLOG_FILTER do FreeRDP não se mostrou confiável; então
/// olhamos a conexão: bytes recebidos no socket deste pid até a porta 3390.
async fn watch_screen(app: AppHandle, pid: u32) {
    let tag = format!("pid={pid},");
    // até 5 min: o bastion às vezes demora ~90s para atender o nosso cliente
    // (atende antes o xfreerdp3 do próprio connector)
    for _ in 0..600 {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        if app.state::<AppState>().status.lock().unwrap().rdp != Stage::Connecting {
            return;
        }
        let Ok(out) = Command::new("ss")
            .args(["-tinpH", "state", "established", "( dport = :3390 )"])
            .output()
            .await
        else {
            return;
        };
        let text = String::from_utf8_lossy(&out.stdout);
        let mut lines = text.lines();
        while let Some(l) = lines.next() {
            if !l.contains(&tag) {
                continue;
            }
            let received = lines
                .next()
                .and_then(|info| {
                    info.split_whitespace()
                        .find_map(|t| t.strip_prefix("bytes_received:"))
                })
                .and_then(|n| n.parse::<u64>().ok())
                .unwrap_or(0);
            if received >= SCREEN_BYTES {
                set_stage(&app, Which::Rdp, Stage::Up);
                log(&app, "[rdp] tela da VM recebida — conectado.");
                return;
            }
        }
    }
}

/// Esconde o valor de /p: nos logs.
fn redact(args: &[String]) -> String {
    args.iter()
        .map(|a| {
            if let Some(rest) = a.strip_prefix("/p:") {
                if rest == "-" {
                    a.clone()
                } else {
                    "/p:***".into()
                }
            } else {
                a.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub async fn close(app: AppHandle) -> Result<()> {
    let child = app.state::<AppState>().rdp_child.lock().unwrap().take();
    if let Some(mut c) = child {
        let _ = c.kill().await;
    }
    set_stage(&app, Which::Rdp, Stage::Idle);
    Ok(())
}
