//! Qualidade da conexão: latência do connector até o bastion, traduzida em
//! 1..5 barras (verde/amarelo/vermelho), emitidas no evento `quality`.

use serde::Serialize;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::process::Command;

#[derive(Serialize, Clone, Copy)]
pub struct Quality {
    /// 0 = sem conexão; 1..5 = barras preenchidas.
    pub bars: u8,
    /// latência em ms (-1 se indisponível).
    pub latency_ms: i64,
}

fn bars_for(latency_ms: i64) -> u8 {
    match latency_ms {
        l if l < 0 => 0,
        0..=20 => 5,
        21..=50 => 4,
        51..=100 => 3,
        101..=180 => 2,
        _ => 1,
    }
}

/// Mede uma vez a latência até o bastion. Não abre conexão: lê a RTT que o
/// kernel já mede no socket do connector para fora (bastion, via VPN). Medir
/// o connect em 127.0.0.1:3390 dava sempre ~0ms, porque a porta é local.
async fn probe() -> Quality {
    match bastion_rtt_ms().await {
        Some(ms) => Quality {
            bars: bars_for(ms),
            latency_ms: ms,
        },
        None => Quality {
            bars: 0,
            latency_ms: -1,
        },
    }
}

/// `rtt:<média>/<desvio>` do `ss -ti` no socket estabelecido do connector
/// cujo destino não é local. None se o túnel não está de pé.
async fn bastion_rtt_ms() -> Option<i64> {
    let out = Command::new("ss")
        .args(["-tinpH", "state", "established"])
        .output()
        .await
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let mut lines = text.lines();
    while let Some(l) = lines.next() {
        // linha do socket: Recv-Q Send-Q Local Peer users:((...)); a seguinte
        // (indentada) traz os detalhes, entre eles o rtt.
        let peer = l.split_whitespace().nth(3).unwrap_or("");
        if !l.contains("\"VaultOneRDPConn") || peer.starts_with("127.") || peer.starts_with('[') {
            continue;
        }
        let info = lines.next()?;
        let rtt = info
            .split_whitespace()
            .find_map(|t| t.strip_prefix("rtt:"))?
            .split('/')
            .next()?
            .parse::<f64>()
            .ok()?;
        return Some(rtt.round() as i64);
    }
    None
}

/// Loop de monitoramento: emite `quality` a cada 2s enquanto o app roda.
/// Usa o runtime async do Tauri (o `setup` roda fora de um contexto Tokio).
pub fn spawn_monitor(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let q = probe().await;
            let _ = app.emit("quality", q);
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });
}
