$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
Push-Location $repo
try {
    # Emscripten is in ../emsdk beside the repository unless EMSDK_DIR says where.
    $emsdk = if ($env:EMSDK_DIR) { $env:EMSDK_DIR } else { Join-Path (Split-Path -Parent $repo) 'emsdk' }
    . (Join-Path $emsdk 'emsdk_env.ps1')
    $env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
    # Cargo prints warnings on stderr, which Windows PowerShell 5.1 would turn into a failure.
    $ErrorActionPreference = 'Continue'
    cargo +nightly -Z build-std=std,panic_abort build -p horizon-client --bin horizon-client --target wasm32-unknown-emscripten --profile fast --no-default-features
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed ($LASTEXITCODE)" }
} finally {
    Pop-Location
}
