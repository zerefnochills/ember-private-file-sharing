$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$ffiManifest = Join-Path $PSScriptRoot "ember-ffi\Cargo.toml"
$jniLibs = Join-Path $PSScriptRoot "app\src\main\jniLibs"
$generatedBindings = Join-Path $PSScriptRoot "app\build\generated\uniffi\kotlin"

if (-not (Get-Command cargo-ndk -ErrorAction SilentlyContinue)) {
    throw "cargo-ndk is required. Install cargo-ndk and Android Rust targets first."
}
New-Item -ItemType Directory -Force -Path $jniLibs, $generatedBindings | Out-Null
cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 -o $jniLibs build --manifest-path $ffiManifest --release
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$library = Join-Path $repoRoot "target\aarch64-linux-android\release\libember_android_ffi.so"
if (-not (Test-Path $library)) {
    throw "Expected Android library was not produced: $library"
}
cargo run --manifest-path $ffiManifest --features cli --bin uniffi-bindgen -- generate --library $library --language kotlin --out-dir $generatedBindings
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
