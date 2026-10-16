//! Configuração não-secreta, em ~/.config/fast-jump-rdp/config.toml.
//! Senhas e seeds TOTP NÃO ficam aqui — vão para o chaveiro (ver secrets.rs).

use crate::error::{msg, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct Config {
    pub configured: bool,
    pub vpn: VpnConfig,
    pub jumpcloud: JumpCloudConfig,
    pub rdp_profiles: Vec<RdpProfile>,
    pub active_profile: usize,
    /// User-Agent usado na webview do portal (disfarce de Chrome).
    pub portal_user_agent: String,
}

fn default_true() -> bool {
    true
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct VpnConfig {
    pub ovpn_path: String,
    pub username: String,
    /// true = confia no servidor: aceita as rotas (e o DNS) que a empresa
    /// empurra; mudou lá, você pega sozinho. É o padrão (igual ao do colega).
    /// false = mantém o controle: ignora o que o servidor manda
    /// (`--route-nopull`), usa a lista `routes` abaixo e bloqueia o DNS deles.
    #[serde(default = "default_true")]
    pub trust_server: bool,
    /// Rotas manuais, usadas só quando trust_server = false.
    pub routes: Vec<String>,
    pub tun_mtu: u32,
    pub mssfix: u32,
}

impl Default for VpnConfig {
    fn default() -> Self {
        VpnConfig {
            ovpn_path: String::new(),
            username: String::new(),
            trust_server: true,
            routes: Vec::new(),
            tun_mtu: 0,
            mssfix: 0,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct JumpCloudConfig {
    pub email: String,
    pub portal_url: String,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct RdpProfile {
    pub name: String,
    pub domain: String,
    pub username: String,
    /// "fullscreen" | "fixed" | "dynamic"
    pub window_mode: String,
    pub width: u32,
    pub height: u32,
    /// "single" | "all" | "list"
    pub monitor_mode: String,
    pub monitor_ids: String, // ex. "0,1"
    /// "fast" | "balanced" | "max"
    pub quality: String,
    pub clipboard: bool,
    pub sound: bool,
    pub microphone: bool,
    pub shared_folder: String,
    pub extra_flags: String,
    /// Workspace do Hyprland onde a janela abre ("3", "special:scratchpad");
    /// vazio = onde o Hyprland colocar.
    pub workspace: String,
}

impl Default for RdpProfile {
    fn default() -> Self {
        // Perfil espelhando o comando do fluxo antigo.
        RdpProfile {
            name: "Padrão".into(),
            domain: String::new(),
            username: String::new(),
            window_mode: "fixed".into(),
            width: 1920,
            height: 1080,
            monitor_mode: "single".into(),
            monitor_ids: "0".into(),
            quality: "balanced".into(),
            clipboard: true,
            sound: false,
            microphone: false,
            shared_folder: String::new(),
            extra_flags: String::new(),
            workspace: String::new(),
        }
    }
}

pub const CHROME_UA: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 \
(KHTML, like Gecko) Chrome/152.0.0.0 Safari/537.36";

pub fn config_dir() -> Result<PathBuf> {
    let d = dirs::config_dir()
        .ok_or_else(|| msg("não achei o diretório de config"))?
        .join("fast-jump-rdp");
    std::fs::create_dir_all(&d)?;
    Ok(d)
}

pub fn data_dir() -> Result<PathBuf> {
    let d = dirs::data_dir()
        .ok_or_else(|| msg("não achei o diretório de dados"))?
        .join("fast-jump-rdp");
    std::fs::create_dir_all(&d)?;
    Ok(d)
}

fn config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.toml"))
}

pub fn load() -> Result<Config> {
    let p = config_path()?;
    if !p.exists() {
        let mut c = Config::default();
        c.defaults();
        return Ok(c);
    }
    let mut c: Config = toml::from_str(&std::fs::read_to_string(p)?)?;
    c.defaults();
    Ok(c)
}

pub fn save(c: &Config) -> Result<()> {
    std::fs::write(config_path()?, toml::to_string_pretty(c)?)?;
    Ok(())
}

impl Config {
    /// Preenche valores padrão sensatos quando o arquivo vem vazio/parcial.
    fn defaults(&mut self) {
        if self.portal_user_agent.is_empty() {
            self.portal_user_agent = CHROME_UA.into();
        }
        if self.vpn.tun_mtu == 0 {
            self.vpn.tun_mtu = 1350;
        }
        if self.vpn.mssfix == 0 {
            self.vpn.mssfix = 1310;
        }
        if self.rdp_profiles.is_empty() {
            self.rdp_profiles = vec![RdpProfile::default()];
        }
    }

    pub fn active(&self) -> &RdpProfile {
        self.rdp_profiles
            .get(self.active_profile)
            .unwrap_or(&self.rdp_profiles[0])
    }
}
