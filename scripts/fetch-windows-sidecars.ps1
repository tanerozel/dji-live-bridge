# Downloads the Windows sidecars the app bundles: MediaMTX, FFmpeg and ffprobe.
#
# Unlike macOS, FFmpeg is not built here — Windows builds come from BtbN's
# official FFmpeg-Builds, pinned to one dated release and verified by hash.
# The build is LGPL (no --enable-gpl, no libx264), which is what lets us ship
# it; hardware H.264 comes from NVENC, QuickSync or AMF.
#
# Usage: pwsh scripts/fetch-windows-sidecars.ps1

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$binaries = Join-Path $root 'src-tauri\binaries'
$licenses = Join-Path $root 'src-tauri\licenses'
$target = 'x86_64-pc-windows-msvc'
New-Item -ItemType Directory -Force -Path $binaries, $licenses | Out-Null

$mediamtxVersion = '1.21.0'
$mediamtxUrl = "https://github.com/bluenviron/mediamtx/releases/download/v$mediamtxVersion/mediamtx_v${mediamtxVersion}_windows_amd64.zip"
$mediamtxSha = '8a58a9b8c25ee99a96c23dc0a17f39ace3072c01d2e148329073c64ddf83493d'

# BtbN autobuild, pinned. A rolling "latest" URL would change under us.
$ffmpegTag = 'autobuild-2026-09-19-13-11'
$ffmpegAsset = 'ffmpeg-n9.0.2-win64-lgpl-9.0.zip'
$ffmpegUrl = "https://github.com/BtbN/FFmpeg-Builds/releases/download/$ffmpegTag/$ffmpegAsset"
$ffmpegSha = 'f0a85cd3977d987992a90d0e2a1e563d2aeb97078823a42216cdce154d8119cb'

function Get-Verified([string]$url, [string]$expected, [string]$destination) {
  Write-Host "==> Downloading $(Split-Path -Leaf $url)"
  Invoke-WebRequest -Uri $url -OutFile $destination -UseBasicParsing
  $actual = (Get-FileHash -Algorithm SHA256 -Path $destination).Hash.ToLower()
  if ($actual -ne $expected.ToLower()) {
    throw "checksum mismatch for $url`n  expected $expected`n  actual   $actual"
  }
}

$work = Join-Path ([System.IO.Path]::GetTempPath()) ("dji-sidecars-" + [guid]::NewGuid())
New-Item -ItemType Directory -Force -Path $work | Out-Null
try {
  $mediamtxZip = Join-Path $work 'mediamtx.zip'
  Get-Verified $mediamtxUrl $mediamtxSha $mediamtxZip
  Expand-Archive -Path $mediamtxZip -DestinationPath (Join-Path $work 'mediamtx') -Force
  Copy-Item (Join-Path $work 'mediamtx\mediamtx.exe') (Join-Path $binaries "mediamtx-$target.exe") -Force

  $ffmpegZip = Join-Path $work 'ffmpeg.zip'
  Get-Verified $ffmpegUrl $ffmpegSha $ffmpegZip
  Expand-Archive -Path $ffmpegZip -DestinationPath (Join-Path $work 'ffmpeg') -Force
  $bin = Get-ChildItem -Path (Join-Path $work 'ffmpeg') -Filter 'ffmpeg.exe' -Recurse | Select-Object -First 1
  if (-not $bin) { throw 'ffmpeg.exe not found in the archive' }
  Copy-Item $bin.FullName (Join-Path $binaries "ffmpeg-$target.exe") -Force
  Copy-Item (Join-Path $bin.DirectoryName 'ffprobe.exe') (Join-Path $binaries "ffprobe-$target.exe") -Force

  # Shipping FFmpeg means shipping its licence.
  $licenseFile = Get-ChildItem -Path (Join-Path $work 'ffmpeg') -Filter 'LICENSE.txt' -Recurse | Select-Object -First 1
  if ($licenseFile) { Copy-Item $licenseFile.FullName (Join-Path $licenses 'FFmpeg-LICENSE-windows.txt') -Force }
  @(
    "FFmpeg for Windows, bundled with DJI Live Bridge."
    "Source: $ffmpegUrl"
    "sha256: $ffmpegSha"
    "Build:  BtbN FFmpeg-Builds, win64 LGPL variant (no --enable-gpl, no libx264)."
    "Build scripts: https://github.com/BtbN/FFmpeg-Builds"
  ) | Set-Content -Path (Join-Path $licenses 'FFmpeg-BUILD-windows.txt')

  # A GPL build must never reach the bundle: the configuration string is inside
  # the executable, so check it rather than trusting the file name.
  $configuration = Select-String -Path (Join-Path $binaries "ffmpeg-$target.exe") -Pattern '--enable-gpl' -Encoding ascii -SimpleMatch -List
  if ($configuration) { throw 'the downloaded FFmpeg is a GPL build and must not be shipped' }

  Write-Host "`n==> Done"
  Get-ChildItem $binaries -Filter "*$target*" | ForEach-Object { '{0,8:N0} KB  {1}' -f ($_.Length / 1KB), $_.Name }
}
finally {
  Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
}
