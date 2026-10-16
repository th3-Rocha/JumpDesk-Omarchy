//! Geração de códigos TOTP e importação de seeds do Google Authenticator.
//!
//! Aceita três formas de seed:
//!  - base32 cru (ex.: "JBSWY3DPEHPK3PXP")
//!  - URI otpauth://totp/...?secret=BASE32
//!  - URI otpauth-migration://offline?data=... (exportação "Transferir contas")
//!    e também um print do QR dessa exportação.

use crate::error::{msg, Result};
use data_encoding::BASE32_NOPAD;
use totp_rs::{Algorithm, TOTP};

/// Uma conta extraída de uma exportação do Google Authenticator.
#[derive(serde::Serialize, Clone)]
pub struct ImportedAccount {
    pub name: String,
    pub issuer: String,
    pub secret_base32: String,
}

/// Gera o código de 6 dígitos atual para um seed base32.
pub fn generate(secret_base32: &str) -> Result<String> {
    let secret = normalize_base32(secret_base32)?;
    // new_unchecked: não impõe o mínimo de 128 bits do RFC. Seeds de 80 bits
    // (16 chars base32) são comuns e válidos — o oathtool do fluxo antigo aceita.
    let totp = TOTP::new_unchecked(Algorithm::SHA1, 6, 1, 30, secret, None, "fastjump".to_string());
    totp.generate_current()
        .map_err(|e| crate::error::AppError::Totp(e.to_string()))
}

/// Valida que um seed base32 é utilizável (gera um código sem erro).
pub fn validate(secret_base32: &str) -> Result<()> {
    generate(secret_base32).map(|_| ())
}

fn normalize_base32(s: &str) -> Result<Vec<u8>> {
    let cleaned: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    let cleaned = cleaned.trim_end_matches('=').to_uppercase();
    BASE32_NOPAD
        .decode(cleaned.as_bytes())
        .map_err(|e| crate::error::AppError::Totp(format!("base32 inválido: {e}")))
}

/// Extrai o(s) seed(s) de qualquer forma aceita.
pub fn import_from_text(text: &str) -> Result<Vec<ImportedAccount>> {
    let t = text.trim();
    if t.starts_with("otpauth-migration://") {
        parse_migration_uri(t)
    } else if t.starts_with("otpauth://") {
        Ok(vec![parse_otpauth_uri(t)?])
    } else {
        // base32 cru
        validate(t)?;
        Ok(vec![ImportedAccount {
            name: "conta".into(),
            issuer: String::new(),
            secret_base32: t.chars().filter(|c| !c.is_whitespace()).collect(),
        }])
    }
}

/// Lê um QR a partir de um arquivo de imagem (print da exportação) e importa.
pub fn import_from_image(path: &str) -> Result<Vec<ImportedAccount>> {
    let img = image::open(path)
        .map_err(|e| msg(format!("não consegui abrir a imagem: {e}")))?
        .to_luma8();
    let mut prep = rqrr::PreparedImage::prepare(img);
    let grids = prep.detect_grids();
    if grids.is_empty() {
        return Err(msg("nenhum QR code encontrado na imagem"));
    }
    let mut out = Vec::new();
    for g in grids {
        let (_meta, content) = g
            .decode()
            .map_err(|e| msg(format!("falha ao ler o QR: {e}")))?;
        out.extend(import_from_text(&content)?);
    }
    if out.is_empty() {
        return Err(msg("QR lido, mas sem contas de autenticação"));
    }
    Ok(out)
}

fn parse_otpauth_uri(uri: &str) -> Result<ImportedAccount> {
    let q = uri.split_once('?').map(|(_, q)| q).unwrap_or("");
    let mut secret = String::new();
    let mut issuer = String::new();
    for pair in q.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            let v = urlencoding::decode(v).unwrap_or_default().into_owned();
            match k {
                "secret" => secret = v,
                "issuer" => issuer = v,
                _ => {}
            }
        }
    }
    if secret.is_empty() {
        return Err(msg("otpauth sem 'secret'"));
    }
    validate(&secret)?;
    let label = uri
        .trim_start_matches("otpauth://totp/")
        .split('?')
        .next()
        .unwrap_or("")
        .to_string();
    Ok(ImportedAccount {
        name: urlencoding::decode(&label).unwrap_or_default().into_owned(),
        issuer,
        secret_base32: secret,
    })
}

/// Decodifica otpauth-migration://offline?data=<base64 do protobuf do Google>.
fn parse_migration_uri(uri: &str) -> Result<Vec<ImportedAccount>> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let q = uri.split_once('?').map(|(_, q)| q).unwrap_or("");
    let data_b64 = q
        .split('&')
        .find_map(|p| p.strip_prefix("data="))
        .ok_or_else(|| msg("migration sem 'data'"))?;
    let data_b64 = urlencoding::decode(data_b64)
        .map_err(|e| msg(format!("url decode: {e}")))?
        .into_owned();
    let bytes = STANDARD
        .decode(data_b64.as_bytes())
        .map_err(|e| msg(format!("base64 inválido: {e}")))?;
    decode_migration_payload(&bytes)
}

// --- Decodificador mínimo do protobuf MigrationPayload do Google Authenticator ---
// Esquema:
//   message MigrationPayload {
//     repeated OtpParameters otp_parameters = 1;
//     ... (outros campos ignorados)
//   }
//   message OtpParameters {
//     bytes  secret   = 1;  // seed binário
//     string name     = 2;
//     string issuer   = 3;
//     ... (algorithm, digits, type, counter — assumimos TOTP/SHA1/6)
//   }

fn decode_migration_payload(buf: &[u8]) -> Result<Vec<ImportedAccount>> {
    let mut accounts = Vec::new();
    let mut i = 0usize;
    while i < buf.len() {
        let (field, wire, n) = read_tag(buf, i)?;
        i = n;
        if field == 1 && wire == 2 {
            let (len, n) = read_varint(buf, i)?;
            i = n;
            let end = i + len as usize;
            if end > buf.len() {
                return Err(msg("migration truncado"));
            }
            accounts.push(decode_otp_parameters(&buf[i..end])?);
            i = end;
        } else {
            i = skip_field(buf, i, wire)?;
        }
    }
    if accounts.is_empty() {
        return Err(msg("exportação sem contas"));
    }
    Ok(accounts)
}

fn decode_otp_parameters(buf: &[u8]) -> Result<ImportedAccount> {
    let mut secret = Vec::new();
    let mut name = String::new();
    let mut issuer = String::new();
    let mut i = 0usize;
    while i < buf.len() {
        let (field, wire, n) = read_tag(buf, i)?;
        i = n;
        match (field, wire) {
            (1, 2) => {
                let (len, n) = read_varint(buf, i)?;
                i = n;
                secret = buf[i..i + len as usize].to_vec();
                i += len as usize;
            }
            (2, 2) => {
                let (len, n) = read_varint(buf, i)?;
                i = n;
                name = String::from_utf8_lossy(&buf[i..i + len as usize]).into_owned();
                i += len as usize;
            }
            (3, 2) => {
                let (len, n) = read_varint(buf, i)?;
                i = n;
                issuer = String::from_utf8_lossy(&buf[i..i + len as usize]).into_owned();
                i += len as usize;
            }
            _ => i = skip_field(buf, i, wire)?,
        }
    }
    if secret.is_empty() {
        return Err(msg("conta sem secret"));
    }
    Ok(ImportedAccount {
        name,
        issuer,
        secret_base32: BASE32_NOPAD.encode(&secret),
    })
}

fn read_tag(buf: &[u8], i: usize) -> Result<(u64, u8, usize)> {
    let (key, n) = read_varint(buf, i)?;
    Ok((key >> 3, (key & 0x7) as u8, n))
}

fn read_varint(buf: &[u8], mut i: usize) -> Result<(u64, usize)> {
    let mut result = 0u64;
    let mut shift = 0;
    loop {
        let b = *buf.get(i).ok_or_else(|| msg("varint fora do buffer"))?;
        result |= ((b & 0x7f) as u64) << shift;
        i += 1;
        if b & 0x80 == 0 {
            break;
        }
        shift += 7;
        if shift >= 64 {
            return Err(msg("varint grande demais"));
        }
    }
    Ok((result, i))
}

fn skip_field(buf: &[u8], i: usize, wire: u8) -> Result<usize> {
    match wire {
        0 => Ok(read_varint(buf, i)?.1),
        1 => Ok(i + 8),
        5 => Ok(i + 4),
        2 => {
            let (len, n) = read_varint(buf, i)?;
            Ok(n + len as usize)
        }
        _ => Err(msg(format!("wire type {wire} não suportado"))),
    }
}
