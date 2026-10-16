//! JumpDesk — backend.
//! Fluxo: VPN → portal JumpCloud (webview) → túnel VaultOne → RDP.

mod config;
mod connector;
mod error;
mod hypr;
mod portal;
mod quality;
mod rdp;
mod secrets;
mod state;
mod totp;
mod tunnel;
mod vpn;

use config::Config;
use error::Result;
use secrets::Key;
use state::{AppState, Status};
use tauri::{AppHandle, Manager};

// ---------- config ----------

#[tauri::command]
fn get_config() -> Result<Config> {
    config::load()
}

#[tauri::command]
fn is_configured() -> bool {
    config::load().map(|c| c.configured).unwrap_or(false)
}

/// Salva config não-secreta + segredos no chaveiro, de uma vez (fim do assistente).
#[derive(serde::Deserialize)]
struct SetupInput {
    config: Config,
    vpn_password: String,
    vpn_seed: String,
    jump_password: String,
    jump_seed: String,
    windows_password: Option<String>,
}

#[tauri::command]
fn save_setup(mut input: SetupInput) -> Result<()> {
    // valida antes de gravar: senha vazia vira AUTH_FAILED lá na frente
    if input.vpn_password.is_empty() || input.jump_password.is_empty() {
        return Err(error::msg("preencha a senha da VPN e a do JumpCloud"));
    }
    totp::validate(&input.vpn_seed)?;
    totp::validate(&input.jump_seed)?;

    secrets::set(Key::VpnPassword, &input.vpn_password)?;
    secrets::set(Key::VpnSeed, &input.vpn_seed)?;
    secrets::set(Key::JumpPassword, &input.jump_password)?;
    secrets::set(Key::JumpSeed, &input.jump_seed)?;
    match input.windows_password.as_deref() {
        Some(pw) if !pw.is_empty() => secrets::set(Key::WindowsPassword, pw)?,
        _ => secrets::delete(Key::WindowsPassword)?,
    }

    input.config.configured = true;
    config::save(&input.config)?;
    Ok(())
}

/// Há senha da VPN guardada (e não vazia)? Para a UI mostrar ••••.
#[tauri::command]
fn has_vpn_password() -> bool {
    matches!(secrets::get(Key::VpnPassword), Ok(Some(p)) if !p.is_empty())
}

/// Troca só a senha da VPN (sem refazer o assistente).
#[tauri::command]
fn set_vpn_password(password: String) -> Result<()> {
    if password.is_empty() {
        return Err(error::msg("a senha da VPN não pode ficar vazia"));
    }
    secrets::set(Key::VpnPassword, &password)
}

#[tauri::command]
fn save_config(config: Config) -> Result<()> {
    config::save(&config)
}

/// Importa seed(s) de texto (base32 / otpauth / migration).
#[tauri::command]
fn import_seed_text(text: String) -> Result<Vec<totp::ImportedAccount>> {
    totp::import_from_text(&text)
}

/// Importa seed(s) de um print do QR de exportação.
#[tauri::command]
fn import_seed_image(path: String) -> Result<Vec<totp::ImportedAccount>> {
    totp::import_from_image(&path)
}

// ---------- status / logs ----------

#[tauri::command]
fn get_status(app: AppHandle) -> Status {
    *app.state::<AppState>().status.lock().unwrap()
}

/// Qual senha do Windows está guardada? (para a UI mostrar o modo).
#[tauri::command]
fn has_windows_password() -> bool {
    secrets::has(Key::WindowsPassword)
}

// ---------- connector ----------

#[tauri::command]
fn connector_installed() -> bool {
    connector::is_installed()
}

#[tauri::command]
async fn connector_download(app: AppHandle) -> Result<()> {
    connector::download(app).await
}

// ---------- ações do fluxo ----------

#[tauri::command]
async fn vpn_connect(app: AppHandle) -> Result<()> {
    let cfg = config::load()?;
    vpn::connect(app, &cfg).await
}

#[tauri::command]
async fn vpn_disconnect(app: AppHandle) -> Result<()> {
    vpn::disconnect(app).await
}

#[tauri::command]
async fn open_portal(app: AppHandle, bounds: portal::Bounds) -> Result<()> {
    let cfg = config::load()?;
    portal::open(app, &cfg, bounds).await
}

/// Reposiciona (bounds) ou esconde (null) o portal embutido.
#[tauri::command]
async fn place_portal(app: AppHandle, bounds: Option<portal::Bounds>) -> Result<()> {
    // async: roda fora da thread principal (o layout espera por ela)
    portal::place(&app, bounds).await
}

/// Usa a URL vosession:// capturada para subir o túnel e abrir o RDP.
#[tauri::command]
async fn connect_session(app: AppHandle) -> Result<()> {
    let url = app
        .state::<AppState>()
        .pending_session
        .lock()
        .unwrap()
        .clone();
    let url = url.ok_or_else(|| error::msg("nenhuma sessão capturada; clique no servidor no portal"))?;

    // Captura repetida (o portal às vezes navega 2x): não sobe outro túnel/RDP.
    let st = *app.state::<AppState>().status.lock().unwrap();
    if st.tunnel == state::Stage::Connecting || st.rdp == state::Stage::Connecting {
        state::log(&app, "[app] já conectando esta sessão — ignorando o disparo repetido.");
        return Ok(());
    }

    tunnel::start(app.clone(), &url).await?;
    let cfg = config::load()?;
    let profile = cfg.active().clone();
    rdp::open(app, &profile).await
}

#[tauri::command]
async fn open_rdp(app: AppHandle) -> Result<()> {
    let cfg = config::load()?;
    let profile = cfg.active().clone();
    rdp::open(app, &profile).await
}

#[tauri::command]
async fn disconnect_all(app: AppHandle) -> Result<()> {
    let _ = rdp::close(app.clone()).await;
    let _ = tunnel::stop(app.clone()).await;
    let _ = vpn::disconnect(app).await;
    Ok(())
}

/// Limpa o que sobrou de tentativas anteriores, inclusive as feitas fora do
/// app (script antigo, conexão pelo navegador): openvpn, connector, xfreerdp
/// na porta do túnel. No fim, confere se a porta e a VPN ficaram livres.
#[tauri::command]
async fn cleanup(app: AppHandle) -> Result<()> {
    use state::log;
    use tokio::process::Command;

    log(&app, "[limpeza] encerrando RDP, túnel e VPN…");
    let _ = rdp::close(app.clone()).await;
    let _ = tunnel::stop(app.clone()).await;
    *app.state::<AppState>().pending_session.lock().unwrap() = None;

    // Processos do próprio usuário, abertos fora do app.
    let user = std::env::var("USER").unwrap_or_default();
    for pat in [
        "vaultone_automator",
        "VaultOneRDPConnector",
        r"xfreerdp3 .*/v:(localhost|127\.0\.0\.1):3390",
    ] {
        let _ = Command::new("pkill").args(["-u", &user, "-f", pat]).status().await;
    }

    let vpn_res = vpn::disconnect(app.clone()).await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let mut sobras = Vec::new();
    if tunnel::is_up().await {
        sobras.push(format!("porta {} ainda ocupada", tunnel::PORT));
    }
    let tuns: Vec<String> = std::fs::read_dir("/sys/class/net")
        .map(|d| {
            d.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.starts_with("tun"))
                .collect()
        })
        .unwrap_or_default();
    if !tuns.is_empty() {
        sobras.push(format!("interface {} ainda existe", tuns.join(", ")));
    }

    vpn_res?;
    if sobras.is_empty() {
        log(&app, "[limpeza] tudo limpo.");
        Ok(())
    } else {
        Err(error::msg(format!("limpeza incompleta: {}", sobras.join("; "))))
    }
}

/// Workspaces do Hyprland para o seletor do perfil (vazio fora do Hyprland).
#[tauri::command]
async fn hypr_workspaces() -> Vec<String> {
    if hypr::available() {
        hypr::workspaces().await
    } else {
        Vec::new()
    }
}

/// Preview do comando xfreerdp para o perfil ativo (sem senha), para a UI de config.
#[tauri::command]
fn rdp_preview() -> Result<String> {
    let cfg = config::load()?;
    let args = rdp::build_args(cfg.active(), None);
    Ok(format!("xfreerdp3 {}", args.join(" ")))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .setup(|app| {
            quality::spawn_monitor(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            is_configured,
            save_setup,
            save_config,
            set_vpn_password,
            has_vpn_password,
            import_seed_text,
            import_seed_image,
            get_status,
            has_windows_password,
            connector_installed,
            connector_download,
            vpn_connect,
            vpn_disconnect,
            open_portal,
            place_portal,
            connect_session,
            open_rdp,
            disconnect_all,
            cleanup,
            rdp_preview,
            hypr_workspaces,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
