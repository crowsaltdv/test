param(
    # The build folder holding Emscripten's loader and the wasm. Without it, the loader and wasm
    # already in dist (put there by web/build-native.ps1, say) stay, and only the app's own files
    # are packed again.
    [string]$From
)
# Packs a browser build into web/dist/, the folder Render publishes (render.yaml): the page,
# Emscripten's loader, the wasm and the app's own files (manifest, service worker, icons), and none
# of Minecraft's files. web/dist.ps1 and web/build-native.ps1 both end here, so either build packs
# the same way.
$ErrorActionPreference = 'Stop'
$dist = Join-Path $PSScriptRoot 'dist'
New-Item -ItemType Directory -Force $dist | Out-Null
if ($From) {
    Copy-Item (Join-Path $From 'horizon-client.js') $dist -Force
    Copy-Item (Join-Path $From 'horizon_client.wasm') $dist -Force
} elseif (-not (Test-Path (Join-Path $dist 'horizon_client.wasm'))) {
    throw "no wasm in ${dist}: build one (web/build.ps1) and pack it with -From"
}
# Read and written as UTF-8 explicitly: Windows PowerShell 5.1 would read it as ANSI and turn
# "·" and "…" into "Â·" and "â€¦".
$utf8 = New-Object System.Text.UTF8Encoding($false)
# The page loads the loader from the build folder when serve.mjs serves it; here it sits beside it.
$page = [System.IO.File]::ReadAllText((Join-Path $PSScriptRoot 'index.html'), $utf8)
$page = $page -replace '/target/wasm32-unknown-emscripten/fast/horizon-client\.js', '/horizon-client.js'
[System.IO.File]::WriteAllText((Join-Path $dist 'index.html'), $page, $utf8)
# Installable app: web/manifest.webmanifest names the icons, web/sw.js keeps the page, the loader
# and the wasm on the player's disk. The worker's cache version is the wasm's hash, so a rebuilt
# game installs a new worker and drops the old cache without anyone bumping a number by hand
# (web/serve.mjs stamps the same constant from the wasm's time and size).
$version = (Get-FileHash (Join-Path $dist 'horizon_client.wasm') -Algorithm SHA256).Hash.Substring(0, 12).ToLowerInvariant()
Copy-Item (Join-Path $PSScriptRoot 'manifest.webmanifest') $dist -Force
$worker = [System.IO.File]::ReadAllText((Join-Path $PSScriptRoot 'sw.js'), $utf8)
$worker = $worker -replace "const VERSION = '[^']*';", "const VERSION = '$version';"
[System.IO.File]::WriteAllText((Join-Path $dist 'sw.js'), $worker, $utf8)
# The manifest's icons are the app icons already rendered into packaging/icons/ by
# `cargo run -p xtask -- icons`; the site holds only Horizon's own artwork.
$icons = Join-Path $dist 'icons'
New-Item -ItemType Directory -Force $icons | Out-Null
$hicolor = Join-Path (Split-Path -Parent $PSScriptRoot) 'packaging\icons\hicolor'
Copy-Item (Join-Path $hicolor '256x256\apps\horizon.png') (Join-Path $icons 'icon-256.png') -Force
Copy-Item (Join-Path $hicolor '512x512\apps\horizon.png') (Join-Path $icons 'icon-512.png') -Force
Copy-Item (Join-Path $hicolor 'scalable\apps\horizon.svg') (Join-Path $dist 'favicon.svg') -Force
Get-ChildItem $dist -Recurse -File | Select-Object Name, @{n = 'MB'; e = { [math]::Round($_.Length / 1MB, 1) } }
