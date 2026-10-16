# Implementação — handoff

Doc de continuidade: se a sessão cair, leia `SCOPE.md` (o quê) e este arquivo
(como e onde paramos). Marque cada passo ao concluir.

## Estado atual

- [x] Repo privado `jumpdesk` criado.
- [x] `README.md` — fluxo antigo documentado + visão do app.
- [x] `SCOPE.md` — escopo fechado com o usuário.
- [x] Fase 0 — esqueleto Tauri + SvelteKit.
- [x] Fase 1 — connector: download + verificação SHA-256 (`src-tauri/src/connector.rs`).
- [x] Fase 2 — config + chaveiro + import de seed (base32/otpauth/migration/QR)
      (`config.rs`, `secrets.rs`, `totp.rs`).
- [x] Fase 3 — VPN: TOTP + openvpn via helper pkexec (`vpn.rs`, `packaging/`).
- [x] Fase 4 — portal embutido: webview Chrome UA + autofill + interceptor
      ConnectThroughBastion (`portal.rs`). **Autofill precisa de ajuste fino
      contra a tela real do JumpCloud (seletores).**
- [x] Fase 5 — túnel + RDP + perfis (`tunnel.rs`, `rdp.rs`).
- [x] Fase 6 — barras de qualidade por latência (`quality.rs`).
- [x] Fase 7 — PKGBUILD + helper + polkit + .desktop (`packaging/`).
- [x] Compila: `cargo build` OK; `pnpm check` 0 erros; `pnpm build` OK.

### Falta testar com dados reais (precisa do usuário)
- [ ] Rodar `pnpm tauri dev` e validar a janela no Hyprland.
- [ ] Assistente: ler QR real do Google Authenticator (os 2 seeds).
- [ ] Instalar helper/polkit (`sudo packaging/install-system.sh`) e testar a VPN.
- [ ] Ajustar seletores do autofill do login do JumpCloud (Fase 4).
- [ ] Teste ponta-a-ponta: conectar tudo → clicar no servidor → RDP sobe.
- [ ] `release` build com LTO ainda não rodado (lento); validar no empacotamento.

Backup do fluxo antigo completo: repo `Fast-FreeRdpOmarchy`, pasta `old-way/`
(scripts, binário do connector, `.ovpn` e `secrets.env` reais).

## Ambiente da máquina do usuário (confirmado)

- Rust 1.98, Node 26, pnpm 11, bun 1.3 disponíveis.
- Libs já instaladas: `webkit2gtk-4.1 2.52`, `libsoup3`, `gtk3`,
  `libappindicator`, `librsvg`, `base-devel`, `gnome-keyring`, `libsecret`.
- GPU AMD Renoir (sem o bug de tela branca do WebKit que afeta NVIDIA).
- Monitores: `HDMI-A-1` (id 0) e `DP-1` (id 1), ambos 1920×1080, scale 1.
- `openvpn` 2.7, `freerdp` 3.31 (`xfreerdp3`), `oath-toolkit`.

## Dados concretos do fluxo (do sistema real)

- Portal: `https://<tenant>.vault.jumpcloud.com/app/main/resources/servers`.
- XHR a interceptar: contém `ConnectThroughBastion`; resposta JSON com
  `result.url` começando em `vosession://`. Pode vir como Blob/JSON/texto.
- URL: `vosession://<tenant>.app.vault.jumpcloud.com:443?code=<uuid>&token=<JWT>&version=2&`.
  JWT com `exp - iat = 300s` (5 min, uso único por sessão).
- Connector oficial: `https://go.vaultone.com/rdp-connector-linux`
  → `https://cdn.vaultone.com/bastion/VaultOneRDPConnectorLinux`.
  SHA-256 `0273a6a2f2adb3ee70c0f7135ec13ff957a1dd455251653f77d22187ea0f4bbb`,
  65.842.003 bytes. É app .NET self-contained; precisa de `icu` + `openssl`;
  rodar com `DOTNET_SYSTEM_NET_DISABLEIPV6=true`; abre túnel em
  `127.0.0.1:3390`.
- Comando RDP de referência do fluxo antigo:
  ```
  xfreerdp3 /v:localhost:3390 /d:"<domínio>" \
    /u:"<usuário>" /p:"-" /client-hostname:"VaultOne" /sec:tls \
    /size:1920x1080 /cert:ignore /network:wan /gfx:AVC444 /bpp:32 \
    +fonts -wallpaper -window-drag -menu-anims -themes
  ```
- VPN (referência): `openvpn --config <ovpn> --auth-user-pass <file>
  --pull-filter ignore "dhcp-option DNS" --route-nopull
  --route <rede-interna-1> 255.255.255.0 vpn_gateway
  --route <host-interno> 255.255.255.255 vpn_gateway
  --tun-mtu 1350 --mssfix 1310`. Credenciais: linha 1 usuário, linha 2
  `senha+TOTP`. TOTP base32 via `oathtool -b --totp`.

## Plano de implementação (ordem)

### Fase 0 — Esqueleto
- [ ] `pnpm create tauri-app` (Svelte + TS). Nome `jumpdesk`.
- [ ] Janela principal com 3 status mockados (VPN/túnel/RDP) + aba de logs.
- [ ] Rodar com `pnpm tauri dev` e confirmar que abre no Hyprland.

### Fase 1 — Connector (testável isolado)
- [ ] Comando Rust `connector_status()` → existe? hash bate?
- [ ] Comando `connector_download()` com progresso via evento Tauri.
- [ ] Verificação SHA-256 (`sha2`), `chmod +x`, salvar em
  `~/.local/share/fast-jump-rdp/`.
- [ ] UI: card do connector (não instalado → botão baixar → barra).

### Fase 2 — Segredos + config
- [ ] Modelo de config (perfis RDP, caminho do ovpn, domínio/usuário).
      Config não-secreta em `~/.config/fast-jump-rdp/config.toml`.
- [ ] Segredos no chaveiro (`keyring`): senhas + os 2 seeds TOTP.
- [ ] Parser do QR `otpauth-migration://` (protobuf do Google Authenticator)
      e também colar base32 direto.

### Fase 3 — VPN
- [ ] Helper root `/usr/local/libexec/fast-jump-rdp-vpn` (lê credenciais por
      stdin, chama openvpn com as flags). Regra polkit para `pkexec` sem senha.
- [ ] Gerar TOTP (`totp-rs`), montar `senha+código`, subir via helper.
- [ ] Monitorar processo/estado; parar; status na UI.
- [ ] Testar conexão real (cuidado: não rodar junto com `re-connect-vpn`).

### Fase 4 — Portal embutido
- [ ] Abrir webview com User-Agent de Chrome configurável.
- [ ] Script de preenchimento de login — **precisa inspecionar os campos reais
      da tela do JumpCloud com o usuário** (seletores de e-mail/senha/TOTP).
- [ ] Script injetado que intercepta `ConnectThroughBastion` e manda
      `result.url` ao backend por IPC. (Base: o userscript em
      `Fast-FreeRdpOmarchy/old-way/tampermonkey/`.)

### Fase 5 — Túnel + RDP + perfis
- [ ] Rodar connector com a URL, capturar logs, esperar porta 3390.
- [ ] Construtor do comando `xfreerdp3` a partir do perfil ativo (F6).
- [ ] Editor de perfis na UI; salvar vários.

### Fase 6 — Qualidade + polish
- [ ] Ping contínuo → 5 barrinhas (verde/amarelo/vermelho) no painel.
- [ ] Reconexão quando VPN/túnel/RDP cai.

### Fase 7 — Empacotamento
- [ ] `PKGBUILD` para Arch (depende de openvpn, freerdp, oath/icu/openssl,
      webkit2gtk, etc.). Instala o app + helper + polkit.

## Pendências que precisam do usuário
- Inspecionar os seletores da tela de login do JumpCloud (Fase 4).
- QR de exportação do Google Authenticator para testar o parser (Fase 2).
- Teste real da VPN e do RDP (o usuário roda).

## Notas / decisões
- GUI: começar pelo mais simples (status no painel; sem overlay sobre o RDP).
- Nunca passar senha/TOTP por argv — sempre stdin ou chaveiro.
- Fluxo antigo é sagrado: não tocar.
