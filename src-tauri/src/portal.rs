//! Portal JumpCloud embutido.
//!
//! Abre uma janela webview com User-Agent de Chrome, injeta:
//!  1. um preenchedor de login (e-mail, senha, TOTP) — best-effort, os
//!     seletores podem precisar de ajuste contra a página real;
//!  2. o interceptador do XHR `ConnectThroughBastion`, que ao achar a URL
//!     `vosession://` navega para ela. A navegação é capturada no Rust
//!     (on_navigation) e dispara o túnel + RDP.

use crate::config::Config;
use crate::error::{msg, Result};
use crate::secrets::{self, Key};
use crate::state::{log, AppState};
use tauri::webview::WebviewBuilder;
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, WebviewUrl};

const WINDOW_LABEL: &str = "portal";

/// Script injetado no início de cada página do portal.
///
/// Fluxo automatizado (SPA Angular, um MutationObserver cobre as transições):
///  1. login local: preenche e-mail e senha (o usuário clica em "Local Account"
///     e "Log In", e passa em eventual checagem de humano);
///  2. "Verify Your Identity": clica em "Authenticator App";
///  3. "Verify Security Code": gera o TOTP FRESCO a partir do seed (via
///     crypto.subtle, dentro da webview) e preenche os 6 quadradinhos, e dá Submit;
///  4. intercepta `ConnectThroughBastion` e captura a URL `vosession://`.
fn init_script(email: &str, password: &str, seed: &str) -> String {
    let email = serde_json::to_string(email).unwrap();
    let password = serde_json::to_string(password).unwrap();
    let seed = serde_json::to_string(seed).unwrap();
    format!(
        r#"
(function() {{
  const EMAIL = {email}, PASSWORD = {password}, SEED = {seed};

  function setVal(el, val) {{
    if (!el) return false;
    const proto = Object.getPrototypeOf(el);
    const desc = Object.getOwnPropertyDescriptor(proto, 'value');
    desc && desc.set ? desc.set.call(el, val) : (el.value = val);
    el.dispatchEvent(new Event('input', {{ bubbles: true }}));
    el.dispatchEvent(new Event('change', {{ bubbles: true }}));
    return true;
  }}

  // --- TOTP no navegador (HMAC-SHA1 via WebCrypto), código fresco na hora ---
  function base32ToBytes(s) {{
    s = String(s).replace(/=+$/,'').replace(/\s+/g,'').toUpperCase();
    const A = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567';
    let bits = 0, value = 0; const out = [];
    for (const c of s) {{ const i = A.indexOf(c); if (i < 0) continue;
      value = (value << 5) | i; bits += 5;
      if (bits >= 8) {{ out.push((value >>> (bits - 8)) & 0xff); bits -= 8; }} }}
    return new Uint8Array(out);
  }}
  async function totp(seedB32) {{
    const key = base32ToBytes(seedB32);
    let counter = Math.floor(Date.now() / 1000 / 30);
    const buf = new ArrayBuffer(8); const dv = new DataView(buf);
    dv.setUint32(0, Math.floor(counter / 0x100000000));
    dv.setUint32(4, counter >>> 0);
    const ck = await crypto.subtle.importKey('raw', key, {{ name: 'HMAC', hash: 'SHA-1' }}, false, ['sign']);
    const sig = new Uint8Array(await crypto.subtle.sign('HMAC', ck, buf));
    const off = sig[19] & 0xf;
    const bin = ((sig[off] & 0x7f) << 24) | (sig[off+1] << 16) | (sig[off+2] << 8) | sig[off+3];
    return String(bin % 1000000).padStart(6, '0');
  }}

  function byText(sel, re) {{
    return [...document.querySelectorAll(sel)].find(e => re.test((e.textContent||'').trim()));
  }}
  // Os 6 inputs são id="code-0".."code-5" (maxlength=6, type=text) no JumpCloud.
  function otpBoxes() {{
    let b = [...document.querySelectorAll('input[id^="code-"]')].filter(i => i.offsetParent !== null);
    if (b.length < 6) {{
      // fallback genérico (um dígito por caixa)
      b = [...document.querySelectorAll('input')].filter(i =>
        i.offsetParent !== null && i.type !== 'password' &&
        (i.maxLength === 1 || i.getAttribute('maxlength') === '1'));
    }}
    return b;
  }}

  let clickedLocal = false, didEmail = false, didPass = false, checkedStay = false;
  let clickedLogin = false, clickedAuth = false, filledOtp = false;

  async function tick() {{
    const boxes = otpBoxes();

    // 3. tela dos 6 quadradinhos (prioridade máxima)
    if (boxes.length >= 6 && !filledOtp && SEED) {{
      filledOtp = true;
      try {{
        const code = await totp(SEED);
        for (let i = 0; i < 6 && i < boxes.length; i++) {{
          const b = boxes[i]; b.focus();
          setVal(b, code[i]);
          b.dispatchEvent(new KeyboardEvent('keydown', {{ bubbles: true, key: code[i] }}));
          b.dispatchEvent(new KeyboardEvent('keyup', {{ bubbles: true, key: code[i] }}));
        }}
        const submit = byText('button', /submit/i);
        if (submit) setTimeout(() => submit.click(), 300);
      }} catch (e) {{ console.error('[fastjump] totp falhou', e); filledOtp = false; }}
      return;
    }}

    // 2. escolher método "Authenticator App"
    if (!clickedAuth && /Verify Your Identity/i.test(document.body.innerText || '')) {{
      const b = byText('button, a, [role=button]', /authenticator app/i);
      if (b) {{ b.click(); clickedAuth = true; return; }}
    }}

    // 1a. revelar o formulário local
    if (!clickedLocal) {{
      const la = byText('a, button, [role=button]', /local account/i);
      if (la) {{ la.click(); clickedLocal = true; return; }}
    }}

    // 1b. preencher e-mail / senha
    if (!didEmail) {{
      const e = document.querySelector('input[type=email], input[name*=email i], input[id*=email i], input[autocomplete=username]');
      if (e && EMAIL) didEmail = setVal(e, EMAIL);
    }}
    if (!didPass) {{
      const p = document.querySelector('input[type=password]');
      if (p && PASSWORD) didPass = setVal(p, PASSWORD);
    }}

    // 1c. marcar "Stay Signed In"
    if (!checkedStay) {{
      const lbl = [...document.querySelectorAll('.form-check-label, label')]
        .find(e => /stay signed in/i.test(e.textContent || ''));
      const cb = (lbl && (lbl.closest('.form-check, label, div') || document)
        .querySelector('input[type=checkbox]')) || null;
      if (cb) {{ if (!cb.checked) cb.click(); checkedStay = true; }}
    }}

    // 1d. clicar em "Log In" quando e-mail e senha já estão preenchidos
    if (didEmail && didPass && !clickedLogin) {{
      const btn = byText('button', /^\s*log in\s*$/i);
      if (btn) {{ clickedLogin = true; setTimeout(() => btn.click(), 400); }}
    }}
  }}

  const obs = new MutationObserver(() => {{ tick(); }});
  obs.observe(document.documentElement, {{ childList: true, subtree: true }});
  document.addEventListener('DOMContentLoaded', tick);
  setInterval(tick, 800);

  // --- 4. interceptador do ConnectThroughBastion ---
  const origOpen = XMLHttpRequest.prototype.open;
  XMLHttpRequest.prototype.open = function(m, u) {{ this._u = u; return origOpen.apply(this, arguments); }};
  const origSend = XMLHttpRequest.prototype.send;
  XMLHttpRequest.prototype.send = function() {{
    this.addEventListener('load', function() {{
      if (this._u && String(this._u).includes('ConnectThroughBastion')) {{
        const status = this.status;
        // Avisa o app (o Rust captura e bloqueia esse esquema) quando o portal
        // respondeu mas não veio a URL — senão o clique "some" sem log.
        const fail = (m) => {{
          window.location.href = 'fastjump-log://portal?m=' + encodeURIComponent(m);
        }};
        const go = (d) => {{
          if (d && d.result && d.result.url && d.result.url.startsWith('vosession://')) {{
            setTimeout(() => {{ window.location.href = d.result.url; }}, 50);
          }} else {{
            const why = (d && (d.error && (d.error.message || d.error.details))) || 'sem result.url';
            fail('Direct Access respondeu HTTP ' + status + ', sem URL de sessão: ' + why);
          }}
        }};
        try {{
          if (this.responseType === 'blob') this.response.text().then(t => go(JSON.parse(t)));
          else if (this.responseType === 'json') go(this.response);
          else go(JSON.parse(this.responseText));
        }} catch (e) {{
          console.error('[fastjump] parse falhou', e);
          fail('Direct Access respondeu HTTP ' + status + ', resposta ilegível');
        }}
      }}
    }});
    return origSend.apply(this, arguments);
  }};
}})();
"#
    )
}

/// Área da janela principal (em px lógicos, como o getBoundingClientRect do
/// front) onde o portal fica embutido.
#[derive(serde::Deserialize, Clone, Copy)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Abre o portal embutido na janela principal, na área `b` (ou só o mostra
/// e reposiciona, se já estiver aberto).
pub async fn open(app: AppHandle, cfg: &Config, b: Bounds) -> Result<()> {
    if app.get_webview(WINDOW_LABEL).is_some() {
        return place(&app, Some(b)).await;
    }
    let main = app
        .get_window("main")
        .ok_or_else(|| msg("janela principal não encontrada"))?;

    let email = cfg.jumpcloud.email.clone();
    let password = secrets::get(Key::JumpPassword)?.unwrap_or_default();
    // O seed vai para a webview para gerar o TOTP FRESCO na hora do MFA
    // (o código muda a cada 30s; não dá para pré-calcular no começo do login).
    let seed = secrets::get(Key::JumpSeed)?.unwrap_or_default();

    let url = cfg
        .jumpcloud
        .portal_url
        .parse()
        .map_err(|_| msg("URL do portal do JumpCloud inválida ou vazia; refaça o passo 2 do assistente"))?;

    log(&app, "[portal] abrindo JumpCloud (webview Chrome)…");

    let app_nav = app.clone();
    let builder = WebviewBuilder::new(WINDOW_LABEL, WebviewUrl::External(url))
        .user_agent(&cfg.portal_user_agent)
        .initialization_script(&init_script(&email, &password, &seed))
        .on_navigation(move |url| {
            // Captura a URL vosession:// e bloqueia a navegação de verdade.
            if url.scheme() == "vosession" {
                store_session(&app_nav, url.to_string());
                return false;
            }
            if url.scheme() == "fastjump-log" {
                let m = url
                    .query_pairs()
                    .find(|(k, _)| k == "m")
                    .map(|(_, v)| v.into_owned())
                    .unwrap_or_default();
                log(&app_nav, format!("[portal] {m}"));
                return false;
            }
            true
        });
    main.add_child(
        builder,
        LogicalPosition::new(b.x, b.y),
        LogicalSize::new(b.width.max(1.0), b.height.max(1.0)),
    )
    .map_err(|e| msg(format!("falha ao abrir a webview: {e}")))?;
    #[cfg(target_os = "linux")]
    {
        gtk_layout(&app, Some(b.y)).await;
        // diagnóstico: como o GTK distribuiu a altura depois do layout
        let a = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
            gtk_probe(&a).await;
        });
    }

    Ok(())
}

/// Loga os filhos da caixa GTK da janela: tipo, visível, altura alocada e
/// pedida, e se expandem. Só leitura; o log sai fora da thread principal.
#[cfg(target_os = "linux")]
async fn gtk_probe(app: &AppHandle) {
    let Some(main) = app.get_webview("main") else {
        return;
    };
    let (tx, rx) = tokio::sync::oneshot::channel::<String>();
    let _ = main.with_webview(move |pw| {
        use gtk::prelude::*;
        let wv = pw.inner();
        let Some(parent) = wv.parent() else {
            let _ = tx.send("página sem pai GTK".into());
            return;
        };
        let mut out = format!("{} alt={}", parent.type_().name(), parent.allocated_height());
        if let Some(p) = parent.downcast_ref::<gtk::Paned>() {
            out.push_str(&format!(" divisor={}", p.position()));
        }
        if let Some(c) = parent.downcast_ref::<gtk::Container>() {
            for w in c.children() {
                out.push_str(&format!(
                    " | {} vis={} alt={}",
                    w.type_().name(),
                    w.is_visible(),
                    w.allocated_height()
                ));
            }
        }
        let _ = tx.send(out);
    });
    if let Ok(Ok(m)) = tokio::time::timeout(std::time::Duration::from_secs(2), rx).await {
        log(app, format!("[portal] gtk: {m}"));
    }
}

/// Reposiciona o portal embutido (Some) ou o esconde (None), quando a área
/// dele no front muda de tamanho ou sai da tela (aba Logs, Perfis). A webview
/// nativa fica por cima do HTML, então precisa sumir junto com a área.
pub async fn place(app: &AppHandle, b: Option<Bounds>) -> Result<()> {
    let Some(w) = app.get_webview(WINDOW_LABEL) else {
        return Ok(());
    };
    let r = match b {
        // No Linux o set_bounds não vale para webview filha (ver gtk_layout).
        #[cfg(target_os = "linux")]
        Some(b) => {
            gtk_layout(app, Some(b.y)).await;
            w.show()
        }
        #[cfg(not(target_os = "linux"))]
        Some(b) => w
            .set_bounds(tauri::Rect {
                position: LogicalPosition::new(b.x, b.y).into(),
                size: LogicalSize::new(b.width.max(1.0), b.height.max(1.0)).into(),
            })
            .and_then(|_| w.show()),
        None => {
            let r = w.hide();
            #[cfg(target_os = "linux")]
            gtk_layout(app, None).await;
            r
        }
    };
    r.map_err(|e| msg(format!("falha ao posicionar o portal: {e}")))
}

/// No Linux o Tauri põe a webview filha numa caixa vertical do GTK, depois da
/// principal, e ignora posição/tamanho pedidos. Numa caixa, quem não expande
/// fica com a altura "natural", que na página é a janela toda (visto: página
/// 1030px, portal 1px), e o size_request é só mínimo. Então, na primeira vez,
/// as duas webviews saem da caixa e vão para um gtk::Paned vertical: página
/// em cima, portal embaixo, divisor no fim dos controles (`top`). O Paned
/// obedece à posição, cresce o portal quando a janela cresce e o divisor dá
/// para arrastar. Portal escondido = a página ocupa tudo (`None` não mexe).
///
/// O callback roda na thread principal e só mexe no GTK: nada de log/emit
/// lá dentro (reentra no despachante do Tauri e trava). O resultado volta
/// por canal e é logado aqui fora.
#[cfg(target_os = "linux")]
async fn gtk_layout(app: &AppHandle, top: Option<f64>) {
    let Some(y) = top else {
        return;
    };
    let Some(main) = app.get_webview("main") else {
        return;
    };
    let (tx, rx) = tokio::sync::oneshot::channel::<Option<String>>();
    let sent = main.with_webview(move |pw| {
        use gtk::prelude::*;
        let wv = pw.inner();
        let y = y.round() as i32;
        let parent = wv.parent();

        // já no Paned: só reposiciona o divisor
        if let Some(paned) = parent.clone().and_then(|p| p.downcast::<gtk::Paned>().ok()) {
            if paned.position() != y {
                paned.set_position(y);
            }
            let _ = tx.send(Some(format!("[portal] layout: divisor em {y}px")));
            return;
        }

        let Some(vbox) = parent.clone().and_then(|p| p.downcast::<gtk::Box>().ok()) else {
            let tipo = parent.map(|p| p.type_().name().to_string());
            let _ = tx.send(Some(format!(
                "[portal] layout: a página não está numa caixa GTK ({tipo:?})"
            )));
            return;
        };
        let main_w: gtk::Widget = wv.clone().upcast();
        let Some(portal) = vbox
            .children()
            .into_iter()
            .find(|c| *c != main_w && c.type_().name() == "WebKitWebView")
        else {
            let _ = tx.send(Some("[portal] layout: webview do portal não achada na caixa".into()));
            return;
        };

        let paned = gtk::Paned::new(gtk::Orientation::Vertical);
        // remove() não destrói: `wv` e `portal` seguram as referências
        vbox.remove(&wv);
        vbox.remove(&portal);
        wv.set_size_request(-1, -1);
        paned.pack1(&wv, false, true);
        paned.pack2(&portal, true, true);
        vbox.pack_start(&paned, true, true, 0);
        paned.set_position(y);
        paned.show();
        let _ = tx.send(Some(format!("[portal] layout: página e portal num divisor em {y}px")));
    });
    if sent.is_err() {
        return;
    }
    if let Ok(Ok(Some(m))) = tokio::time::timeout(std::time::Duration::from_secs(2), rx).await {
        // o mesmo layout é reaplicado a cada resize: só loga quando muda
        static LAST: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());
        let mut last = LAST.lock().unwrap();
        if *last != m {
            *last = m.clone();
            drop(last);
            log(app, m);
        }
    }
}

/// Chamado pelo on_navigation handler no lib.rs para guardar a URL capturada.
pub fn store_session(app: &AppHandle, url: String) {
    let state = app.state::<AppState>();
    if state.pending_session.lock().unwrap().as_deref() == Some(url.as_str()) {
        return; // mesma URL navegada de novo: já tratada
    }
    log(app, format!("[portal] Direct Access detectado — {}", describe_session(&url)));
    app.state::<AppState>()
        .pending_session
        .lock()
        .unwrap()
        .replace(url);
    let _ = tauri::Emitter::emit(app, "session-captured", ());
}

/// Resumo da URL vosession:// para o log, sem o token: host, ids da sessão e
/// do recurso (do payload do JWT) e quanto tempo o token ainda vale.
fn describe_session(raw: &str) -> String {
    use base64::Engine;
    let Ok(u) = tauri::Url::parse(raw) else {
        return "URL vosession:// ilegível".into();
    };
    let host = format!("{}:{}", u.host_str().unwrap_or("?"), u.port().unwrap_or(443));
    let claims = u
        .query_pairs()
        .find(|(k, _)| k == "token")
        .and_then(|(_, t)| {
            let payload = t.split('.').nth(1)?.to_string();
            let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(payload.trim_end_matches('='))
                .ok()?;
            serde_json::from_slice::<serde_json::Value>(&bytes).ok()
        });
    let Some(c) = claims else {
        return format!("{host}, token sem payload legível");
    };
    let get = |k: &str| c.get(k).and_then(|v| v.as_str()).unwrap_or("?").to_string();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let validade = match c.get("exp").and_then(|v| v.as_i64()) {
        Some(exp) if exp > now => format!("token vale mais {}s", exp - now),
        Some(_) => "token JÁ EXPIRADO".into(),
        None => "sem validade no token".into(),
    };
    format!(
        "{host}, sessão {}, recurso {}, {validade}",
        get("session_id"),
        get("resource_id")
    )
}
