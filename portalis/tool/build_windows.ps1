<#
.SYNOPSIS
  Incrementally build the Portalis Windows backend and Flutter application.
.DESCRIPTION
  FRB generation is conditional and shared with frb_generate.ps1. Rust and
  Flutter builds retain their normal incremental caches. The backend DLL is
  copied beside the Windows runner so the generated FFI loader can find it.
.EXAMPLE
  ./tool/build_windows.ps1
.EXAMPLE
  ./tool/build_windows.ps1 -ForceFrb -Configuration Release -Run
#>

[CmdletBinding()]
param(
  [switch]$ForceFrb,
  [switch]$NoCodegen,
  [switch]$Clean,
  [switch]$Run,
  [ValidateSet('Debug', 'Profile', 'Release')]
  [string]$Configuration = 'Debug'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Write-Info($message) { Write-Host "[INFO ] $message" -ForegroundColor Cyan }
function Write-Ok($message) { Write-Host "[ OK  ] $message" -ForegroundColor Green }

$toolRoot = Split-Path -Parent $MyInvocation.MyCommand.Definition
$repoRoot = (Resolve-Path (Join-Path $toolRoot '..')).Path
$crateRoot = Join-Path $repoRoot 'rust/backend'
# Keep the native DLL in release mode for parity with the FRB loader's
# configured rust/backend/target/release lookup. Flutter may still be Debug.
$profile = 'release'
Set-Location $repoRoot

if ($Clean) {
  Write-Info 'Cleaning Flutter Windows artifacts.'
  flutter clean
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
  Remove-Item -Recurse -Force -ErrorAction SilentlyContinue (Join-Path $repoRoot 'build/windows')
}

if (-not $NoCodegen) {
  if ($ForceFrb) { & (Join-Path $toolRoot 'frb_generate.ps1') -Force }
  else { & (Join-Path $toolRoot 'frb_generate.ps1') }
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} else {
  Write-Info 'Skipping Flutter-Rust Bridge generation.'
}

$packageConfig = Join-Path $repoRoot '.dart_tool/package_config.json'
$pubStamp = Join-Path $repoRoot '.dart_tool/portalis/pub-get.stamp'
$pubspec = Join-Path $repoRoot 'pubspec.yaml'
$lockfile = Join-Path $repoRoot 'pubspec.lock'
$needPubGet = -not (Test-Path -LiteralPath $packageConfig) -or -not (Test-Path -LiteralPath $pubStamp)
if (-not $needPubGet) {
  $stampTime = (Get-Item -LiteralPath $pubStamp).LastWriteTimeUtc
  $needPubGet = ((Get-Item -LiteralPath $pubspec).LastWriteTimeUtc -gt $stampTime) -or
    ((Get-Item -LiteralPath $lockfile).LastWriteTimeUtc -gt $stampTime)
}
if ($needPubGet) {
  Write-Info 'Resolving Dart packages.'
  flutter pub get
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
  New-Item -ItemType Directory -Force -Path (Split-Path -Parent $pubStamp) | Out-Null
  Set-Content -NoNewline -LiteralPath $pubStamp -Value (Get-Date -Format o)
} else {
  Write-Info 'Dart packages are up to date.'
}

$dll = Join-Path $crateRoot "target/$profile/backend.dll"
$rustInputs = @(
  (Join-Path $crateRoot 'Cargo.toml'),
  (Join-Path $crateRoot 'Cargo.lock'),
  (Join-Path $crateRoot 'src'),
  (Join-Path $crateRoot 'vendor')
)
$needsRust = -not (Test-Path -LiteralPath $dll)
if (-not $needsRust) {
  $dllTime = (Get-Item -LiteralPath $dll).LastWriteTimeUtc
  foreach ($input in $rustInputs) {
    if (Test-Path -LiteralPath $input -PathType Container) {
      $newer = Get-ChildItem -LiteralPath $input -Recurse -File | Where-Object { $_.LastWriteTimeUtc -gt $dllTime } | Select-Object -First 1
      if ($null -ne $newer) { $needsRust = $true; break }
    } elseif ((Get-Item -LiteralPath $input).LastWriteTimeUtc -gt $dllTime) {
      $needsRust = $true; break
    }
  }
}
if ($needsRust) {
  Write-Info "Building Rust backend ($profile)."
  Push-Location $crateRoot
  if ($profile -eq 'release') { cargo build --release } else { cargo build }
  $cargoStatus = $LASTEXITCODE
  Pop-Location
  if ($cargoStatus -ne 0) { exit $cargoStatus }
} else {
  Write-Info "Rust backend is up to date ($profile)."
}

Write-Info "Building Flutter Windows application ($Configuration)."
$flutterMode = $Configuration.ToLowerInvariant()
flutter build windows "--$flutterMode"
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$runnerDir = Join-Path $repoRoot "build/windows/x64/runner/$Configuration"
if (-not (Test-Path -LiteralPath $runnerDir)) {
  throw "Flutter Windows output directory not found: $runnerDir"
}
Copy-Item -Force -LiteralPath $dll -Destination (Join-Path $runnerDir 'backend.dll')
Write-Ok "Windows backend copied to $runnerDir/backend.dll"

if ($Run) {
  Write-Info 'Running Flutter Windows application.'
  flutter run -d windows "--$flutterMode"
  exit $LASTEXITCODE
}

Write-Ok 'Windows build complete.'
