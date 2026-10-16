#!/bin/bash
# Instala as partes de sistema do Fast Jump RDP (helper da VPN + regra polkit).
# Roda uma vez, com sudo. O app em si roda sem root.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
USERNAME="${SUDO_USER:-$USER}"

echo "==> Grupo fastjumprdp"
getent group fastjumprdp >/dev/null || groupadd fastjumprdp
usermod -aG fastjumprdp "$USERNAME"

echo "==> Helper da VPN"
install -Dm755 "$HERE/fast-jump-rdp-vpn" /usr/local/libexec/fast-jump-rdp-vpn

echo "==> Regra polkit"
install -Dm644 "$HERE/49-fast-jump-rdp.rules" /etc/polkit-1/rules.d/49-fast-jump-rdp.rules

echo
echo "OK. Saia e entre na sessão (ou reinicie) para o grupo valer."
echo "Dependências de runtime: openvpn freerdp oath-toolkit icu openssl webkit2gtk-4.1 gnome-keyring"
