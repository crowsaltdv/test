param([switch]$SkipBuild)
# Packs the browser build into web/dist/, the folder Render publishes (render.yaml).
# It holds the page, Emscripten's loader and the wasm, and none of Minecraft's files: players
# load their own prepared game files in the page.
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
if (-not $SkipBuild) { & "$PSScriptRoot\build.ps1" }
& "$PSScriptRoot\pack.ps1" -From (Join-Path $repo 'target\wasm32-unknown-emscripten\fast')
