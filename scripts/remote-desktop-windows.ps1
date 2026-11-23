# TwinFolders — liga a Área de Trabalho Remota neste PC, aceitando conexões
# SÓ pela rede privada do Tailscale, para a central do escritório ver a tela.
#
# Precisa de Windows Pro, Enterprise ou Education (o Windows Home não recebe
# Área de Trabalho Remota). Abra o PowerShell COMO ADMINISTRADOR:
#   powershell -ExecutionPolicy Bypass -File .\scripts\remote-desktop-windows.ps1
#   powershell -ExecutionPolicy Bypass -File .\scripts\remote-desktop-windows.ps1 -Undo
#
# O que faz, nesta ordem (nada fica exposto em nenhum momento):
#   1. firewall: regra própria para a porta 3389 só de endereços do Tailscale;
#      as regras padrão de Área de Trabalho Remota (qualquer rede) ficam desligadas
#   2. exige autenticação antes da conexão (NLA)
#   3. só então liga a Área de Trabalho Remota
param([switch]$Undo)
$ErrorActionPreference = "Stop"

function Say($t)  { Write-Host "`n$t" -ForegroundColor White }
function Info($t) { Write-Host "  $t" }
function Die($t)  { Write-Host "`nErro: $t" -ForegroundColor Red; exit 1 }

$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) { Die "abra o PowerShell como administrador (botão direito → Executar como administrador)." }

$ts   = "HKLM:\System\CurrentControlSet\Control\Terminal Server"
$rdp  = "HKLM:\System\CurrentControlSet\Control\Terminal Server\WinStations\RDP-Tcp"
$rule = "TwinFolders-RDP-Tailscale"

if ($Undo) {
  Say "Desligando a Área de Trabalho Remota"
  Set-ItemProperty -Path $ts -Name fDenyTSConnections -Value 1
  Get-NetFirewallRule -Name $rule -ErrorAction SilentlyContinue | Remove-NetFirewallRule
  Info "Pronto. Ninguém consegue mais ver a tela deste PC remotamente."
  exit 0
}

$os = (Get-CimInstance Win32_OperatingSystem).Caption
if ($os -match "Home") {
  Die "este PC tem $os. O Windows Home não recebe Área de Trabalho Remota. Use o terminal (SSH) pelo TwinFolders, ou atualize para o Windows Pro."
}

try {
  Say "1/3 · Firewall: porta 3389 só pela rede do Tailscale"
  Get-NetFirewallRule -DisplayGroup "Remote Desktop" -ErrorAction SilentlyContinue | Disable-NetFirewallRule
  Get-NetFirewallRule -DisplayGroup "Área de Trabalho Remota" -ErrorAction SilentlyContinue | Disable-NetFirewallRule
  Get-NetFirewallRule -Name $rule -ErrorAction SilentlyContinue | Remove-NetFirewallRule
  New-NetFirewallRule -Name $rule -DisplayName "TwinFolders Área de Trabalho Remota (Tailscale)" -Direction Inbound -Protocol TCP -LocalPort 3389 `
    -RemoteAddress "100.64.0.0/10", "fd7a:115c:a1e0::/48" -Action Allow | Out-Null
  Info "Só endereços do Tailscale entram; as regras padrão (qualquer rede) foram desligadas."

  Say "2/3 · Pedir senha antes de conectar (NLA)"
  Set-ItemProperty -Path $rdp -Name UserAuthentication -Value 1

  Say "3/3 · Ligando a Área de Trabalho Remota"
  Set-ItemProperty -Path $ts -Name fDenyTSConnections -Value 0
} catch {
  Set-ItemProperty -Path $ts -Name fDenyTSConnections -Value 1 -ErrorAction SilentlyContinue
  Die "$($_.Exception.Message). A Área de Trabalho Remota foi deixada desligada."
}

Say "Pronto"
Info "Na central, no TwinFolders: Computadores → ⋯ deste PC → Ver a tela."
Info "O usuário é a conta deste PC: $env:USERNAME (com a senha dela)."
Info "Para desfazer: execute este script com -Undo"
