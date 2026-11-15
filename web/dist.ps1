param([switch]$SkipBuild)
# Packs the browser build into web/dist/, the folder Render publishes (render.yaml).
# It holds the page, Emscripten's loader and the wasm, and none of Minecraft's files: players
# load their own prepared game files in the page.
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
if (-not $SkipBuild) { & "$PSScriptRoot\build.ps1" }
$out = Join-Path $repo 'target\wasm32-unknown-emscripten\fast'
$dist = Join-Path $PSScriptRoot 'dist'
New-Item -ItemType Directory -Force $dist | Out-Null
Copy-Item (Join-Path $out 'horizon-client.js') $dist -Force
Copy-Item (Join-Path $out 'horizon_client.wasm') $dist -Force
# The page loads the loader from the build folder when served by serve.mjs; here it sits beside it.
# Read and written as UTF-8 explicitly: Windows PowerShell 5.1 would read it as ANSI and turn
# "·" and "…" into "Â·" and "â€¦".
$utf8 = New-Object System.Text.UTF8Encoding($false)
$page = [System.IO.File]::ReadAllText((Join-Path $PSScriptRoot 'index.html'), $utf8)
$page = $page -replace '/target/wasm32-unknown-emscripten/fast/horizon-client\.js', '/horizon-client.js'
[System.IO.File]::WriteAllText((Join-Path $dist 'index.html'), $page, $utf8)
Get-ChildItem $dist | Select-Object Name, @{n = 'MB'; e = { [math]::Round($_.Length / 1MB, 1) } }
