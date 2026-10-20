<#
.SYNOPSIS
  Authenticode-sign files with signtool when signing secrets are configured.
#>
param(
  [Parameter(Mandatory = $true, ValueFromRemainingArguments = $true)]
  [string[]] $Files
)
$ErrorActionPreference = 'Stop'

function Write-Warn([string] $Message) {
  if ($env:GITHUB_ACTIONS) { Write-Output "::warning::$Message" } else { Write-Warning $Message }
}

$haveCert = [bool]$env:WINDOWS_CERTIFICATE
if (-not $haveCert) {
  Write-Warn "Windows signing secrets not set (WINDOWS_CERTIFICATE): leaving unsigned: $($Files -join ', ')"
  exit 0
}

$onPath = Get-Command signtool.exe -ErrorAction SilentlyContinue
if (-not $onPath) { throw 'signtool.exe not found' }
$tmp = Join-Path ([IO.Path]::GetTempPath()) "coralcraft-sign-$PID"
New-Item -ItemType Directory -Force -Path $tmp | Out-Null
try {
  $pfx = Join-Path $tmp 'cert.pfx'
  [IO.File]::WriteAllBytes($pfx, [Convert]::FromBase64String($env:WINDOWS_CERTIFICATE))
  $ts = if ($env:WINDOWS_TIMESTAMP_URL) { $env:WINDOWS_TIMESTAMP_URL } else { 'http://timestamp.digicert.com' }
  $signArgs = @('sign', '/v', '/fd', 'SHA256', '/td', 'SHA256', '/d', 'CoralCraft', '/tr', $ts, '/f', $pfx)
  if ($env:WINDOWS_CERTIFICATE_PASSWORD) { $signArgs += @('/p', $env:WINDOWS_CERTIFICATE_PASSWORD) }
  & $onPath.Source @($signArgs + $Files)
  if ($LASTEXITCODE -ne 0) { throw "signtool failed with exit code $LASTEXITCODE" }
} finally {
  Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
