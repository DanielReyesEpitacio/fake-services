<#
.SYNOPSIS
  Instala o actualiza Fake Services para tu usuario en Windows 10/11 (sin permisos de administrador).

.DESCRIPTION
  .\instalar.ps1                instala, actualiza o reinstala (siempre el mismo comando)
  .\instalar.ps1 -Desinstalar   quita lo instalado (tus datos se conservan)

  Si hay una versión anterior: la detiene, la reemplaza, limpia restos de
  instalaciones viejas (rutas antiguas, app de escritorio) y, si estaba en
  ejecución, inicia la nueva.

  Funciona desde el .zip descargado (usa fakes.exe incluido) o desde el
  repositorio (compila primero con scripts\build.ps1).
#>
[CmdletBinding()]
param(
  [switch]$Desinstalar
)

$ErrorActionPreference = 'Stop'
$InstallDir = Join-Path $env:LOCALAPPDATA 'Programs\FakeServices'
$Exe = Join-Path $InstallDir 'fakes.exe'
$Shortcut = Join-Path ([Environment]::GetFolderPath('Programs')) 'Fake Services.lnk'
$RunKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$UninstallKeys = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*'
# Ubicación usada por versiones anteriores del instalador.
$LegacyBinDir = Join-Path $env:LOCALAPPDATA 'FakeServices\bin'
$LogDir = Join-Path $env:LOCALAPPDATA 'FakeServices\logs'

function Paso($t)  { Write-Host ""; Write-Host "▸ $t" -ForegroundColor White }
function Ok($t)    { Write-Host "  ✓ $t" -ForegroundColor Green }
function Aviso($t) { Write-Host "  ! $t" -ForegroundColor Yellow }

function Get-UserPath { [Environment]::GetEnvironmentVariable('Path', 'User') }
function Set-UserPath($v) { [Environment]::SetEnvironmentVariable('Path', $v, 'User') }
function Remove-FromPath($dir) {
  $parts = (Get-UserPath) -split ';' | Where-Object { $_ -and ($_.TrimEnd('\') -ne $dir.TrimEnd('\')) }
  Set-UserPath ($parts -join ';')
}

function Get-Version($path) {
  if (-not (Test-Path $path)) { return $null }
  try { ((& $path --version) -split ' ')[1] } catch { $null }
}

# Procesos de Fake Services instalados (no toca compilaciones de desarrollo).
function Get-InstalledProcesses {
  $known = @($Exe, (Join-Path $LegacyBinDir 'fakes.exe'))
  Get-Process -Name fakes, 'fake-services', 'Fake Services' -ErrorAction SilentlyContinue |
    Where-Object { $_.Path -and (($known -contains $_.Path) -or ($_.Path -like '*\Fake Services\*')) }
}

function Stop-Running($tool) {
  $procs = @(Get-InstalledProcesses)
  if ($procs.Count -eq 0) { return $false }
  Aviso 'Fake Services está en ejecución; se detendrá para continuar.'
  if ($tool -and (Test-Path $tool)) {
    try { & $tool detener | Out-Null } catch { }
  }
  for ($i = 0; $i -lt 40; $i++) {
    if (@(Get-InstalledProcesses).Count -eq 0) { break }
    Start-Sleep -Milliseconds 300
  }
  # Versiones sin «fakes detener».
  Get-InstalledProcesses | Stop-Process -Force -ErrorAction SilentlyContinue
  Start-Sleep -Milliseconds 500
  Ok 'Instancia anterior detenida'
  return $true
}

# Desinstala la app de escritorio de versiones anteriores (instalador NSIS/MSI).
function Remove-LegacyDesktopApp {
  $entries = Get-ItemProperty $UninstallKeys -ErrorAction SilentlyContinue |
    Where-Object { $_.DisplayName -eq 'Fake Services' -and $_.UninstallString }
  foreach ($e in $entries) {
    $cmd = $e.UninstallString.Trim('"')
    try {
      if ($cmd -match 'msiexec') {
        Start-Process msiexec.exe -ArgumentList "/x $($e.PSChildName) /qn" -Wait
      } else {
        Start-Process -FilePath $cmd -ArgumentList '/S' -Wait
      }
      Ok 'Se desinstaló la app de escritorio anterior'
    } catch {
      Aviso "No se pudo desinstalar la app de escritorio anterior: $($_.Exception.Message)"
    }
  }
}

function Remove-LegacyCli {
  if (Test-Path $LegacyBinDir) {
    Remove-Item -Recurse -Force $LegacyBinDir -ErrorAction SilentlyContinue
    Ok 'Se quitó la ubicación anterior del programa'
  }
  Remove-FromPath $LegacyBinDir
}

# ---------------------------------------------------------------------------
if ($Desinstalar) {
  Paso 'Desinstalando Fake Services'
  Stop-Running $Exe | Out-Null
  if (Test-Path $InstallDir) { Remove-Item -Recurse -Force $InstallDir }
  if (Test-Path $Shortcut) { Remove-Item -Force $Shortcut }
  Remove-ItemProperty -Path $RunKey -Name 'FakeServices' -ErrorAction SilentlyContinue
  Remove-FromPath $InstallDir
  Remove-LegacyCli
  Remove-LegacyDesktopApp
  Ok 'Se quitaron el programa, el acceso del menú Inicio, el inicio automático y la entrada del PATH'
  Write-Host ''
  Write-Host '  Tus espacios de trabajo e historial siguen en %APPDATA%\FakeServices.'
  exit 0
}

# ---------------------------------------------------------------------------
# ¿Paquete descargado (fakes.exe junto al script) o repositorio?
$source = Join-Path $PSScriptRoot 'fakes.exe'
if (-not (Test-Path $source)) {
  $root = Split-Path -Parent $PSScriptRoot
  & (Join-Path $PSScriptRoot 'build.ps1')
  if (-not $?) { exit 1 }
  $source = Join-Path $root 'target\release\fakes.exe'
}

$newVersion = Get-Version $source
$oldVersion = Get-Version $Exe
if (-not $oldVersion) { $oldVersion = Get-Version (Join-Path $LegacyBinDir 'fakes.exe') }

if (-not $oldVersion) { Paso "Instalando Fake Services $newVersion" }
elseif ($oldVersion -eq $newVersion) { Paso "Reinstalando Fake Services $newVersion" }
else { Paso "Actualizando Fake Services de $oldVersion a $newVersion" }

$wasRunning = Stop-Running $source
Remove-LegacyCli
Remove-LegacyDesktopApp

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
Copy-Item -Force $source $Exe
Ok "Programa: $Exe"

$userPath = Get-UserPath
if (-not (($userPath -split ';') -contains $InstallDir)) {
  Set-UserPath ((@($userPath, $InstallDir) | Where-Object { $_ }) -join ';')
  Ok 'Agregado al PATH del usuario (abre una terminal nueva para usar «fakes»)'
}

$shell = New-Object -ComObject WScript.Shell
$lnk = $shell.CreateShortcut($Shortcut)
$lnk.TargetPath = $Exe
$lnk.WorkingDirectory = $InstallDir
$lnk.Description = 'Fake Services: simulador local de servicios externos'
$lnk.IconLocation = "$Exe,0"
$lnk.Save()
Ok 'Acceso en el menú Inicio: «Fake Services»'

# El inicio automático siempre apunta al programa instalado.
if (Get-ItemProperty -Path $RunKey -Name 'FakeServices' -ErrorAction SilentlyContinue) {
  $cmd = "powershell.exe -NoProfile -WindowStyle Hidden -Command `"& '$($Exe.Replace("'", "''"))' serve --sin-navegador`""
  Set-ItemProperty -Path $RunKey -Name 'FakeServices' -Value $cmd
  Ok 'Inicio automático actualizado'
}

if ($wasRunning) {
  New-Item -ItemType Directory -Force -Path $LogDir | Out-Null
  Start-Process -FilePath $Exe -ArgumentList 'serve', '--sin-navegador' -WindowStyle Hidden `
    -RedirectStandardOutput (Join-Path $LogDir 'fakes.log') -RedirectStandardError (Join-Path $LogDir 'fakes-error.log')
  Ok "Fake Services $newVersion se reinició en segundo plano (detenlo con: fakes detener)"
}

Paso 'Listo'
Write-Host '  Abre «Fake Services» desde el menú Inicio o ejecuta:  fakes'
Write-Host '  La interfaz se abre en tu navegador: http://127.0.0.1:4010'
Write-Host '  Tus espacios de trabajo y configuración se conservan entre versiones.'
