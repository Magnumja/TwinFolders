# Roteiro de testes manuais — Windows (fatia 3: serviço em segundo plano)

> Status: **NÃO VERIFICADO**. Marque ☐ → ☑ conforme executa e preencha a tabela final.
> Critérios de aceite cobertos (SPEC §6): "Reiniciar qualquer máquina mantém a sync funcionando sem abrir o app" e "Nenhuma porta aberta no roteador".
> Use PowerShell 7 ou Windows PowerShell 5.1, como usuário comum (sem "Executar como administrador").

Valores de referência: tarefa `\TwinFolders\Syncthing` (gatilho: logon do usuário atual, privilégio mínimo, sem limite de tempo, reinício em falha a cada 1 min, executa na bateria); home `%LOCALAPPDATA%\TwinFolders\syncthing`; log `%LOCALAPPDATA%\TwinFolders\logs\syncthing.log`; GUI `127.0.0.1:8385`; sync tcp `22001`; flags `--no-console --no-browser --no-restart --no-port-probing`.

## 1. Pré-requisitos
- ☐ Windows 10 22H2 ou 11: `[System.Environment]::OSVersion.Version; (Get-ComputerInfo).OsName` → build >= 19045.
- ☐ Instalar Syncthing: `winget install --id Syncthing.Syncthing -e` → "Instalado com êxito". Reabra o PowerShell.
- ☐ Descobrir o binário: `(Get-Command syncthing.exe).Source` → caminho. Se vazio: `Get-ChildItem "$env:LOCALAPPDATA\Microsoft\WinGet\Packages" -Recurse -Filter syncthing.exe | Select -First 1 -Expand FullName`. Guarde: `$BIN = '<caminho>'; & $BIN --version` → `syncthing v...`.
- ☐ Portas livres: `Get-NetTCPConnection -LocalPort 8385,22001 -State Listen -ErrorAction SilentlyContinue` → sem saída.
- ☐ Estado limpo: `Get-ScheduledTask -TaskPath '\TwinFolders\' -ErrorAction SilentlyContinue` → sem saída; `twinfolders service status` → não instalado.
- ☐ (Opcional) módulo para Credential Manager: `Install-Module CredentialManager -Scope CurrentUser` (aceite o prompt do PSGallery).

## 2. Instalar e verificar
- ☐ `twinfolders service install --syncthing $BIN --gui-port 8385 --listen-port 22001` → código 0 (`$LASTEXITCODE` → 0).
- ☐ Firewall (primeira execução): se aparecer o prompt do Windows Defender Firewall para `syncthing.exe`, marcar APENAS "Redes privadas" (desmarcar públicas) e permitir. Interface Tailscale deve estar no perfil Privada: `Get-NetConnectionProfile`.
- ☐ Tarefa registrada: `Get-ScheduledTask -TaskPath '\TwinFolders\' -TaskName Syncthing | Format-List TaskName,State,TaskPath` → `State: Running` (ou `Ready` se ainda não disparou).
- ☐ Info da tarefa: `Get-ScheduledTaskInfo -TaskPath '\TwinFolders\' -TaskName Syncthing` → `LastTaskResult` 0 (ou 267009 = em execução), `LastRunTime` recente.
- ☐ Configuração da tarefa: `$t = Get-ScheduledTask -TaskPath '\TwinFolders\' -TaskName Syncthing; $t.Triggers[0].CimClass.CimClassName; $t.Principal.RunLevel; $t.Principal.LogonType; $t.Settings | Select ExecutionTimeLimit,DisallowStartIfOnBatteries,StopIfGoingOnBatteries,RestartCount,RestartInterval` → gatilho `MSFT_TaskLogonTrigger`; RunLevel `Limited`; LogonType `Interactive`; `ExecutionTimeLimit` `PT0S`; baterias: `False`/`False`; `RestartInterval` `PT1M`; `RestartCount` >= 1.
- ☐ Processo rodando: `Get-Process syncthing | Select Id,Path,StartTime` → um processo (o `Path` bate com `$BIN`).
- ☐ `twinfolders service status` → instalado: sim, rodando: sim, PID igual ao de `Get-Process`, GUI `127.0.0.1:8385`, device ID.
- ☐ Log: `Get-Content "$env:LOCALAPPDATA\TwinFolders\logs\syncthing.log" -Tail 20` → linhas do Syncthing ("GUI and API listening on 127.0.0.1:8385").
- ☐ Ping SEM chave: `try { Invoke-RestMethod http://127.0.0.1:8385/rest/system/ping } catch { $_.Exception.Response.StatusCode.value__ }` → `403`.
- ☐ Obter a chave (a): `cmdkey /list | Select-String twinfolders` → mostra alvo contendo `twinfolders` / `syncthing-api-key` (cmdkey NÃO exibe a senha).
- ☐ Obter a chave (b): `$KEY = (Get-StoredCredential -Target 'twinfolders:syncthing-api-key').GetNetworkCredential().Password` (ajuste o Target conforme o mostrado por `cmdkey /list`). Alternativa: `$KEY = ([xml](Get-Content "$env:LOCALAPPDATA\TwinFolders\syncthing\config.xml")).configuration.gui.apikey`. `$KEY.Length` → > 0.
- ☐ Ping COM chave: `Invoke-RestMethod http://127.0.0.1:8385/rest/system/ping -Headers @{ 'X-API-Key' = $KEY }` → `ping: pong`.

## 3. Verificações de segurança
- ☐ 8385 só em loopback: `Get-NetTCPConnection -LocalPort 8385 -State Listen | Select LocalAddress,LocalPort,OwningProcess` → `LocalAddress` = `127.0.0.1` (NÃO `0.0.0.0` nem `::`); `OwningProcess` = PID do syncthing.
- ☐ Porta de sync: `Get-NetTCPConnection -LocalPort 22001 -State Listen` → escutando em 22001.
- ☐ Chave fora da linha de comando: `(Get-CimInstance Win32_Process -Filter "Name='syncthing.exe'").CommandLine` → contém `--no-console --no-browser --no-restart --no-port-probing` e NÃO contém `$KEY` nem `apikey`. Teste: `(Get-CimInstance Win32_Process -Filter "Name='syncthing.exe'").CommandLine -like "*$KEY*"` → `False`.
- ☐ Chave fora do XML da tarefa: `(Export-ScheduledTask -TaskPath '\TwinFolders\' -TaskName Syncthing) -like "*$KEY*"` → `False`; e `... -match 'apikey'` → `False`.
- ☐ Opções do config.xml: `Select-String -Path "$env:LOCALAPPDATA\TwinFolders\syncthing\config.xml" -Pattern '<(natEnabled|relaysEnabled|globalAnnounceEnabled|urAccepted)>'` → `natEnabled`, `relaysEnabled`, `globalAnnounceEnabled` = `false`; `urAccepted` = `-1`.
- ☐ Regras de firewall: `Get-NetFirewallRule -DisplayName '*syncthing*' | Select DisplayName,Profile,Action,Enabled` → perfil `Private` apenas (sem `Public`/`Any`).
- ☐ Roteador (opcional): na página do roteador, UPnP/NAT-PMP/redirecionamento → nenhum mapeamento novo para 22001/8385.

## 4. Resiliência
- ☐ Anotar PID: `$old = (Get-Process syncthing).Id; $old`.
- ☐ Crash: `Stop-Process -Id $old -Force` e aguardar até ~1 min: `Start-Sleep 75; Get-Process syncthing | Select Id` → NOVO PID diferente de `$old`.
- ☐ `Get-ScheduledTaskInfo -TaskPath '\TwinFolders\' -TaskName Syncthing | Select LastRunTime,LastTaskResult` → `LastRunTime` atualizado.
- ☐ Atenção: encerrar só o `syncthing.exe` filho (monitor + filho) — confirme com `Get-CimInstance Win32_Process -Filter "Name='syncthing.exe'" | Select ProcessId,ParentProcessId` que mata o processo correto; com `--no-restart` há um único processo.
- ☐ Ping com chave volta a `pong` após o restart.
- ☐ Desligamento limpo: `Invoke-RestMethod -Method Post http://127.0.0.1:8385/rest/system/shutdown -Headers @{ 'X-API-Key' = $KEY }` → `ok: shutting down`.
- ☐ Comportamento esperado no Windows: o processo sai com código 0, a tarefa TERMINA (`Get-ScheduledTask ... | Select State` → `Ready`) e NÃO é reiniciada (a reinicialização por falha só vale para código != 0). Verificar após 2 min: `Get-Process syncthing -ErrorAction SilentlyContinue` → sem saída. Anote o comportamento observado.
- ☐ Voltar a subir: `Start-ScheduledTask -TaskPath '\TwinFolders\' -TaskName Syncthing` (ou novo logon) → processo volta; ping com chave = pong.
- ☐ Sem duplicação: `(Get-Process syncthing).Count` → 1.

## 5. Reinício (critério de aceite)
- ☐ (Opcional) Parear com o outro computador e compartilhar uma pasta de teste; confirmar sync antes do reboot.
- ☐ Reiniciar: `Restart-Computer`.
- ☐ Fazer logon com o mesmo usuário. NÃO abrir o app TwinFolders nem o navegador.
- ☐ `Get-Process syncthing` → rodando (novo PID, `StartTime` pós-boot).
- ☐ `Get-ScheduledTaskInfo -TaskPath '\TwinFolders\' -TaskName Syncthing` → `LastRunTime` ≈ horário do logon; `LastTaskResult` 267009 (em execução) ou 0.
- ☐ Ping com chave → `pong`; sem chave → 403.
- ☐ Sem janela de console visível e sem navegador aberto (`--no-console --no-browser`).
- ☐ Com par pareado: criar `teste-reboot-<hora>.txt` na pasta sincronizada da OUTRA máquina → chega neste Windows em poucos segundos: `Get-ChildItem <pasta> -Filter teste-reboot-*`.
- ☐ Repetir no sentido inverso.

## 6. Desinstalar
- ☐ Registrar antes: `Get-ChildItem "$env:LOCALAPPDATA\TwinFolders\syncthing"` e a pasta sincronizada.
- ☐ `twinfolders service uninstall` → código 0.
- ☐ Tarefa removida: `Get-ScheduledTask -TaskPath '\TwinFolders\' -ErrorAction SilentlyContinue` → sem saída.
- ☐ Processo parado: `Get-Process syncthing -ErrorAction SilentlyContinue` → sem saída; `Get-NetTCPConnection -LocalPort 8385,22001 -State Listen -ErrorAction SilentlyContinue` → sem saída.
- ☐ Home preservado: `Test-Path "$env:LOCALAPPDATA\TwinFolders\syncthing\config.xml"` → `True` (e `cert.pem`, `key.pem`, `index-*`).
- ☐ Pastas sincronizadas intactas: mesmos arquivos de antes.
- ☐ `twinfolders service status` → não instalado.
- ☐ Desinstalar de novo (idempotência) → sem erro fatal.
- ☐ Reinstalar reaproveita o mesmo device ID anotado no passo 2.

## Conflitos e versionamento (Fatia 5)

- ☐ Com os dois computadores pausados um para o outro, edite o mesmo arquivo nos dois e retome. Em até ~1 min aparece uma **notificação nativa** "Conflito de sincronização" (uma só por conflito) e o topo do painel mostra "1 conflito para resolver".
- ☐ No cartão do conflito, "Manter as duas": a cópia vira `nome (conflito XXXXXXX AAAA-MM-DD HH.MM.SS).ext` ao lado do original, nos **dois** computadores.
- ☐ Repita e escolha "Usar a cópia": o original vai para `.stversions/…/nome~AAAAMMDD-HHMMSS.ext` e a cópia toma o lugar; o outro computador recebe a mudança e o cartão some dos dois lados.
- ☐ "Versões" numa pasta: troque para "Últimas N versões" (5), salve, reabra e confira que persistiu. No painel web do Syncthing a pasta mostra o mesmo tipo.
- ☐ (Windows) A notificação aparece na Central de Ações; nome com acento e caminho com espaço resolvem sem erro.

## SSH via Tailscale (Fatia 6)

- ☐ Tailscale instalado e logado nos dois com a mesma conta; `tailscale status` mostra o outro computador.
- ☐ `powershell -ExecutionPolicy Bypass -File .\scripts\ssh-setup-windows.ps1 (como administrador)` cria a chave e mostra a linha `ssh-ed25519 …`.
- ☐ Rodar de novo com a chave do outro: termina em "Pronto"; repetir mostra "já estava autorizada".
- ☐ Do outro computador, `ssh usuario@nome-no-tailscale` entra sem pedir senha; com outra chave (ou `-o PubkeyAuthentication=no`) é recusado (senha desligada).
- ☐ No app, Computadores → Acesso remoto mostra "Pronto em …" e "⋯ → Abrir terminal" no cartão do computador abre o terminal já conectado.
- ☐ Desfazer (`--undo` / `-Undo`) volta ao estado anterior.

## Instalador e primeira abertura (Fatia 7) — NÃO VERIFICADO

- ☐ Baixar o `.msi` (ou `-setup.exe`) do rascunho de release; `Get-FileHash` bate com `SHA256SUMS-Windows.txt`.
- ☐ Instalar; o SmartScreen pede "Executar assim mesmo" (sem certificado).
- ☐ Num Windows sem Syncthing: a tela "Vamos preparar este computador" aparece; "Instalar com o winget" abre o Prompt com `winget install --id Syncthing.Syncthing`; ao terminar, o passo 2 libera sozinho (até ~4 s).
- ☐ "Ligar agora": em poucos segundos aparece o painel normal; `Get-ScheduledTask -TaskPath "\TwinFolders\"` mostra a tarefa; o Firewall pergunta sobre o syncthing.exe (só Redes privadas).
- ☐ Sair do app e reiniciar o PC: a sincronização volta sozinha.
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

## Atividade e segurança (Fatia 10) — NÃO VERIFICADO

- ☐ Mudar um arquivo numa pasta sincronizada no outro computador: aparece em Atividade com o nome dele, "alterou" e o caminho, em poucos segundos.
- ☐ Apagar um arquivo aqui: aparece como "apagou" com o nome deste computador.
- ☐ Clicar no cartão de um computador filtra só as mudanças dele; Hoje / 7 dias / 30 dias mudam a lista.
- ☐ Aprovar ou desconectar um computador aparece em "Ações neste computador".
- ☐ Preferências → Segurança: o item do disco bate com o estado real do FileVault/BitLocker; com o serviço desligado, os itens do painel ficam como "?".
- ☐ Regras do Tailscale de docs/tailscale-escritorio.md: um notebook do time não alcança outro (`tailscale ping`), mas alcança a central.

## 7. Resultados

| Passo | Resultado | Observações |
|---|---|---|
| 1. Pré-requisitos | NÃO VERIFICADO | |
| 2. Instalar e verificar (tarefa, processo, ping 403/pong, firewall privado) | NÃO VERIFICADO | |
| 3. Segurança (loopback, chave fora de CommandLine/XML, config.xml, roteador) | NÃO VERIFICADO | |
| 4a. Resiliência: kill reinicia em ~1 min | NÃO VERIFICADO | |
| 4b. Desligamento limpo (tarefa termina, sem reinício) | NÃO VERIFICADO | |
| 5. Reinício sem abrir o app (+ arquivo do par chega) | NÃO VERIFICADO | |
| 6. Desinstalar (home e pastas preservados) | NÃO VERIFICADO | |
| Conflitos e versionamento (Fatia 5) | NÃO VERIFICADO | |
| SSH via Tailscale (Fatia 6) | NÃO VERIFICADO | |
