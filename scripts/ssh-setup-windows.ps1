# TwinFolders — prepara este PC Windows para o acesso por SSH com o outro computador.
#
# Abra o PowerShell COMO ADMINISTRADOR (botão direito → Executar como administrador):
#   powershell -ExecutionPolicy Bypass -File .\scripts\ssh-setup-windows.ps1
#       cria a chave deste PC e mostra a parte pública
#   powershell -ExecutionPolicy Bypass -File .\scripts\ssh-setup-windows.ps1 -OtherKey "ssh-ed25519 AAAA… macbook"
#       + instala o servidor OpenSSH, autoriza a chave do outro computador,
#         desliga login por senha e libera a porta 22 só para o Tailscale
#   powershell -ExecutionPolicy Bypass -File .\scripts\ssh-setup-windows.ps1 -Undo
#       restaura o sshd_config anterior e fecha a porta
param(
  [string]$OtherKey = "",
  [switch]$Undo
)
$ErrorActionPreference = "Stop"

function Say($t)  { Write-Host "`n$t" -ForegroundColor White }
function Info($t) { Write-Host "  $t" }
function Die($t)  { Write-Host "`nErro: $t" -ForegroundColor Red; exit 1 }

# The Tailscale adapter. Rules are bound to it, not only to Tailscale's
# address range: 100.64.0.0/10 is also used by some internet providers
# (CGNAT), so an address match alone could let the provider's network in.
function Get-TailscaleAlias {
  $a = Get-NetAdapter -ErrorAction SilentlyContinue | Where-Object { $_.InterfaceDescription -like "Tailscale*" -or $_.Name -like "Tailscale*" } | Select-Object -First 1
  if (-not $a) { Die "o Tailscale não está instalado ou conectado neste PC. Instale, entre na conta e rode de novo." }
  $a.Name
}

$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
$sshDir   = Join-Path $env:USERPROFILE ".ssh"
$key      = Join-Path $sshDir "id_ed25519"
$conf     = "C:\ProgramData\ssh\sshd_config"
$backup   = "C:\ProgramData\ssh\sshd_config.twinfolders-backup"
$rule     = "TwinFolders-SSH-Tailscale"

if ($Undo) {
  if (-not $isAdmin) { Die "abra o PowerShell como administrador." }
  Say "Desfazendo a configuração do TwinFolders"
  if (Test-Path $backup) { Copy-Item $backup $conf -Force; Remove-Item $backup; Info "sshd_config anterior restaurado." }
  Get-NetFirewallRule -Name $rule -ErrorAction SilentlyContinue | Remove-NetFirewallRule
  if (Get-Service sshd -ErrorAction SilentlyContinue) {
    Stop-Service sshd -ErrorAction SilentlyContinue
    Set-Service sshd -StartupType Manual
  }
  Info "Pronto. O servidor SSH ficou desligado; para religar com segurança, rode o script de novo."
  exit 0
}

Say "1/5 · Chave deste PC"
New-Item -ItemType Directory -Force -Path $sshDir | Out-Null
if (Test-Path $key) {
  Info "Já existe: $key"
} else {
  Info "Criando uma chave ed25519. Escolha uma frase-senha (recomendado) ou deixe em branco."
  ssh-keygen -t ed25519 -a 100 -C "twinfolders-$env:COMPUTERNAME" -f $key
  if ($LASTEXITCODE -ne 0) { Die "ssh-keygen falhou." }
}
Info "Copie a linha abaixo e use no OUTRO computador (no script de lá):"
Write-Host ""
Get-Content "$key.pub"

if (-not $OtherKey) {
  Say "Próximo passo"
  Info "Rode de novo, como administrador, passando a chave pública do outro computador:"
  Info "  -OtherKey `"ssh-ed25519 AAAA… nome`""
  exit 0
}
if (-not $isAdmin) { Die "para os próximos passos, abra o PowerShell como administrador." }
if ($OtherKey -notmatch '^(ssh-ed25519|sk-ssh-ed25519@openssh\.com) [A-Za-z0-9+/=]+( .*)?$' -or $OtherKey.Contains("`n")) {
  Die "isso não parece uma chave pública ed25519 (uma linha começando com ssh-ed25519)."
}

# Order matters: sshd only starts AFTER the firewall is limited to Tailscale
# and password login is off, so it is never exposed with weaker settings.
Say "2/5 · Servidor OpenSSH (instalado, ainda desligado)"
$cap = Get-WindowsCapability -Online -Name "OpenSSH.Server*" | Select-Object -First 1
if ($cap.State -ne "Installed") {
  Info "Instalando o OpenSSH Server (recurso do Windows)…"
  Add-WindowsCapability -Online -Name $cap.Name | Out-Null
}
Stop-Service sshd -ErrorAction SilentlyContinue

try {
  Say "3/5 · Firewall: porta 22 só pela rede do Tailscale"
  Get-NetFirewallRule -Name "OpenSSH-Server-In-TCP" -ErrorAction SilentlyContinue | Disable-NetFirewallRule
  Get-NetFirewallRule -Name $rule -ErrorAction SilentlyContinue | Remove-NetFirewallRule
  New-NetFirewallRule -Name $rule -DisplayName "TwinFolders SSH (Tailscale)" -Direction Inbound -Protocol TCP -LocalPort 22 `
    -InterfaceAlias (Get-TailscaleAlias) -RemoteAddress "100.64.0.0/10", "fd7a:115c:a1e0::/48" -Action Allow | Out-Null
  Info "A regra padrão do OpenSSH (qualquer rede) foi desativada; só endereços do Tailscale entram."

  Say "4/5 · Autorizando a chave do outro computador"
  # Contas administradoras usam um arquivo próprio, com ACL restrita
  # (exigência do OpenSSH para Windows).
  $userIsAdmin = $null -ne (Get-LocalGroupMember -SID "S-1-5-32-544" -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -eq "$env:USERDOMAIN\$env:USERNAME" })
  if ($userIsAdmin) {
    $auth = "C:\ProgramData\ssh\administrators_authorized_keys"
  } else {
    $auth = Join-Path $sshDir "authorized_keys"
  }
  if (-not (Test-Path $auth)) { New-Item -ItemType File -Path $auth -Force | Out-Null }
  if ((Get-Content $auth -ErrorAction SilentlyContinue) -contains $OtherKey) {
    Info "Essa chave já estava autorizada."
  } else {
    Add-Content -Path $auth -Value $OtherKey -Encoding ascii
    Info "Autorizada em $auth"
  }
  if ($userIsAdmin) {
    icacls.exe $auth /inheritance:r /grant "*S-1-5-32-544:F" /grant "SYSTEM:F" | Out-Null
  }

  Say "5/5 · Só chaves e só a sua conta"
  if (-not (Test-Path $conf)) {
    # Fresh install: sshd writes its default config on first start; create it
    # from the shipped default instead so we never start it unhardened.
    Copy-Item "$env:WINDIR\System32\OpenSSH\sshd_config_default" $conf
  }
  if (-not (Test-Path $backup)) { Copy-Item $conf $backup }
  $me = $env:USERNAME.ToLower()
  $lines = Get-Content $conf | Where-Object { $_ -notmatch '^\s*#?\s*(PasswordAuthentication|PubkeyAuthentication|PermitEmptyPasswords|AllowUsers)\b' }
  $new = @("# TwinFolders: só chaves, só a conta $me", "PasswordAuthentication no", "PubkeyAuthentication yes",
           "PermitEmptyPasswords no", "AllowUsers $me") + $lines
  Set-Content -Path $conf -Value $new -Encoding ascii
  & "$env:WINDIR\System32\OpenSSH\sshd.exe" -t
  if ($LASTEXITCODE -ne 0) { throw "a configuração do SSH ficou inválida" }

  Set-Service sshd -StartupType Automatic
  Start-Service sshd
  Info "Servidor ligado só agora, já protegido: senha desligada, só \"$me\", só pelo Tailscale."
} catch {
  Stop-Service sshd -ErrorAction SilentlyContinue
  Set-Service sshd -StartupType Manual -ErrorAction SilentlyContinue
  if (Test-Path $backup) { Copy-Item $backup $conf -Force }
  Die "$($_.Exception.Message). O servidor SSH foi deixado desligado e a configuração anterior restaurada."
}

Say "Pronto"
Info "No outro computador, o TwinFolders abre um terminal aqui usando o usuário: $env:USERNAME"
Info "Para desfazer: execute este script com -Undo"
