# Builds the DirectShow camera filter and puts it where the bundler expects it.
#
# Both CI and a local `npx tauri build` run this same script (the latter through
# scripts/before-bundle.mjs), so the DLL in the installer is always built from
# the sources in this checkout.

$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$source = Join-Path $root "native/windows"
$build = Join-Path $source "build"
$resources = Join-Path $root "src-tauri/resources"

cmake -S $source -B $build -A x64
if ($LASTEXITCODE -ne 0) { throw "configuring the camera filter failed" }

cmake --build $build --config Release
if ($LASTEXITCODE -ne 0) { throw "building the camera filter failed" }

$dll = Join-Path $build "Release/dji_virtual_camera.dll"
if (-not (Test-Path $dll)) { throw "the camera filter was not produced at $dll" }

New-Item -ItemType Directory -Force -Path $resources | Out-Null
Copy-Item $dll $resources -Force
Write-Host "camera filter ready: $(Join-Path $resources 'dji_virtual_camera.dll')"
