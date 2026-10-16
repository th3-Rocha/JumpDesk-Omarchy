//! Gestão do VaultOneRDPConnector: detecção, download verificado por SHA-256.

use crate::config::data_dir;
use crate::error::{msg, Result};
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter};

pub const URL: &str = "https://go.vaultone.com/rdp-connector-linux";
pub const EXPECTED_SHA256: &str =
    "0273a6a2f2adb3ee70c0f7135ec13ff957a1dd455251653f77d22187ea0f4bbb";
pub const EXPECTED_SIZE: u64 = 65_842_003;

/// Onde o app guarda a sua própria cópia (destino do download).
pub fn path() -> Result<PathBuf> {
    Ok(data_dir()?.join("VaultOneRDPConnector"))
}

/// Locais onde o connector pode já existir, em ordem de preferência.
/// Inclui a instalação antiga em /usr/local/bin, que o usuário já usava.
fn candidate_paths() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Ok(p) = path() {
        v.push(p);
    }
    v.push(PathBuf::from("/usr/local/bin/VaultOneRDPConnector"));
    v
}

fn sha256_file(p: &PathBuf) -> Result<String> {
    let mut f = std::fs::File::open(p)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut f, &mut hasher)?;
    Ok(hex::encode(hasher.finalize()))
}

/// Retorna o primeiro caminho existente cujo hash confere, se houver.
pub fn installed_path() -> Option<PathBuf> {
    for p in candidate_paths() {
        if p.exists() && matches!(sha256_file(&p), Ok(h) if h == EXPECTED_SHA256) {
            return Some(p);
        }
    }
    None
}

/// Instalado = existe em algum caminho conhecido e o hash bate.
pub fn is_installed() -> bool {
    installed_path().is_some()
}

/// Baixa o connector, emitindo progresso 0..100 no evento `connector-progress`.
/// Verifica o SHA-256 antes de instalar; só então marca executável.
pub async fn download(app: AppHandle) -> Result<()> {
    let dest = path()?;
    let tmp = dest.with_extension("part");

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()?;
    let resp = client.get(URL).send().await?.error_for_status()?;
    let total = resp.content_length().unwrap_or(EXPECTED_SIZE);

    let mut file = std::fs::File::create(&tmp)?;
    let mut hasher = Sha256::new();
    let mut downloaded: u64 = 0;
    let mut last_pct: u8 = 255;
    let mut stream = resp.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk)?;
        hasher.update(&chunk);
        downloaded += chunk.len() as u64;
        let pct = ((downloaded.min(total) as f64 / total as f64) * 100.0) as u8;
        if pct != last_pct {
            last_pct = pct;
            let _ = app.emit("connector-progress", pct);
        }
    }
    file.flush()?;
    drop(file);

    let got = hex::encode(hasher.finalize());
    if got != EXPECTED_SHA256 {
        let _ = std::fs::remove_file(&tmp);
        return Err(msg(format!(
            "SHA-256 não confere.\nesperado: {EXPECTED_SHA256}\nrecebido: {got}\n\
             A VaultOne pode ter publicado uma versão nova; confira antes de usar."
        )));
    }

    std::fs::rename(&tmp, &dest)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = std::fs::metadata(&dest)?.permissions();
        perm.set_mode(0o755);
        std::fs::set_permissions(&dest, perm)?;
    }
    let _ = app.emit("connector-progress", 100u8);
    Ok(())
}
