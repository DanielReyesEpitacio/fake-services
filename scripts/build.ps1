<#
.SYNOPSIS
  Compila Fake Services en Windows 10/11.

.DESCRIPTION
  .\scripts\build.ps1            binario target\release\fakes.exe (con la interfaz web incluida)
  .\scripts\build.ps1 -Paquete   además genera dist\fake-services-<versión>-windows-x86_64.zip

  Requisitos: Rust (rustup, toolchain MSVC con "Desarrollo para el escritorio con C++"),
  Node.js 20+ y pnpm. El resultado no necesita nada de eso.
#>
[CmdletBinding()]
param(
  [switch]$Paquete
)

$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

function Paso($t)  { Write-Host ""; Write-Host "▸ $t" -ForegroundColor White }
function Ok($t)    { Write-Host "  ✓ $t" -ForegroundColor Green }
function Falla($t) { Write-Host "  ✗ $t" -ForegroundColor Red; exit 1 }

function Ejecutar([string]$Exe, [string[]]$Arguments) {
  & $Exe @Arguments
  if ($LASTEXITCODE -ne 0) { Falla "Falló: $Exe $($Arguments -join ' ')" }
}

Paso 'Revisando herramientas'
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
  Falla 'No se encontró Rust. Instálalo desde https://rustup.rs (toolchain MSVC) y vuelve a ejecutar.'
}
Ok "Rust $((rustc --version).Split(' ')[1])"
if (-not (Get-Command node -ErrorAction SilentlyContinue)) {
  Falla 'No se encontró Node.js (20 o superior). Instálalo desde https://nodejs.org.'
}
Ok "Node $(node --version)"
$PnpmCmd = $null
if (Get-Command pnpm -ErrorAction SilentlyContinue) { $PnpmCmd = @('pnpm') }
elseif (Get-Command corepack -ErrorAction SilentlyContinue) { $PnpmCmd = @('corepack', 'pnpm') }
else { Falla 'No se encontró pnpm. Instálalo con: npm install -g pnpm' }

function Pnpm([string[]]$Arguments) {
  if ($PnpmCmd.Count -eq 2) { Ejecutar $PnpmCmd[0] (@($PnpmCmd[1]) + $Arguments) } else { Ejecutar $PnpmCmd[0] $Arguments }
}

Paso 'Compilando la interfaz web'
Push-Location ui
try { Pnpm @('install', '--frozen-lockfile'); Pnpm @('build') } finally { Pop-Location }
Ok 'Interfaz lista en ui\dist'

Paso 'Compilando fakes.exe'
Ejecutar cargo @('build', '--release', '-p', 'fakes-cli')
Ok 'Binario: target\release\fakes.exe'

if ($Paquete) {
  Paso 'Generando el .zip'
  $meta = cargo metadata --no-deps --format-version 1 | ConvertFrom-Json
  $version = ($meta.packages | Where-Object { $_.name -eq 'fakes-cli' }).version
  $name = "fake-services-$version-windows-x86_64"
  $tmp = Join-Path ([IO.Path]::GetTempPath()) $name
  if (Test-Path $tmp) { Remove-Item -Recurse -Force $tmp }
  New-Item -ItemType Directory -Force -Path $tmp | Out-Null
  Copy-Item target\release\fakes.exe $tmp
  Copy-Item scripts\install.ps1 (Join-Path $tmp 'instalar.ps1')
  Copy-Item packaging\windows\instalar.cmd $tmp
  @"
Fake Services $version
Servicios falsos. Integraciones reales.

Instalar:          doble clic en instalar.cmd
                   (o: powershell -ExecutionPolicy Bypass -File instalar.ps1)
Usar sin instalar: doble clic en fakes.exe
Desinstalar:       powershell -ExecutionPolicy Bypass -File instalar.ps1 -Desinstalar

Al ejecutar Fake Services se abre la interfaz en tu navegador (http://127.0.0.1:4010).
Las simulaciones responden en http://127.0.0.1:4000.
"@ | Set-Content -Encoding UTF8 (Join-Path $tmp 'LEEME.txt')
  New-Item -ItemType Directory -Force -Path dist | Out-Null
  $zip = Join-Path $Root "dist\$name.zip"
  if (Test-Path $zip) { Remove-Item -Force $zip }
  Compress-Archive -Path $tmp -DestinationPath $zip
  Remove-Item -Recurse -Force $tmp
  Ok $zip
}

Paso 'Listo'
Write-Host '  Ejecuta:  .\target\release\fakes.exe'
Write-Host '  Instala:  .\scripts\install.ps1'
