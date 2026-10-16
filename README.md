# JumpDesk

App para Arch Linux que leva você da VPN até a tela da VM Windows da empresa
com um clique:

1. sobe a **VPN** (OpenVPN) gerando o código MFA sozinho;
2. abre o **portal do JumpCloud** dentro do app e faz o login por você;
3. quando você clica no servidor, sobe o **túnel** (VaultOneRDPConnector);
4. abre o **RDP** (xfreerdp3) — se quiser, direto num workspace fixo do Hyprland.

---

## Instalar (primeira vez)

Precisa de Arch Linux (ou derivado) com o [`yay`](https://github.com/Jguer/yay).

```bash
git clone https://github.com/th3-Rocha/JumpDesk-Omarchy.git ~/jumpdesk
cd ~/jumpdesk
yay -Bi packaging
```

O `yay` instala as dependências, compila (a primeira vez leva alguns minutos,
é o Rust) e instala o pacote `jumpdesk-git`.

Depois, **uma vez só**, libere a VPN para não pedir senha de admin:

```bash
sudo usermod -aG fastjumprdp $USER
```

e **saia e entre na sessão** (ou reinicie) para valer.

Abra o **JumpDesk** pelo lançador de apps (procure por "JumpDesk"), ou rode
`jumpdesk` no terminal.

### Primeiro uso

O app abre um assistente de 3 passos. Tenha em mãos:

- o arquivo **.ovpn** da VPN, seu usuário e senha da VPN;
- a **URL do portal** do JumpCloud PAM (a página de servidores, algo como
  `https://<empresa>.vault.jumpcloud.com/app/main/resources/servers`) e seu
  e-mail e senha do **JumpCloud**;
- os **seeds do MFA** (VPN e JumpCloud). Dá para importar direto do Google
  Authenticator: no celular, *Transferir contas → Exportar*, tire print do QR e
  use **Ler QR de imagem…** (é lido offline, nada sai da sua máquina);
- domínio e usuário do Windows (a senha do Windows é opcional).

Na tela principal, se aparecer **"VaultOneRDPConnector não instalado"**, clique
em **Baixar agora**.

### Uso do dia a dia

1. **Conectar tudo** → sobe a VPN e abre o portal (o login é automático).
2. No portal, clique no servidor e em **Direct Access**.
3. O túnel e o RDP sobem sozinhos. Na tela da VM, digite a senha do Windows
   (se não salvou no assistente).

Em **Perfis RDP** você ajusta resolução, monitores, qualidade, som, pasta
compartilhada e o **workspace do Hyprland** onde o RDP abre (inclusive o
scratchpad, `Super + S`).

Deu problema? A aba **Logs** mostra tudo; **Limpar** encerra VPN, túnel e RDP
que tenham sobrado.

---

## Atualizar

Sempre que sair versão nova no GitHub:

```bash
cd ~/jumpdesk
git restore packaging/PKGBUILD   # o build anota a versão nele; descarta isso
git pull
yay -Bi packaging
```

Isso recompila a versão mais nova e substitui o pacote. **Seus dados não são
apagados** (veja abaixo). Feche o JumpDesk antes e abra de novo depois.

> Por que não `yay -Syu`? O `yay -Syu` só atualiza pacotes do AUR, e este não
> está lá. O comando acima faz a mesma coisa a partir deste repositório.

## Onde ficam seus dados

Tudo o que você cadastra fica na sua pasta pessoal, **fora do pacote**:

| o quê | onde |
| --- | --- |
| configurações e perfis RDP | `~/.config/fast-jump-rdp/config.toml` |
| senhas e seeds do MFA | `~/.config/fast-jump-rdp/secrets.json` (só seu usuário lê) |
| VaultOneRDPConnector e logs | `~/.local/share/fast-jump-rdp/` |

Atualizar, reinstalar ou desinstalar o pacote **nunca** mexe nessas pastas.
Para trocar de computador, copie as duas pastas. Para começar do zero, apague-as.

## Desinstalar

```bash
yay -R jumpdesk-git
```

(Os dados continuam nas pastas acima; apague-as se quiser.)

---

## Problemas comuns

- **"exists in filesystem" ao instalar** (`fast-jump-rdp-vpn` ou
  `49-fast-jump-rdp.rules`) → você já tinha rodado o `install-system.sh`. Instale
  deixando o pacote assumir esses arquivos:
  `yay -Bi packaging --overwrite '/usr/local/libexec/fast-jump-rdp-vpn,/etc/polkit-1/rules.d/49-fast-jump-rdp.rules'`
- **A VPN pede senha de admin toda vez** → faltou o `usermod` acima, ou você
  não saiu/entrou na sessão depois dele. Confira com `groups | grep fastjumprdp`.
- **Janela em branco** (algumas GPUs) → abra com
  `WEBKIT_DISABLE_DMABUF_RENDERER=1 jumpdesk`.
- **"Não deu para falar com a VM"** → a VM provavelmente está desligada. Ligue
  pelo portal e tente de novo.
- **O RDP demora para abrir** → às vezes o bastion leva ~1–2 min para atender;
  o status fica em "conectando…" até a tela chegar.

---

## Para desenvolvedores

```bash
pnpm install
pnpm tauri dev       # abre o app em modo de desenvolvimento
```

Sem o pacote instalado, instale o helper da VPN com `sudo packaging/install-system.sh`.
Detalhes da implementação em [`IMPLEMENTATION.md`](IMPLEMENTATION.md) e
[`SCOPE.md`](SCOPE.md); o fluxo manual que o app substituiu está em
[`docs/FLUXO-ANTIGO.md`](docs/FLUXO-ANTIGO.md).
