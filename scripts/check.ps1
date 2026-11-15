$ErrorActionPreference = "Stop"

$llvmBin = "C:\Users\altze\AppData\Local\Microsoft\WinGet\Packages\MartinStorsjo.LLVM-MinGW.UCRT_Microsoft.Winget.Source_8wekyb3d8bbwe\llvm-mingw-20261020-ucrt-x86_64\bin"
$cleanedPath = (($env:PATH -split ';') | Where-Object { $_ -notlike '*MinGW*' }) -join ';'
$env:PATH = "$llvmBin;$env:USERPROFILE\.cargo\bin;$cleanedPath"

Write-Host "=== Running cargo fmt ===" -ForegroundColor Cyan
cargo fmt --all --check
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "=== Running cargo clippy ===" -ForegroundColor Cyan
cargo clippy --workspace --all-targets -- -D warnings
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "=== Building binaries ===" -ForegroundColor Cyan
cargo build --bins
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "=== Running cargo test ===" -ForegroundColor Cyan
cargo test --workspace
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "=== Gate passed cleanly ===" -ForegroundColor Green
