<#
.SYNOPSIS
  Compila Fake Services en Windows 10/11.

.DESCRIPTION
  .\scripts\build.ps1             CLI + instaladores de escritorio (NSIS .exe y MSI)
  .\scripts\build.ps1 -SoloCli    solo el binario fakes.exe

  Requisitos: Rust (rustup, toolchain MSVC), Visual Studio Build Tools con
  "Desarrollo para el escritorio con C++", Node.js 20+ y pnpm.
  WebView2 ya viene incluido en Windows 10/11.
#>
[CmdletBinding()]
param(
  [switch]$SoloCli
)

$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

function Paso($t)  { Write-Host ""; Write-Host "▸ $t" -ForegroundColor White }
function Ok($t)    { Write-Host "  ✓ $t" -ForegroundColor Green }
function Aviso($t) { Write-Host "  ! $t" -ForegroundColor Yellow }
function Falla($t) { Write-Host "  ✗ $t" -ForegroundColor Red; exit 1 }

function Ejecutar([string]$Exe, [string[]]$Arguments) {
  & $Exe @Arguments
  if ($LASTEXITCODE -ne 0) { Falla "Falló: $Exe $($Arguments -join ' ')" }
}

# ---------------------------------------------------------------------------
Paso 'Revisando herramientas'

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
  Falla 'No se encontró Rust. Instálalo desde https://rustup.rs (toolchain MSVC) y vuelve a ejecutar.'
}
Ok "Rust $((rustc --version).Split(' ')[1])"

if (-not (Get-Command node -ErrorAction SilentlyContinue)) {
  Falla 'No se encontró Node.js (20 o superior). Instálalo desde https://nodejs.org.'
}
Ok "Node $(node --version)"

$Pnpm = $null
if (Get-Command pnpm -ErrorAction SilentlyContinue) { $Pnpm = @('pnpm') }
elseif (Get-Command corepack -ErrorAction SilentlyContinue) { $Pnpm = @('corepack', 'pnpm') }
else { Falla 'No se encontró pnpm. Instálalo con: npm install -g pnpm' }

function Pnpm([string[]]$Arguments) {
  if ($Pnpm.Count -eq 2) { Ejecutar $Pnpm[0] (@($Pnpm[1]) + $Arguments) } else { Ejecutar $Pnpm[0] $Arguments }
}

if (-not $SoloCli) {
  $tauri = $null
  try { $tauri = & cargo tauri --version 2>$null } catch { $tauri = $null }
  if (-not $tauri -or $LASTEXITCODE -ne 0) {
    Aviso 'Falta tauri-cli (se instala una sola vez, tarda unos minutos)'
    Ejecutar cargo @('install', 'tauri-cli', '--version', '^2', '--locked')
    $tauri = & cargo tauri --version
  }
  Ok $tauri
}

# ---------------------------------------------------------------------------
Paso 'Compilando la interfaz'
Push-Location ui
try {
  Pnpm @('install', '--frozen-lockfile')
  Pnpm @('build')
} finally { Pop-Location }
Ok 'Interfaz lista en ui\dist'

Paso 'Compilando el CLI (fakes.exe)'
Ejecutar cargo @('build', '--release', '-p', 'fakes-cli')
Ok 'Binario: target\release\fakes.exe'

if (-not $SoloCli) {
  Paso 'Generando instaladores de escritorio'
  Push-Location apps\desktop
  try { Ejecutar cargo @('tauri', 'build') } finally { Pop-Location }
}

# ---------------------------------------------------------------------------
Paso 'Listo'
Write-Host "  CLI:        $Root\target\release\fakes.exe"
if (-not $SoloCli) {
  Get-ChildItem -ErrorAction SilentlyContinue `
    "$Root\target\release\bundle\nsis\*.exe", "$Root\target\release\bundle\msi\*.msi" |
    ForEach-Object { Write-Host "  Instalador: $($_.FullName)" }
}
Write-Host ''
Write-Host '  Para instalar: .\scripts\install.ps1'
