$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
Push-Location $repo
try {
    . ..\emsdk\emsdk_env.ps1
    $env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
    cargo +nightly -Z build-std=std,panic_abort build -p horizon-client --bin horizon-client --target wasm32-unknown-emscripten --profile fast --no-default-features
} finally {
    Pop-Location
}
