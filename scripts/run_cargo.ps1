$ErrorActionPreference = "Stop"
$llvmBin = "C:\Users\altze\AppData\Local\Microsoft\WinGet\Packages\MartinStorsjo.LLVM-MinGW.UCRT_Microsoft.Winget.Source_8wekyb3d8bbwe\llvm-mingw-20261020-ucrt-x86_64\bin"
$cleanedPath = (($env:PATH -split ';') | Where-Object { $_ -notlike '*MinGW*' }) -join ';'
$env:PATH = "$llvmBin;$env:USERPROFILE\.cargo\bin;$cleanedPath"
$env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS = "-L native=C:/Users/altze/llvm-compat"
$env:HOST_RUSTFLAGS = "-L native=C:/Users/altze/llvm-compat"

Set-Location $PSScriptRoot\..
& cargo $args
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
