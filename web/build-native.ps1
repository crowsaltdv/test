param(
    # The registry ids (one per line, `#` for comments) to compile into the browser build.
    [string]$List = (Join-Path $PSScriptRoot 'native-mods.txt'),
    # Stop after staging the modded source (no compile, no pack).
    [switch]$StageOnly
)
# Builds the browser game with native mods compiled in, and packs it into web/dist/.
#
# A native mod is a patch to the game's own Rust source (docs/native-mods.md), so a browser, which
# runs one prebuilt game, can only have the ones this script compiled in. They show up in
# Options > Mods with a switch, and "(built in)" in Browse Mods. Needs git, Rust nightly and
# Emscripten (EMSDK_DIR, else ../emsdk), as web/build.ps1 does. Run it from a clean checkout:
# the source it patches is `git archive HEAD`, so commit your changes first.
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$work = Join-Path $repo '.local\native-build'
$toolDir = Join-Path $repo '.local\horizon-mixins'
$registry = 'https://horizon-rust.github.io/horizon-mods/index.json'
New-Item -ItemType Directory -Force $work | Out-Null
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"

# 1. horizon-mixins, the tool that applies the bundles (github.com/horizon-rust/horizon-mixins).
$tool = Join-Path $toolDir 'target\debug\horizon-mixins.exe'
if (-not (Test-Path $tool)) {
    if (-not (Test-Path $toolDir)) { git clone --depth 1 https://github.com/horizon-rust/horizon-mixins.git $toolDir }
    Push-Location $toolDir
    try {
        $ErrorActionPreference = 'Continue'
        cargo +nightly build --locked -p horizon-mixins-cli
        if ($LASTEXITCODE -ne 0) { throw "building horizon-mixins failed ($LASTEXITCODE)" }
        $ErrorActionPreference = 'Stop'
    } finally { Pop-Location }
}

# 2. The bundles: downloaded from the registry and checked against the index's SHA-256.
$ids = Get-Content $List | ForEach-Object { ($_ -replace '#.*', '').Trim() } | Where-Object { $_ }
$index = Invoke-RestMethod $registry
$mods = Join-Path $work 'mods'
Remove-Item $mods -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $mods | Out-Null
foreach ($id in $ids) {
    $project = $index.projects | Where-Object { $_.id -eq $id -and $_.kind -eq 'native-mod' } | Select-Object -First 1
    if (-not $project) { Write-Warning "$id is not a native mod of the registry: skipped"; continue }
    $file = $project.versions[0].file
    $zip = Join-Path $work "$id.zip"
    Invoke-WebRequest $file.url -OutFile $zip
    if ((Get-FileHash $zip -Algorithm SHA256).Hash -ne $file.sha256.ToUpper()) { throw "$id does not match the reviewed file" }
    $unpacked = Join-Path $work "unpacked-$id"
    Remove-Item $unpacked -Recurse -Force -ErrorAction SilentlyContinue
    Expand-Archive $zip $unpacked
    Copy-Item (Join-Path $unpacked "mixins\$id") (Join-Path $mods $id) -Recurse
    # The bundle names the one game commit it was tested on; it needs only the stable hook
    # points (hooks 1), which this checkout has: the pin is dropped, as a launcher's build does.
    $toml = Join-Path $mods "$id\horizon-mixin.toml"
    (Get-Content $toml) | Where-Object { $_ -notmatch '^horizon_revision\s*=' } | Set-Content $toml
    Remove-Item $zip, $unpacked -Recurse -Force
}

# 3. The pristine source, and the tool's staged copy of it with the bundles applied.
$game = Join-Path $work 'game'
$modded = Join-Path $work 'modded'
Remove-Item $game -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $game | Out-Null
git -C $repo archive HEAD | tar -x -C $game
& $tool apply --workspace $game --mods $mods --out $modded
if ($LASTEXITCODE -ne 0) { throw 'applying the mods failed: drop the mod it names from the list' }
if ($StageOnly) { Write-Host "Staged in $modded"; return }

# 4. Compile for the browser and pack.
$emsdk = if ($env:EMSDK_DIR) { $env:EMSDK_DIR } else { Join-Path (Split-Path -Parent $repo) 'emsdk' }
$env:EMSDK_QUIET = '1'
. (Join-Path $emsdk 'emsdk_env.ps1')
Push-Location $modded
try {
    $ErrorActionPreference = 'Continue'
    cargo +nightly -Z build-std=std,panic_abort build -p horizon-client --bin horizon-client --target wasm32-unknown-emscripten --profile fast --no-default-features
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed ($LASTEXITCODE)" }
    $ErrorActionPreference = 'Stop'
} finally { Pop-Location }
$out = Join-Path $modded 'target\wasm32-unknown-emscripten\fast'
# The page, the app's files (manifest, worker, icons) and the loader and wasm, packed as
# web/dist.ps1 packs them.
& (Join-Path $PSScriptRoot 'pack.ps1') -From $out
