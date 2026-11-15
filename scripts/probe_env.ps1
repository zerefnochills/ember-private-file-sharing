$llvmBin = "C:\Users\altze\AppData\Local\Microsoft\WinGet\Packages\MartinStorsjo.LLVM-MinGW.UCRT_Microsoft.Winget.Source_8wekyb3d8bbwe\llvm-mingw-20261020-ucrt-x86_64\bin"
$cleanedPath = (($env:PATH -split ';') | Where-Object { $_ -notlike '*MinGW*' }) -join ';'
$env:PATH = "$llvmBin;$env:USERPROFILE\.cargo\bin;$cleanedPath"

Write-Host "=== Rust Toolchain ==="
rustc --version
cargo --version
rustup --version
Write-Host "=== Installed Targets ==="
rustup target list --installed

Write-Host "=== Cargo Plugins ==="
cargo --list

Write-Host "=== Android SDK ==="
Get-ChildItem -Path "C:\Users\altze\AppData\Local\Android\Sdk" -ErrorAction SilentlyContinue | Select-Object Name

Write-Host "=== Java ==="
java -version
javac -version
Write-Host "JAVA_HOME: $env:JAVA_HOME"

Write-Host "=== Gradle ==="
Get-Command gradle -ErrorAction SilentlyContinue
