$ErrorActionPreference = "Stop"

$llvmBin = "C:\Users\altze\AppData\Local\Microsoft\WinGet\Packages\MartinStorsjo.LLVM-MinGW.UCRT_Microsoft.Winget.Source_8wekyb3d8bbwe\llvm-mingw-20261020-ucrt-x86_64\bin"
$cleanedPath = (($env:PATH -split ';') | Where-Object { $_ -notlike '*MinGW*' }) -join ';'
$env:PATH = "$llvmBin;$env:USERPROFILE\.cargo\bin;$cleanedPath"

Set-Location (Join-Path $PSScriptRoot "..")

Write-Host "=== Git Status ===" -ForegroundColor Cyan
git status

Write-Host "=== Git Log (Last 10) ===" -ForegroundColor Cyan
git log --oneline -10

Write-Host "=== Rustc & Cargo Version ===" -ForegroundColor Cyan
rustc --version
cargo --version

Write-Host "=== Checking Cross-Agent Requests ===" -ForegroundColor Cyan
if (Test-Path "docs/cross-agent-requests.md") {
    Get-Content "docs/cross-agent-requests.md"
} else {
    Write-Host "No docs/cross-agent-requests.md found."
}
