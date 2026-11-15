$ErrorActionPreference = "Stop"

if (-not [string]::IsNullOrWhiteSpace($env:EMBER_LLVM_MINGW_BIN)) {
    $dlltool = Join-Path $env:EMBER_LLVM_MINGW_BIN "dlltool.exe"
    if (-not (Test-Path $dlltool)) {
        throw "EMBER_LLVM_MINGW_BIN must contain dlltool.exe: $env:EMBER_LLVM_MINGW_BIN"
    }
    $env:PATH = "$env:EMBER_LLVM_MINGW_BIN;$env:USERPROFILE\.cargo\bin;$env:PATH"
} else {
    $env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$ffiManifest = Join-Path $repoRoot "android-bindings\Cargo.toml"
$jniLibs = Join-Path $PSScriptRoot "app\src\main\jniLibs"
$generatedBindings = Join-Path $PSScriptRoot "app\build\generated\uniffi\kotlin"

New-Item -ItemType Directory -Force -Path $jniLibs, $generatedBindings | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $jniLibs "arm64-v8a"), (Join-Path $jniLibs "armeabi-v7a"), (Join-Path $jniLibs "x86_64") | Out-Null

Push-Location $repoRoot
try {
    if (Get-Command cargo-ndk -ErrorAction SilentlyContinue) {
        Write-Host "Building with cargo-ndk..." -ForegroundColor Cyan
        cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 -o $jniLibs build --manifest-path $ffiManifest --release
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    } else {
        Write-Host "Building with direct NDK target linkers..." -ForegroundColor Cyan
        $targets = @(
            @{ rust = "aarch64-linux-android"; abi = "arm64-v8a" },
            @{ rust = "armv7-linux-androideabi"; abi = "armeabi-v7a" },
            @{ rust = "x86_64-linux-android"; abi = "x86_64" }
        )
        foreach ($t in $targets) {
            Write-Host "Building for $($t.rust) ($($t.abi))..." -ForegroundColor Cyan
            & cargo build --manifest-path $ffiManifest --target $($t.rust) --release
            if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
            $srcSo = Join-Path $repoRoot "target\$($t.rust)\release\libember_android_bindings.so"
            $dstDir = Join-Path $jniLibs $t.abi
            Copy-Item -Path $srcSo -Destination (Join-Path $dstDir "libember_android_bindings.so") -Force
        }
    }

    $library = Join-Path $repoRoot "target\aarch64-linux-android\release\libember_android_bindings.so"
    if (-not (Test-Path $library)) {
        throw "Expected Android library was not produced: $library"
    }

    Write-Host "Generating Kotlin bindings..." -ForegroundColor Cyan
    $uniffiConfig = Join-Path $repoRoot "android-bindings\uniffi.toml"
    & cargo run --manifest-path $ffiManifest --features cli --bin uniffi-bindgen -- generate $library --language kotlin --no-format --out-dir $generatedBindings --config $uniffiConfig
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    Write-Host "Native build and binding generation completed successfully." -ForegroundColor Green
}
finally {
    Pop-Location
}
