# Roteiro de testes manuais — macOS (fatia 3: serviço em segundo plano)

> Status: **NÃO VERIFICADO**. Marque ☐ → ☑ conforme executa e preencha a tabela final.
> Critérios de aceite cobertos (SPEC §6): "Reiniciar qualquer máquina mantém a sync funcionando sem abrir o app" e "Nenhuma porta aberta no roteador".

Valores de referência: label `dev.twinfolders.syncthing`; plist `~/Library/LaunchAgents/dev.twinfolders.syncthing.plist`; home `~/Library/Application Support/TwinFolders/syncthing`; log `~/Library/Logs/TwinFolders/syncthing.log`; GUI `127.0.0.1:8385`; sync tcp `22001`.

## 1. Pré-requisitos
- ☐ macOS 13 ou superior: `sw_vers -productVersion` → versão >= 13.
- ☐ Syncthing instalado: `brew install syncthing` e depois `syncthing --version` → imprime `syncthing v1.x/2.x ...`.
- ☐ Descobrir o binário: `which syncthing` ou `brew --prefix syncthing`/bin/syncthing → ex.: `/opt/homebrew/bin/syncthing` (Apple Silicon) ou `/usr/local/bin/syncthing` (Intel). Anote: `BIN=$(which syncthing); echo $BIN`.
- ☐ Nada ocupando as portas: `lsof -nP -iTCP:8385 -iTCP:22001 -sTCP:LISTEN` → sem saída.
- ☐ Estado limpo: `twinfolders service status` → "não instalado" (ou equivalente); `ls ~/Library/LaunchAgents/dev.twinfolders.syncthing.plist` → "No such file".
- ☐ Sessão gráfica ativa (LaunchAgent exige login do usuário): `id -u` → número (ex.: 501).

## 2. Instalar e verificar
- ☐ `twinfolders service install --syncthing "$BIN" --gui-port 8385 --listen-port 22001` → termina com código 0 (`echo $?` → 0) e mensagem de sucesso.
- ☐ Plist existe: `ls -l ~/Library/LaunchAgents/dev.twinfolders.syncthing.plist` → arquivo presente.
- ☐ Plist válido: `plutil -lint ~/Library/LaunchAgents/dev.twinfolders.syncthing.plist` → `OK`.
- ☐ Registrado no launchd: `launchctl print gui/$(id -u)/dev.twinfolders.syncthing` → mostra `state = running`, `pid = N`, `program`/`arguments` com o binário e `KeepAlive`.
- ☐ Processo rodando: `pgrep -fl syncthing` → uma linha com o PID acima.
- ☐ `twinfolders service status` → instalado: sim, rodando: sim, PID igual ao do `pgrep`, GUI `127.0.0.1:8385`, device ID (7 blocos de 8 caracteres separados por hífen).
- ☐ Log sendo escrito: `tail -n 20 ~/Library/Logs/TwinFolders/syncthing.log` → linhas do Syncthing (ex.: "My ID", "GUI and API listening on 127.0.0.1:8385").
- ☐ Ping SEM chave: `curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:8385/rest/system/ping` → `403`.
- ☐ Item no Keychain existe (sem abrir o conteúdo, sem diálogo): `security find-generic-password -s twinfolders -a syncthing-api-key >/dev/null && echo ok` → `ok`.
- ☐ Obter a chave para os testes a partir do `config.xml` (dono = seu usuário, 0600): `KEY=$(sed -n 's:.*<apikey>\(.*\)</apikey>.*:\1:p' "$HOME/Library/Application Support/TwinFolders/syncthing/config.xml"); echo ${#KEY}` → > 0.
  - ⚠️ `security find-generic-password ... -w` abre um **diálogo do Keychain** pedindo permissão, porque o item pertence ao binário `twinfolders` (comportamento esperado do Keychain; verificado em 2026-11-16). Em terminal sem interface ele fica travado esperando.
- ☐ Ping COM chave: `curl -s -H "X-API-Key: $KEY" http://127.0.0.1:8385/rest/system/ping` → `{"ping":"pong"}`.
- ☐ Chave errada: `curl -s -o /dev/null -w '%{http_code}\n' -H "X-API-Key: errada" http://127.0.0.1:8385/rest/system/ping` → `403`.
- ☐ Reexecutar `twinfolders service install ...` (idempotência) → sem erro; continua um único processo (`pgrep -fc syncthing` → 1).

## 3. Verificações de segurança
- ☐ Porta 8385 só em loopback: `lsof -nP -iTCP:8385 -sTCP:LISTEN` → endereço `127.0.0.1:8385` (NÃO `*:8385`, `0.0.0.0` nem IP da LAN/Tailscale).
- ☐ Porta de sync: `lsof -nP -iTCP:22001 -sTCP:LISTEN` → escutando em `22001` (tcp).
- ☐ Chave fora da linha de comando: `ps -axww -o command | grep -i [s]yncthing | grep -F "$KEY"` → sem saída; e `ps -axww -o command | grep [s]yncthing` não contém `--gui-apikey`.
- ☐ Chave fora do plist: `grep -F "$KEY" ~/Library/LaunchAgents/dev.twinfolders.syncthing.plist` → sem saída; `grep -i apikey ~/Library/LaunchAgents/dev.twinfolders.syncthing.plist` → sem saída.
- ☐ Chave fora do ambiente do processo: `launchctl print gui/$(id -u)/dev.twinfolders.syncthing | grep -F "$KEY"` → sem saída.
- ☐ Opções do config.xml: `CFG="$HOME/Library/Application Support/TwinFolders/syncthing/config.xml"; grep -E '<(natEnabled|relaysEnabled|globalAnnounceEnabled)>' "$CFG"` → as três linhas com `false`.
- ☐ Telemetria: `grep -E '<urAccepted>' "$CFG"` → `-1` (uso reportado desabilitado).
- ☐ GUI address no config: `grep -E '<address>127.0.0.1:8385</address>' "$CFG"` → presente.
- ☐ Roteador (opcional): abrir a página do roteador, seção UPnP/NAT-PMP/redirecionamento de portas → nenhum mapeamento novo para 22001/8385 criado pelo Mac.
- ☐ Permissões do home: `ls -ld "$HOME/Library/Application Support/TwinFolders/syncthing"` → dono é o seu usuário.

## 4. Resiliência
- ☐ Anotar PID: `OLD=$(pgrep -f syncthing | head -1); echo $OLD`.
- ☐ Crash: `kill -9 $OLD` e `sleep 10; pgrep -fl syncthing` → NOVO PID diferente de `$OLD` em até ~10 s.
- ☐ `launchctl print gui/$(id -u)/dev.twinfolders.syncthing | grep -E 'state|pid|last exit'` → `state = running`, novo pid.
- ☐ Ping com chave volta a responder `pong` após o restart: `curl -s -H "X-API-Key: $KEY" http://127.0.0.1:8385/rest/system/ping`.
- ☐ Desligamento limpo: `curl -s -X POST -H "X-API-Key: $KEY" http://127.0.0.1:8385/rest/system/shutdown` → `{"ok":"shutting down"}`.
- ☐ Após 15 s: `pgrep -fl syncthing` → sem saída (NÃO reiniciou; KeepAlive só reage a saída não-zero).
- ☐ `twinfolders service status` → instalado: sim, rodando: não.
- ☐ Voltar a subir sem reinstalar: `launchctl kickstart gui/$(id -u)/dev.twinfolders.syncthing` (ou `launchctl bootout` + `bootstrap`) → processo volta; ping com chave = pong. Se o job saiu do launchd, `launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/dev.twinfolders.syncthing.plist`.
- ☐ Sem duplicação: `pgrep -fc syncthing` → 1.

## 5. Reinício (critério de aceite)
- ☐ (Opcional) Parear com o outro computador e compartilhar uma pasta de teste; confirmar sync normal antes do reboot.
- ☐ Reiniciar o Mac: menu Apple → Reiniciar (ou `sudo shutdown -r now`).
- ☐ Fazer login. NÃO abrir o app TwinFolders, o navegador nem o terminal além do necessário.
- ☐ `pgrep -fl syncthing` → processo rodando (novo PID).
- ☐ `launchctl print gui/$(id -u)/dev.twinfolders.syncthing | grep -E 'state|pid'` → `state = running`.
- ☐ Ping com chave → `pong`; sem chave → `403`.
- ☐ Log mostra início recente: `tail -n 5 ~/Library/Logs/TwinFolders/syncthing.log` → timestamps do boot atual.
- ☐ Com par pareado: criar `teste-reboot-$(date +%s).txt` na pasta sincronizada da OUTRA máquina → o arquivo chega neste Mac em poucos segundos, sem abrir o app.
- ☐ Repetir no sentido inverso: criar arquivo no Mac, confirmar na outra máquina.

## 6. Desinstalar
- ☐ Registrar antes: `ls "$HOME/Library/Application Support/TwinFolders/syncthing"` e a pasta sincronizada (ex.: `ls ~/TwinFolders-teste`).
- ☐ `twinfolders service uninstall` → código 0, mensagem de remoção.
- ☐ Serviço removido: `launchctl print gui/$(id -u)/dev.twinfolders.syncthing` → erro "Could not find service".
- ☐ Plist removido: `ls ~/Library/LaunchAgents/dev.twinfolders.syncthing.plist` → "No such file".
- ☐ Processo parado: `pgrep -fl syncthing` → sem saída; `lsof -nP -iTCP:8385 -iTCP:22001 -sTCP:LISTEN` → sem saída.
- ☐ Home preservado: `ls "$HOME/Library/Application Support/TwinFolders/syncthing"` → `config.xml`, `cert.pem`, `key.pem`, `index-*` ainda presentes.
- ☐ Pastas sincronizadas intactas: mesmos arquivos de antes.
- ☐ `twinfolders service status` → não instalado.
- ☐ Desinstalar de novo (idempotência): `twinfolders service uninstall` → sem erro fatal.
- ☐ Reinstalar (`service install`) reaproveita o mesmo device ID: compare com o anotado no passo 2.

## Conflitos e versionamento (Fatia 5)

- ☐ Com os dois computadores pausados um para o outro, edite o mesmo arquivo nos dois e retome. Em até ~1 min aparece uma **notificação nativa** "Conflito de sincronização" (uma só por conflito) e o topo do painel mostra "1 conflito para resolver".
- ☐ No cartão do conflito, "Manter as duas": a cópia vira `nome (conflito XXXXXXX AAAA-MM-DD HH.MM.SS).ext` ao lado do original, nos **dois** computadores.
- ☐ Repita e escolha "Usar a cópia": o original vai para `.stversions/…/nome~AAAAMMDD-HHMMSS.ext` e a cópia toma o lugar; o outro computador recebe a mudança e o cartão some dos dois lados.
- ☐ "Versões" numa pasta: troque para "Últimas N versões" (5), salve, reabra e confira que persistiu. No painel web do Syncthing a pasta mostra o mesmo tipo.

## SSH via Tailscale (Fatia 6)

- ☐ Tailscale instalado e logado nos dois com a mesma conta; `tailscale status` mostra o outro computador.
- ☐ `./scripts/ssh-setup-mac.sh` cria a chave e mostra a linha `ssh-ed25519 …`.
- ☐ Rodar de novo com a chave do outro: termina em "Pronto"; repetir mostra "já estava autorizada".
- ☐ Do outro computador, `ssh usuario@nome-no-tailscale` entra sem pedir senha; com outra chave (ou `-o PubkeyAuthentication=no`) é recusado (senha desligada).
- ☐ No app, Computadores → Acesso remoto mostra "Pronto em …" e "Abrir terminal" abre o terminal já conectado.
- ☐ Desfazer (`--undo` / `-Undo`) volta ao estado anterior.

## Instalador e primeira abertura (Fatia 7) — NÃO VERIFICADO

- ☐ Baixar o `.dmg` do rascunho de release; `shasum -a 256` bate com `SHA256SUMS-macOS.txt`.
- ☐ Arrastar para Aplicativos; botão direito → Abrir funciona (sem certificado) ou abre direto (assinado e notarizado).
- ☐ Num Mac sem Syncthing: a tela "Vamos preparar este computador" aparece; "Instalar com o Homebrew" abre o Terminal com `brew install syncthing`; ao terminar, o passo 2 libera sozinho (até ~4 s).
- ☐ "Ligar agora": em poucos segundos aparece o painel normal; `launchctl print gui/$(id -u)/dev.twinfolders.syncthing` mostra o serviço; nenhum prompt do Keychain.
- ☐ Sair do app e reiniciar o Mac: a sincronização volta sozinha.
- ☐ Desinstalar pelos comandos do README: serviço some, pastas ficam.

## Vários computadores (Fatia 8) — NÃO VERIFICADO

- ☐ Com 3 ou mais computadores: em Preferências, o papel muda os textos (Central: "Computadores do time").
- ☐ Adicionar pasta marcando 2 computadores: os dois recebem o pedido; a linha da pasta mostra "Com A e B".
- ☐ Compartilhar com…: tirar um computador mostra o aviso; ele para de receber mudanças e mantém os arquivos.
- ☐ Desconectar um computador: some da lista e das pastas; os arquivos dele continuam lá.
- ☐ Tailscale com um PC marcado com `tag:escritorio`: o Acesso remoto o reconhece.

## Convite para o time (Fatia 9) — NÃO VERIFICADO

- ☐ Central: Gerar convite com 1 pasta marcada; Copiar convite; a lista mostra o código e "vence em 24 h".
- ☐ Computador do time: colar o convite mostra "Entrar no espaço de <central>" com o início do ID certo; Entrar no time.
- ☐ Central: o pedido aparece como "Convidado · código", com o nome sem o código; aprovar oferece a pasta escolhida.
- ☐ Computador do time: depois de aprovado, o nome volta ao normal e a tela "Esperando a central aprovar" some.
- ☐ Usar o mesmo convite num terceiro computador: o pedido aparece como "Convite vencido ou já usado".
- ☐ Computador do time tenta aprovar/aceitar algo de outro computador: o app recusa com a mensagem da central.

## 7. Resultados

| Passo | Resultado | Observações |
|---|---|---|
| 1. Pré-requisitos | NÃO VERIFICADO | |
| 2. Instalar e verificar (launchd, processo, ping 403/pong) | NÃO VERIFICADO | |
| 3. Segurança (loopback, chave fora de ps/plist, config.xml, roteador) | NÃO VERIFICADO | |
| 4a. Resiliência: kill -9 reinicia em ~10 s | NÃO VERIFICADO | |
| 4b. Desligamento limpo não reinicia | NÃO VERIFICADO | |
| 5. Reinício sem abrir o app (+ arquivo do par chega) | NÃO VERIFICADO | |
| 6. Desinstalar (home e pastas preservados) | NÃO VERIFICADO | |
| Conflitos e versionamento (Fatia 5) | NÃO VERIFICADO | |
| SSH via Tailscale (Fatia 6) | NÃO VERIFICADO | |
