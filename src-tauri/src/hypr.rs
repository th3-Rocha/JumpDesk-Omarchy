//! Hyprland: manda a janela do xfreerdp3 para o workspace escolhido no perfil
//! (um número, ou um especial como o scratchpad do Omarchy, `special:scratchpad`).

use crate::state::log;
use std::time::Duration;
use tauri::AppHandle;
use tokio::process::Command;

/// Estamos dentro de uma sessão Hyprland?
pub fn available() -> bool {
    std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some()
}

/// Só nomes simples: o valor vai parar dentro de um comando do hyprctl.
fn valid(ws: &str) -> bool {
    !ws.is_empty()
        && ws.len() <= 64
        && ws.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '_' | '-'))
}

/// Workspaces para o seletor: 1–10, o scratchpad e os que existem agora.
pub async fn workspaces() -> Vec<String> {
    let mut list: Vec<String> = (1..=10).map(|n| n.to_string()).collect();
    list.push("special:scratchpad".into());
    if let Ok(out) = Command::new("hyprctl").args(["workspaces", "-j"]).output().await {
        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&out.stdout) {
            for w in v.as_array().into_iter().flatten() {
                if let Some(name) = w["name"].as_str() {
                    if valid(name) && !list.iter().any(|l| l == name) {
                        list.push(name.to_string());
                    }
                }
            }
        }
    }
    list
}

/// A janela deste pid já existe?
async fn has_window(pid: u32) -> bool {
    let Ok(out) = Command::new("hyprctl").args(["clients", "-j"]).output().await else {
        return false;
    };
    serde_json::from_slice::<serde_json::Value>(&out.stdout)
        .ok()
        .and_then(|v| v.as_array().cloned())
        .is_some_and(|a| a.iter().any(|c| c["pid"].as_u64() == Some(pid as u64)))
}

/// Move a janela do pid e vai junto. O Hyprland com config em Lua (Omarchy)
/// só aceita o dispatch em Lua; o antigo, a sintaxe clássica.
async fn move_window(pid: u32, ws: &str) -> Result<(), String> {
    let lua = format!(
        "hl.dsp.window.move({{ workspace = \"{ws}\", follow = true, window = \"pid:{pid}\" }})"
    );
    let out = Command::new("hyprctl")
        .args(["dispatch", &lua])
        .output()
        .await
        .map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if text == "ok" {
        return Ok(());
    }
    let out = Command::new("hyprctl")
        .args(["dispatch", "movetoworkspace", &format!("{ws},pid:{pid}")])
        .output()
        .await
        .map_err(|e| e.to_string())?;
    let classic = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if classic == "ok" {
        Ok(())
    } else {
        Err(format!("{text} / {classic}"))
    }
}

/// Espera a janela do xfreerdp3 aparecer (ela só abre depois de conectar)
/// e a coloca no workspace do perfil.
pub async fn place_rdp_window(app: AppHandle, pid: u32, ws: String) {
    if !available() {
        return;
    }
    if !valid(&ws) {
        log(&app, format!("[rdp] workspace \"{ws}\" inválido; deixei a janela onde abriu."));
        return;
    }
    // ~90s: dá tempo do handshake e da tela de login do Windows
    for _ in 0..300 {
        tokio::time::sleep(Duration::from_millis(300)).await;
        if !std::path::Path::new(&format!("/proc/{pid}")).exists() {
            return; // o xfreerdp3 já saiu
        }
        if has_window(pid).await {
            match move_window(pid, &ws).await {
                Ok(()) => log(&app, format!("[rdp] janela movida para o workspace {ws}.")),
                Err(e) => log(&app, format!("[rdp] não consegui mover a janela para {ws}: {e}")),
            }
            return;
        }
    }
}
