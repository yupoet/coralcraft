<#
.SYNOPSIS
  Build, sign and package CoralCraft for Windows (x64 or arm64) on a Windows runner.

.DESCRIPTION
  Produces, in $env:DIST (default: dist/release):
    coralcraft-<version>-windows-<arch>.msi            per-machine installer (WiX v5, when installed)
    coralcraft-<version>-windows-<arch>-portable.zip   coralcraft.exe + coralcraft-cli.exe + portable.txt
#>
param(
  [ValidateSet('x64', 'arm64')] [string] $Arch = 'x64',
  [switch] $SkipBuild
)
$ErrorActionPreference = 'Stop'
$Root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path

function Invoke-Native([string] $What, [scriptblock] $Block) {
  Write-Output "==> $What"
  & $Block
  if ($LASTEXITCODE -ne 0) { throw "$What failed with exit code $LASTEXITCODE" }
}

$Version = $env:CORALCRAFT_VERSION
if (-not $Version) {
  $inPkg = $false
  foreach ($line in Get-Content (Join-Path $Root 'Cargo.toml')) {
    if ($line -match '^\s*\[') { $inPkg = ($line.Trim() -eq '[workspace.package]'); continue }
    if ($inPkg -and $line -match '^\s*version\s*=\s*"([^"]+)"') { $Version = $Matches[1]; break }
  }
}
if (-not $Version) { throw 'could not read [workspace.package] version from Cargo.toml' }
$MsiVersion = ($Version -split '-')[0]

$Target = switch ($Arch) { 'x64' { 'x86_64-pc-windows-msvc' } 'arm64' { 'aarch64-pc-windows-msvc' } }
$Dist = if ($env:DIST) { $env:DIST } else { Join-Path $Root 'dist\release' }
$TargetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $Root 'target' }
New-Item -ItemType Directory -Force -Path $Dist | Out-Null

if (-not $SkipBuild) {
  $flagVar = 'CARGO_TARGET_' + ($Target.ToUpper() -replace '-', '_') + '_RUSTFLAGS'
  [Environment]::SetEnvironmentVariable($flagVar, '-C target-feature=+crt-static')
  Invoke-Native "cargo build ($Target)" { cargo build --release --locked -p coralcraft -p coralcraft-cli --target $Target }
}

$Bin = Join-Path $TargetDir "$Target\release"

function Get-PeHeader([string] $Path) {
  $bytes = [System.IO.File]::ReadAllBytes($Path)
  $pe = [BitConverter]::ToInt32($bytes, 0x3C)
  return @{ Machine = [BitConverter]::ToUInt16($bytes, $pe + 4); Subsystem = [BitConverter]::ToUInt16($bytes, $pe + 0x5C) }
}
$Machine = switch ($Arch) { 'x64' { 0x8664 } 'arm64' { 0xAA64 } }
foreach ($check in @(@('coralcraft.exe', 2), @('coralcraft-cli.exe', 3))) {
  $h = Get-PeHeader (Join-Path $Bin $check[0])
  if ($h.Machine -ne $Machine) { throw "$($check[0]) is for machine 0x$('{0:X}' -f $h.Machine), expected 0x$('{0:X}' -f $Machine) ($Arch)" }
  if ($h.Subsystem -ne $check[1]) { throw "$($check[0]) has PE subsystem $($h.Subsystem), expected $($check[1])" }
  Write-Output "ok $($check[0]): $Arch, PE subsystem $($h.Subsystem)"
}

$IconsDir = Join-Path $TargetDir 'package-icons'
Invoke-Native 'generate icons' { python (Join-Path $Root 'packaging\icons.py') $IconsDir }

$Stage = Join-Path $TargetDir "windows-package\$Arch"
Remove-Item -Recurse -Force $Stage -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $Stage | Out-Null
Copy-Item (Join-Path $Bin 'coralcraft.exe'), (Join-Path $Bin 'coralcraft-cli.exe') $Stage

& (Join-Path $PSScriptRoot 'sign.ps1') (Join-Path $Stage 'coralcraft.exe') (Join-Path $Stage 'coralcraft-cli.exe')

$Docs = Join-Path $Stage 'docs'
New-Item -ItemType Directory -Force -Path $Docs | Out-Null
foreach ($f in 'README.md', 'ROADMAP.md', 'ASSETS.md', 'LICENSE-MIT', 'LICENSE-APACHE') {
  $p = Join-Path $Root $f
  if (Test-Path $p) { Copy-Item $p $Docs }
}

if (Get-Command wix -ErrorAction SilentlyContinue) {
  Add-Type -AssemblyName System.Drawing
  $Banner = Join-Path $Stage 'installer-banner.bmp'
  $Dialog = Join-Path $Stage 'installer-dialog.bmp'
  foreach ($spec in @(@($Banner, 493, 58), @($Dialog, 493, 312))) {
    $bmp = New-Object System.Drawing.Bitmap $spec[1], $spec[2], ([System.Drawing.Imaging.PixelFormat]::Format24bppRgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.Clear([System.Drawing.Color]::White)
    $g.Dispose()
    $bmp.Save($spec[0], [System.Drawing.Imaging.ImageFormat]::Bmp)
    $bmp.Dispose()
  }
  $Msi = Join-Path $Dist "coralcraft-$Version-windows-$Arch.msi"
  Invoke-Native 'wix build' {
    wix build (Join-Path $PSScriptRoot 'coralcraft.wxs') -arch $Arch `
      -ext WixToolset.UI.wixext -ext WixToolset.Util.wixext `
      -culture en-US -loc (Join-Path $PSScriptRoot 'coralcraft.en-us.wxl') `
      -d "Version=$MsiVersion" -d "DisplayVersion=$Version" -d "BinDir=$Stage" -d "IconPath=$(Join-Path $IconsDir 'coralcraft.ico')" `
      -d "DocsDir=$Docs" -d "BannerBmp=$Banner" -d "DialogBmp=$Dialog" `
      -o $Msi
  }
  Remove-Item -Force -ErrorAction SilentlyContinue ([IO.Path]::ChangeExtension($Msi, '.wixpdb'))
  & (Join-Path $PSScriptRoot 'sign.ps1') $Msi
}

$Portable = Join-Path $TargetDir "windows-package\coralcraft-$Version-windows-$Arch-portable"
Remove-Item -Recurse -Force $Portable -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $Portable | Out-Null
Copy-Item (Join-Path $Stage '*.exe') $Portable
Copy-Item -Recurse (Join-Path $Docs '*') $Portable
Copy-Item (Join-Path $PSScriptRoot 'portable.txt') $Portable
$Zip = Join-Path $Dist "coralcraft-$Version-windows-$Arch-portable.zip"
Remove-Item -Force $Zip -ErrorAction SilentlyContinue
Compress-Archive -Path $Portable -DestinationPath $Zip

$HostArch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString().ToLowerInvariant()
if ($Arch -ne 'arm64' -or $HostArch -eq 'arm64') {
  Invoke-Native 'coralcraft-cli --version' { & (Join-Path $Stage 'coralcraft-cli.exe') --version }
}
Get-ChildItem $Dist -Filter "coralcraft-$Version-windows-$Arch*" | Format-Table Name, Length
