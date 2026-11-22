#!/bin/bash
# TwinFolders — prepara este Mac para o acesso por SSH com o outro computador.
#
#   ./scripts/ssh-setup-mac.sh                 # cria a chave deste Mac e mostra a parte pública
#   ./scripts/ssh-setup-mac.sh "ssh-ed25519 AAAA… pc-escritorio"
#                                              # + autoriza a chave do outro computador,
#                                              #   desliga login por senha e liga o Login Remoto
#   ./scripts/ssh-setup-mac.sh --undo          # remove a configuração do TwinFolders no sshd
#
# Passos que mudam o sistema pedem a sua senha (sudo). Nada é feito sem você ver.
# DRY_RUN=1 mostra os comandos com sudo em vez de executá-los.
set -euo pipefail

CONF=/etc/ssh/sshd_config.d/100-twinfolders.conf
KEY="$HOME/.ssh/id_ed25519"
AUTH="$HOME/.ssh/authorized_keys"
USER_NAME="$(id -un)"

say()  { printf '\n\033[1m%s\033[0m\n' "$*"; }
info() { printf '  %s\n' "$*"; }
die()  { printf '\n\033[31mErro:\033[0m %s\n' "$*" >&2; exit 1; }
root() { if [ "${DRY_RUN:-0}" = 1 ]; then printf '  [dry-run] sudo %s\n' "$*"; else sudo "$@"; fi; }

if [ "${1:-}" = "--undo" ]; then
  say "Removendo a configuração do TwinFolders no SSH"
  root rm -f "$CONF"
  info "Pronto. O Login Remoto continua como estava; desligue em Ajustes do Sistema → Geral → Compartilhamento se quiser."
  exit 0
fi

say "1/4 · Chave deste Mac"
mkdir -p "$HOME/.ssh" && chmod 700 "$HOME/.ssh"
if [ -f "$KEY" ]; then
  info "Já existe: $KEY"
else
  info "Criando uma chave ed25519. Escolha uma frase-senha (recomendado) ou deixe em branco."
  ssh-keygen -t ed25519 -a 100 -C "twinfolders-$(hostname -s)" -f "$KEY"
fi
info "Copie a linha abaixo e use no OUTRO computador (no script de lá):"
printf '\n%s\n' "$(cat "$KEY.pub")"

OTHER="${1:-}"
if [ -z "$OTHER" ]; then
  say "Próximo passo"
  info "Rode de novo passando a chave pública do outro computador entre aspas:"
  info "  ./scripts/ssh-setup-mac.sh \"ssh-ed25519 AAAA… nome\""
  exit 0
fi

say "2/4 · Autorizando a chave do outro computador"
case "$OTHER" in
  ssh-ed25519\ *|sk-ssh-ed25519@openssh.com\ *) ;;
  *) die "isso não parece uma chave pública ed25519 (deve começar com ssh-ed25519)." ;;
esac
case "$OTHER" in *$'\n'*) die "cole apenas uma linha." ;; esac
touch "$AUTH" && chmod 600 "$AUTH"
if grep -qxF "$OTHER" "$AUTH"; then
  info "Essa chave já estava autorizada."
else
  printf '%s\n' "$OTHER" >> "$AUTH"
  info "Autorizada em $AUTH"
fi

say "3/4 · Desligando login por senha (só chaves)"
TMP="$(mktemp)"
cat > "$TMP" <<EOF
# Criado pelo TwinFolders (scripts/ssh-setup-mac.sh). Remova com --undo.
PasswordAuthentication no
KbdInteractiveAuthentication no
PermitRootLogin no
PubkeyAuthentication yes
AllowUsers $USER_NAME
EOF
if ! grep -qE '^[[:space:]]*Include[[:space:]]+/etc/ssh/sshd_config.d/\*' /etc/ssh/sshd_config 2>/dev/null; then
  rm -f "$TMP"
  die "/etc/ssh/sshd_config não inclui sshd_config.d (macOS muito antigo?). Nada foi alterado."
fi
root install -m 644 "$TMP" "$CONF"
rm -f "$TMP"
if [ "${DRY_RUN:-0}" != 1 ] && ! sudo /usr/sbin/sshd -t; then
  sudo rm -f "$CONF"
  die "a configuração do SSH ficou inválida e foi desfeita. Nada mudou."
fi
info "Só a conta \"$USER_NAME\", só com chave."

say "4/4 · Ligando o Login Remoto"
if root systemsetup -setremotelogin on 2>/dev/null; then
  info "Login Remoto ligado."
else
  info "O macOS não deixou ligar por aqui. Ligue manualmente:"
  info "Ajustes do Sistema → Geral → Compartilhamento → Login Remoto (permitir só \"$USER_NAME\")."
fi

say "Pronto"
info "No outro computador, o TwinFolders abre um terminal aqui usando o usuário: $USER_NAME"
info "Para desfazer: ./scripts/ssh-setup-mac.sh --undo"
