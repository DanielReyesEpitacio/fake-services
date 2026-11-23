<#
.SYNOPSIS
  Instala Fake Services en Windows 10/11 (compila primero si hace falta).

.DESCRIPTION
  .\scripts\install.ps1               CLI fakes.exe + app de escritorio
  .\scripts\install.ps1 -SoloCli      solo el CLI fakes.exe
  .\scripts\install.ps1 -Desinstalar  quita el CLI (la app se desinstala desde
                                      "Aplicaciones instaladas" de Windows)

  El CLI se copia a %LOCALAPPDATA%\FakeServices\bin y se agrega al PATH del
  usuario. La app de escritorio se instala con su instalador NSIS, solo para
  el usuario actual (sin permisos de administrador).
#>
[CmdletBinding()]
param(
  [switch]$SoloCli,
  [switch]$Desinstalar
)

$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

$BinDir = Join-Path $env:LOCALAPPDATA 'FakeServices\bin'

function Paso($t)  { Write-Host ""; Write-Host "▸ $t" -ForegroundColor White }
function Ok($t)    { Write-Host "  ✓ $t" -ForegroundColor Green }
function Aviso($t) { Write-Host "  ! $t" -ForegroundColor Yellow }

function Get-UserPath { [Environment]::GetEnvironmentVariable('Path', 'User') }
function Set-UserPath($v) { [Environment]::SetEnvironmentVariable('Path', $v, 'User') }

# ---------------------------------------------------------------------------
if ($Desinstalar) {
  Paso 'Desinstalando el CLI'
  if (Test-Path $BinDir) { Remove-Item -Recurse -Force $BinDir; Ok "Eliminado $BinDir" }
  $parts = (Get-UserPath) -split ';' | Where-Object { $_ -and ($_ -ne $BinDir) }
  Set-UserPath ($parts -join ';')
  Ok 'PATH del usuario actualizado'
  Write-Host ''
  Write-Host '  La app de escritorio se desinstala desde Configuración > Aplicaciones > Fake Services.'
  Write-Host '  Tus espacios e historial se conservan en %APPDATA%\FakeServices.'
  exit 0
}

# ---------------------------------------------------------------------------
# Siempre se compila: cargo y pnpm solo rehacen lo que cambió, y así nunca
# se instala un binario viejo.
$cli = Join-Path $Root 'target\release\fakes.exe'
if ($SoloCli) { & "$PSScriptRoot\build.ps1" -SoloCli } else { & "$PSScriptRoot\build.ps1" }
if (-not $?) { exit 1 }
$nsis = Get-ChildItem -ErrorAction SilentlyContinue "$Root\target\release\bundle\nsis\*.exe" |
  Sort-Object LastWriteTime | Select-Object -Last 1

# ---------------------------------------------------------------------------
Paso 'Instalando el CLI'
New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
Copy-Item -Force $cli (Join-Path $BinDir 'fakes.exe')
Ok "fakes.exe → $BinDir"

$userPath = Get-UserPath
if (-not (($userPath -split ';') -contains $BinDir)) {
  Set-UserPath ((@($userPath, $BinDir) | Where-Object { $_ }) -join ';')
  Ok 'Agregado al PATH del usuario (abre una terminal nueva para usarlo)'
}

# ---------------------------------------------------------------------------
if (-not $SoloCli) {
  Paso 'Instalando la app de escritorio'
  if ($nsis) {
    Write-Host "  Ejecutando $($nsis.Name)…"
    $p = Start-Process -FilePath $nsis.FullName -ArgumentList '/S' -Wait -PassThru
    if ($p.ExitCode -eq 0) { Ok 'App instalada (búscala como «Fake Services» en el menú Inicio)' }
    else { Aviso "El instalador terminó con código $($p.ExitCode)" }
  } else {
    Aviso 'No se encontró el instalador NSIS; solo quedó instalado el CLI.'
  }
}

# ---------------------------------------------------------------------------
Paso 'Listo'
Write-Host '  Ejecuta:  fakes serve'
Write-Host '  Interfaz: http://127.0.0.1:4010   Simulaciones: http://127.0.0.1:4000'
