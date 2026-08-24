<#
.SYNOPSIS
  Conditionally regenerate Flutter-Rust Bridge bindings on Windows.
.DESCRIPTION
  Uses the same explicit bridge boundary as tool/frb_build.sh and stores the
  content fingerprint in .dart_tool/portalis. Backend-only Rust changes do not
  trigger code generation.
#>

[CmdletBinding()]
param(
  [switch]$Force
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$toolRoot = Split-Path -Parent $MyInvocation.MyCommand.Definition
$repoRoot = (Resolve-Path (Join-Path $toolRoot '..')).Path
$crateRoot = Join-Path $repoRoot 'rust/backend'
$stampDir = Join-Path $repoRoot '.dart_tool/portalis'
$stampPath = Join-Path $stampDir 'frb-inputs.sha256'
Set-Location $repoRoot

$inputs = @(
  (Join-Path $crateRoot 'src/bridge.rs'),
  (Join-Path $crateRoot 'src/portalis_api.rs'),
  (Join-Path $crateRoot 'src/nexus/device.rs'),
  (Join-Path $crateRoot 'src/nexus/settings.rs'),
  (Join-Path $crateRoot 'Cargo.toml'),
  (Join-Path $crateRoot 'Cargo.lock'),
  (Join-Path $repoRoot 'pubspec.yaml'),
  (Join-Path $toolRoot 'frb_generate.ps1')
)
$outputs = @(
  (Join-Path $crateRoot 'src/api.rs'),
  (Join-Path $repoRoot 'lib/nexus/bridge/bridge.dart'),
  (Join-Path $repoRoot 'lib/nexus/bridge/portalis_api.dart'),
  (Join-Path $repoRoot 'lib/nexus/bridge/frb_generated.dart'),
  (Join-Path $repoRoot 'lib/nexus/bridge/frb_generated.io.dart'),
  (Join-Path $repoRoot 'lib/nexus/bridge/frb_generated.web.dart'),
  (Join-Path $repoRoot 'lib/nexus/bridge/nexus/device.dart'),
  (Join-Path $repoRoot 'lib/nexus/bridge/nexus/settings.dart')
)

function Get-GeneratorVersion {
  if ($null -eq (Get-Command flutter_rust_bridge_codegen -ErrorAction SilentlyContinue)) {
    return 'missing'
  }
  return ((& flutter_rust_bridge_codegen --version 2>$null) -join ' ').Trim()
}

function Get-Fingerprint {
  $lines = [System.Collections.Generic.List[string]]::new()
  $lines.Add("generator=$(Get-GeneratorVersion)")
  foreach ($input in $inputs) {
    $relative = $input.Substring($repoRoot.Length).TrimStart('\', '/')
    if (Test-Path -LiteralPath $input -PathType Leaf) {
      $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $input).Hash
      $lines.Add("$relative=$hash")
    } else {
      $lines.Add("$relative=MISSING")
    }
  }
  $sha = [System.Security.Cryptography.SHA256]::Create()
  try {
    $bytes = [System.Text.Encoding]::UTF8.GetBytes(($lines -join "`n"))
    return (($sha.ComputeHash($bytes) | ForEach-Object { $_.ToString('x2') }) -join '')
  } finally {
    $sha.Dispose()
  }
}

$needsCodegen = $Force -or -not (Test-Path -LiteralPath $stampPath)
if (-not $needsCodegen) {
  foreach ($output in $outputs) {
    if (-not (Test-Path -LiteralPath $output -PathType Leaf)) {
      $needsCodegen = $true
      break
    }
  }
}
$fingerprint = Get-Fingerprint
if (-not $needsCodegen) {
  $needsCodegen = ((Get-Content -Raw -LiteralPath $stampPath).Trim() -ne $fingerprint)
}

if (-not $needsCodegen) {
  Write-Host '[ OK  ] Flutter-Rust bindings are up to date.' -ForegroundColor Green
  exit 0
}

if ($null -eq (Get-Command flutter_rust_bridge_codegen -ErrorAction SilentlyContinue)) {
  throw 'flutter_rust_bridge_codegen is required because FRB outputs are missing or stale. Install with: cargo install flutter_rust_bridge_codegen'
}

Write-Host '[INFO ] Regenerating flutter_rust_bridge bindings...' -ForegroundColor Cyan
& flutter_rust_bridge_codegen generate `
  --rust-root $crateRoot `
  --rust-input 'crate::bridge,crate::portalis_api,crate::nexus::device,crate::nexus::settings' `
  --dart-output 'lib/nexus/bridge' `
  --rust-output (Join-Path $crateRoot 'src/api.rs') `
  --no-add-mod-to-lib
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

& cargo fmt --manifest-path (Join-Path $crateRoot 'Cargo.toml') --all
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

New-Item -ItemType Directory -Force -Path $stampDir | Out-Null
Set-Content -NoNewline -LiteralPath $stampPath -Value $fingerprint
Write-Host '[ OK  ] Flutter-Rust bindings regenerated.' -ForegroundColor Green
