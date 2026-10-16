# Escopo — jumpdesk

## O que o app é

App de desktop (Rust + Tauri v2) para Arch Linux + Hyprland que executa, numa
janela só, todo o fluxo de acesso à VM da empresa:

```
VPN  →  portal JumpCloud  →  túnel VaultOne  →  RDP
```

Substitui o fluxo antigo (`re-connect-vpn` + Tampermonkey + handler
`vosession://` + terminais soltos), que continua documentado em `README.md`.

Distribuído como pacote Arch (PKGBUILD) para reinstalar fácil em PC novo.

## Princípios

- **Não mexe no fluxo antigo.** As peças em `/usr/local/bin`, `~/.local/bin` e
  `~/.local/share/applications` ficam intactas. O app usa caminhos próprios.
- **Usa o connector oficial** (`VaultOneRDPConnector`) e reporta o que ele
  reportar. O app não faz nada em cima do status de sessão da plataforma.
- **Segredos nunca em texto puro** — vão para o chaveiro do sistema (Secret
  Service / gnome-keyring via crate `keyring`).
- **VPN sem pedir senha toda vez** — helper root + regra polkit, instalados uma
  vez no assistente.

## Features

### F1 — Assistente de primeira configuração
- Selecionar o arquivo `.ovpn`.
- Credenciais da VPN: usuário + senha.
- Credenciais do JumpCloud: e-mail + senha.
- Dois seeds de MFA (VPN e JumpCloud): colar base32 **ou** ler print do QR
  "Exportar contas" do Google Authenticator (formato `otpauth-migration://`).
- Domínio + usuário do Windows da VM.
- Instalar helper da VPN (pede senha de admin uma vez).
- Salvar tudo no chaveiro.

### F2 — Gestão do connector
- Detectar `VaultOneRDPConnector` em `~/.local/share/fast-jump-rdp/`.
- Se faltar, baixar de `https://go.vaultone.com/rdp-connector-linux`
  (→ `https://cdn.vaultone.com/bastion/VaultOneRDPConnectorLinux`) com barra de
  progresso.
- Conferir SHA-256 `0273a6a2f2adb3ee70c0f7135ec13ff957a1dd455251653f77d22187ea0f4bbb`
  (65.842.003 bytes). Se não bater, avisar e pedir confirmação.
- `chmod +x`.

### F3 — VPN
- Gerar TOTP (crate `totp-rs`), montar `senha + código`.
- Subir `openvpn` 2 via helper root, mesmas rotas/MTU do fluxo antigo
  (configuráveis). Credenciais por stdin, nunca na linha de comando.
- Status (desconectada / conectando / conectada / erro) + conectar/desconectar.

### F4 — Portal JumpCloud embutido
- Abrir `https://<tenant>.vault.jumpcloud.com/app/main/resources/servers` numa
  webview do Tauri, com User-Agent de Chrome (configurável).
- Preencher login automaticamente: e-mail → senha → TOTP.
- Script injetado intercepta o XHR `ConnectThroughBastion`, extrai
  `result.url` (`vosession://...`) e envia ao backend por IPC do Tauri.

### F5 — Túnel + RDP
- Rodar `VaultOneRDPConnector <url>` com `DOTNET_SYSTEM_NET_DISABLEIPV6=true`,
  logs capturados no app.
- Esperar `127.0.0.1:3390`.
- Rodar `xfreerdp3` com o perfil ativo.

### F6 — Perfis do FreeRDP
- Monitores: 1 (escolher qual) / todos (`/multimon`) / alguns (`/monitors:`).
- Janela: tela cheia / tamanho fixo / dinâmico (`/dynamic-resolution`).
- Qualidade: Rápido / Equilibrado / Máxima → `/network`, `/gfx`, `/bpp`,
  efeitos (`-wallpaper`, `-themes`, `-menu-anims`, `-window-drag`).
- Extras: clipboard, som, microfone, pasta compartilhada, escala.
- Campo livre para flags.
- Vários perfis salvos.

### F7 — Painel de status, logs e qualidade
- Três indicadores: VPN, túnel, RDP.
- Logs de cada etapa na janela.
- Reconexão quando algo cai.
- **Qualidade da conexão: 5 barrinhas por latência** (ping contínuo à VM/túnel;
  verde = baixa latência, vermelho = alta). Exibido **no painel do app**
  (opção mais simples). Overlay flutuante sobre o RDP fica como ideia futura,
  fora do escopo inicial.

## Fora de escopo (por ora)
- Overlay de qualidade por cima da janela do RDP.
- Ler métricas internas do FreeRDP (só ping no começo).
- Qualquer coisa que mire o status "in use / not in use" da plataforma.

## Decisões técnicas
- **Backend:** Rust, Tauri v2, `tokio`, `totp-rs`, `keyring`, `reqwest`, `sha2`.
- **Frontend:** Svelte 5 + TypeScript + Vite.
- **Distribuição:** PKGBUILD para Arch.
- **Ambiente alvo:** Arch, Hyprland, GPU AMD, 2× 1920×1080.
