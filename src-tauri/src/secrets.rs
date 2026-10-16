//! Segredos num arquivo simples, `~/.config/fast-jump-rdp/secrets.json`,
//! legível só pelo usuário (0600), sem criptografia.
//!
//! Antes ficavam no chaveiro (Secret Service / gnome-keyring), mas nesta
//! máquina ele às vezes gravava vazio por cima (a senha da VPN "sumia").
//! O chaveiro agora só é lido para migrar: se um segredo não está no arquivo
//! mas está no chaveiro (e não vazio), ele é copiado para o arquivo.

use crate::config::config_dir;
use crate::error::Result;
use std::collections::BTreeMap;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::sync::Mutex;

const SERVICE: &str = "fast-jump-rdp";

/// Serializa leitura+escrita do arquivo entre comandos simultâneos.
static LOCK: Mutex<()> = Mutex::new(());

/// Chaves lógicas de cada segredo guardado.
#[derive(Clone, Copy)]
pub enum Key {
    VpnPassword,
    VpnSeed,
    JumpPassword,
    JumpSeed,
    WindowsPassword,
}

impl Key {
    fn id(self) -> &'static str {
        match self {
            Key::VpnPassword => "vpn_password",
            Key::VpnSeed => "vpn_seed",
            Key::JumpPassword => "jump_password",
            Key::JumpSeed => "jump_seed",
            Key::WindowsPassword => "windows_password",
        }
    }
}

type Store = BTreeMap<String, String>;

fn path() -> Result<PathBuf> {
    Ok(config_dir()?.join("secrets.json"))
}

fn load() -> Result<Store> {
    let p = path()?;
    if !p.exists() {
        return Ok(Store::new());
    }
    serde_json::from_str(&std::fs::read_to_string(p)?)
        .map_err(|e| crate::error::msg(format!("secrets.json ilegível: {e}")))
}

/// Grava num temporário 0600 e renomeia por cima: nunca fica meio escrito.
fn save(s: &Store) -> Result<()> {
    let p = path()?;
    let tmp = p.with_extension("json.tmp");
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)?;
    let json = serde_json::to_string_pretty(s).map_err(|e| crate::error::msg(e.to_string()))?;
    f.write_all(json.as_bytes())?;
    f.sync_all()?;
    std::fs::rename(tmp, p)?;
    Ok(())
}

/// Valor antigo no chaveiro, se houver (vazio conta como ausente).
fn from_keyring(k: Key) -> Option<String> {
    keyring::Entry::new(SERVICE, k.id())
        .ok()?
        .get_password()
        .ok()
        .filter(|v| !v.is_empty())
}

pub fn set(k: Key, value: &str) -> Result<()> {
    let _g = LOCK.lock().unwrap();
    let mut s = load()?;
    s.insert(k.id().into(), value.into());
    save(&s)
}

pub fn get(k: Key) -> Result<Option<String>> {
    let _g = LOCK.lock().unwrap();
    let mut s = load()?;
    if let Some(v) = s.get(k.id()) {
        return Ok(Some(v.clone()));
    }
    let Some(v) = from_keyring(k) else {
        return Ok(None);
    };
    s.insert(k.id().into(), v.clone());
    save(&s)?;
    Ok(Some(v))
}

pub fn delete(k: Key) -> Result<()> {
    let _g = LOCK.lock().unwrap();
    let mut s = load()?;
    if s.remove(k.id()).is_some() {
        save(&s)?;
    }
    // senão a migração traria o valor antigo de volta
    if let Ok(e) = keyring::Entry::new(SERVICE, k.id()) {
        let _ = e.delete_credential();
    }
    Ok(())
}

pub fn has(k: Key) -> bool {
    matches!(get(k), Ok(Some(_)))
}
