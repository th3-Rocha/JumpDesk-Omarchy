# Fluxo antigo (antes do JumpDesk)

Referência histórica: as peças soltas que o app substituiu.


```
 [1] VPN                         [2] Portal JumpCloud            [3] Handler vosession://
 ─────────                       ─────────────────────           ─────────────────────────
 re-connect-vpn                  Chrome, logado no JumpCloud     vaultone_automator.sh
  ├ oathtool → código TOTP        └ clique em "conectar"           ├ ghostty: VaultOneRDPConnector <url>
  ├ senha = fixa + TOTP              └ XHR ConnectThroughBastion   │    └ túnel 127.0.0.1:3390 → bastion → VM
  └ sudo openvpn (v2)                   └ Tampermonkey pega        ├ espera a porta 3390 abrir
      só rotas da empresa                  result.url (vosession://) └ xfreerdp3 /v:localhost:3390 ...
                                           e abre essa URL ─────────►
```

## 1. VPN: `re-connect-vpn`

Instalado em `/usr/local/bin/re-connect-vpn`. Roda num terminal que fica aberto (no scratchpad do Hyprland).

- O MFA é um TOTP (o mesmo do Google Authenticator). O script tem o **seed** (`MFA_SECRET`, base32) e gera o código de 6 dígitos com `oathtool`, então não precisa pegar o celular.
- A senha enviada ao servidor é `senha fixa + código TOTP`, concatenados.
- Usa **OpenVPN 2** (pacote `openvpn` do Arch). O OpenVPN 3 dava erro.
- `--route-nopull` + rotas manuais: só o tráfego da empresa passa pela VPN. O DNS da VPN é ignorado.
- MTU reduzido (`--tun-mtu 1350 --mssfix 1310`) para evitar problemas de fragmentação.

```bash
#!/bin/bash

VPN_PASSWORD="<senha fixa>"
VPN_USERNAME="<usuário>"
VPN_PROFILE="~/Downloads/<perfil>.ovpn"
MFA_SECRET="<seed base32 do TOTP>"

AUTH_FILE=$(mktemp)
trap 'rm -f "$AUTH_FILE"; echo -e "\nCredenciais temporárias removidas."' EXIT

MFA_CODE=$(oathtool -b --totp "$MFA_SECRET")
FULL_PASSWORD="${VPN_PASSWORD}${MFA_CODE}"

echo "$VPN_USERNAME" > "$AUTH_FILE"
echo "$FULL_PASSWORD" >> "$AUTH_FILE"

echo "Iniciando conexão com a VPN..."

sudo openvpn --config "$VPN_PROFILE" --auth-user-pass "$AUTH_FILE" \
  --pull-filter ignore "dhcp-option DNS" \
  --route-nopull \
  --route <rede-interna-1> 255.255.255.0 vpn_gateway \
  --route <host-interno> 255.255.255.255 vpn_gateway \
  --tun-mtu 1350 \
  --mssfix 1310

unset FULL_PASSWORD
unset VPN_PASSWORD
```

## 2. Portal JumpCloud + Tampermonkey

- Portal: `https://<tenant>.vault.jumpcloud.com/app/main/resources/servers`. É preciso estar logado no JumpCloud no Chrome.
- Ao clicar em conectar num servidor, o front (Angular) faz um XHR para `.../ConnectThroughBastion`. A resposta é um JSON com `result.url`, uma URL `vosession://`:

  ```
  vosession://<tenant>.app.vault.jumpcloud.com:443?code=<uuid>&token=<JWT>&version=2&
  ```

  O `token` é um JWT emitido pelo VaultOne, com `exp - iat = 300s`. **Ele vale 5 minutos e é de uso único por sessão**, então precisa ser gerado pelo portal a cada conexão.
- No Windows, o cliente oficial do VaultOne trata o `vosession://`. No Linux o portal não dispara nada, e é aí que entra o userscript: ele intercepta o XHR, lê a resposta (que pode vir como Blob, JSON ou texto, dependendo do Angular) e faz `window.location.href = result.url`, o que entrega a URL ao handler do sistema.

```js
// ==UserScript==
// @name         VaultOne Linux Fixer (God Mode)
// @namespace    http://tampermonkey.net/
// @version      1.4
// @match        *://*.vault.jumpcloud.com/*
// @run-at       document-start
// @grant        none
// ==/UserScript==

(function() {
    'use strict';
    const origOpen = XMLHttpRequest.prototype.open;
    XMLHttpRequest.prototype.open = function(method, url) {
        this._requestUrl = url;
        return origOpen.apply(this, arguments);
    };

    const origSend = XMLHttpRequest.prototype.send;
    XMLHttpRequest.prototype.send = function() {
        this.addEventListener('load', function() {
            if (this._requestUrl && this._requestUrl.includes('ConnectThroughBastion')) {
                const processPayload = (data) => {
                    if (data && data.result && data.result.url && data.result.url.startsWith('vosession://')) {
                        setTimeout(() => { window.location.href = data.result.url; }, 50);
                    }
                };
                try {
                    if (this.responseType === 'blob') {
                        this.response.text().then(text => processPayload(JSON.parse(text)));
                    } else if (this.responseType === 'json') {
                        processPayload(this.response);
                    } else if (this.responseType === '' || this.responseType === 'text') {
                        processPayload(JSON.parse(this.responseText));
                    }
                } catch (e) {
                    console.error("[VaultOne Fixer] Falha ao processar o payload da API", e);
                }
            }
        });
        return origSend.apply(this, arguments);
    };
})();
```

## 3. Handler do protocolo `vosession://`

`~/.local/share/applications/vaultone-rdp-connector.desktop`, registrado como `x-scheme-handler/vosession`:

```ini
[Desktop Entry]
Type=Application
Name=VaultOne RDP Connector
Exec=~/.local/bin/vaultone_automator.sh %u
Icon=utilities-terminal
StartupNotify=false
Terminal=false
MimeType=x-scheme-handler/vosession;
```

## 4. `vaultone_automator.sh`: túnel + RDP

`~/.local/bin/vaultone_automator.sh`:

```bash
#!/bin/bash

URL="$1"
echo "--- DEBUG: A URL recebida foi: '$URL' ---"
/usr/bin/ghostty -e env DOTNET_SYSTEM_NET_DISABLEIPV6=true /usr/local/bin/VaultOneRDPConnector "$URL" &

while ! ss -tln | grep -q ":3390\b"; do
    sleep 0.2
done

xfreerdp3 /v:localhost:3390 /d:"<domínio>" /u:"<usuário>" /p:"-" /client-hostname:"VaultOne" /sec:tls /size:1920x1080 /cert:ignore /network:wan /gfx:AVC444 /bpp:32 +fonts -wallpaper -window-drag -menu-anims -themes
```

- **`VaultOneRDPConnector`** (`/usr/local/bin`, ELF x86-64 de ~63MB, app .NET self-contained, sem pacote): recebe a URL `vosession://`, autentica no bastion com `code` + `token` e abre um túnel TCP local em `127.0.0.1:3390` até a VM. Precisa de `icu` e `openssl`.
- `DOTNET_SYSTEM_NET_DISABLEIPV6=true`: sem isso, o connector tenta IPv6 e falha.
- Ele roda num `ghostty` visível para dar para ver os logs e matar o túnel.
- O script fica em polling até a porta 3390 abrir e então chama o `xfreerdp3` (FreeRDP 3).
- `/p:"-"`: a senha real não passa pelo script, o túnel/bastion cuida da autenticação.
- Flags de performance: `/network:wan /gfx:AVC444 /bpp:32 +fonts -wallpaper -window-drag -menu-anims -themes`.

## Dependências

| Pacote | Uso |
|---|---|
| `openvpn` (2.x) | VPN |
| `oath-toolkit` | `oathtool`, gera o TOTP |
| `freerdp` (3.x) | `xfreerdp3` |
| `ghostty` | terminal do connector |
| `iproute2` | `ss`, espera a porta |
| `icu`, `openssl` | runtime do `VaultOneRDPConnector` |
| Chrome + Tampermonkey | portal + interceptação |

## Dores do fluxo antigo

- Peças espalhadas em `/usr/local/bin`, `~/.local/bin`, `~/.local/share/applications` e no navegador.
- Senha e seed do MFA em texto puro dentro de um script `755`.
- Dois terminais abertos (VPN e connector) mais a janela do RDP.
- O Tampermonkey é frágil: depende do nome do endpoint e do formato da resposta do Angular.
- Não tem status centralizado (VPN está de pé? o túnel caiu?) nem reconexão.

---

## Programa novo: app Tauri

Um app só (Rust + Tauri v2) que substitui todas as peças do fluxo antigo.

## Fluxo

```
Abrir o app
 │
 ├─ 1ª vez? → Assistente de configuração
 │    1. VPN: escolher o .ovpn, usuário, senha, seed do MFA da VPN
 │    2. JumpCloud: e-mail, senha, seed do MFA do JumpCloud
 │       (seeds: colar o base32 OU ler um print do QR "Exportar contas" do Google Authenticator)
 │    3. Connector: verifica se o VaultOneRDPConnector existe;
 │       se não existir, baixa de https://go.vaultone.com/rdp-connector-linux
 │       com barra de progresso, confere o SHA-256 e dá chmod +x
 │    4. Máquina remota: domínio e usuário do Windows
 │    5. Admin (uma vez só): instala o helper da VPN + regra polkit
 │    → segredos no chaveiro do sistema (Secret Service / gnome-keyring)
 │
 ├─ Janela principal
 │    ├─ VPN: gera o TOTP, senha+código → helper root → openvpn 2
 │    │       (mesmas rotas/MTU do fluxo antigo, configuráveis)  ● status
 │    ├─ Portal JumpCloud embutido (WebKitGTK, User-Agent do Chrome)
 │    │    ├─ login preenchido sozinho: e-mail → senha → TOTP
 │    │    └─ script injetado intercepta ConnectThroughBastion → result.url (vosession://)
 │    │         → enviado ao Rust por IPC do Tauri
 │    ├─ Túnel: VaultOneRDPConnector <url> (IPv6 off), logs no app,
 │    │         espera 127.0.0.1:3390                                ● status
 │    └─ RDP: xfreerdp3 com o perfil escolhido                       ● status
 │
 └─ Falhou algo (VPN/túnel/RDP caiu) → indicador vermelho + botão reconectar
```

## Connector

- Link oficial: `https://go.vaultone.com/rdp-connector-linux` → redireciona para `https://cdn.vaultone.com/bastion/VaultOneRDPConnectorLinux`.
- SHA-256 publicado pela VaultOne: `0273a6a2f2adb3ee70c0f7135ec13ff957a1dd455251653f77d22187ea0f4bbb` (65.842.003 bytes). É idêntico ao binário usado no fluxo antigo.
- O app guarda o connector em `~/.local/share/fast-jump-rdp/VaultOneRDPConnector`. Não precisa de sudo, porque o app chama o binário direto com a URL.
- Se o hash do download não bater (a VaultOne pode ter lançado uma versão nova), o app avisa e pergunta antes de usar.
- O handler `vosession://` e o Tampermonkey deixam de existir.

## Perfis do FreeRDP (tela ⚙)

- **Monitores:** um (escolher qual), todos (`/multimon`) ou alguns (`/monitors:0,1`).
- **Janela:** tela cheia, tamanho fixo ou `/dynamic-resolution`.
- **Qualidade:** Rápido / Equilibrado / Máxima, mapeados para `/network`, `/gfx`, `/bpp` e efeitos visuais.
- **Extras:** clipboard, som, microfone, pasta compartilhada, escala.
- **Avançado:** campo livre para flags.
- Vários perfis salvos, por exemplo "2 monitores" e "Notebook".

## Stack

- **Backend:** Rust, Tauri v2, `tokio` para os processos, `totp-rs` para o TOTP, `keyring` para os segredos, `reqwest` para o download, `sha2` para o hash.
- **Frontend:** Svelte 5 + TypeScript + Vite.
- **VPN como root:** um helper pequeno em `/usr/local/libexec/fast-jump-rdp-vpn` mais uma regra polkit que permite ao seu usuário rodá-lo via `pkexec` sem senha. As credenciais vão por stdin, nunca pela linha de comando.
