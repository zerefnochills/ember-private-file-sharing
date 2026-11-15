$ErrorActionPreference = "Stop"

$androidDir = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path

Write-Host "Checking for forbidden Google dependencies in Android module..." -ForegroundColor Cyan

$forbiddenPatterns = @(
    'com\.google\.android\.gms',
    'com\.google\.firebase',
    'com\.google\.mlkit',
    'play-services',
    'firebase-',
    'firebase:',
    'mlkit',
    'NearbyConnections'
)

$files = Get-ChildItem -Path $androidDir -File -Recurse -Include "*.gradle.kts", "*.gradle", "*.xml", "*.kt", "*.java", "*.properties" |
    Where-Object { $_.FullName -notlike "*\build\*" -and $_.FullName -notlike "*\.gradle\*" }

$violations = @()

foreach ($file in $files) {
    $content = Get-Content -Path $file.FullName -Raw
    foreach ($pattern in $forbiddenPatterns) {
        if ($content -match $pattern) {
            $violations += "[$($file.FullName)] contains forbidden pattern: $pattern"
        }
    }
}

if ($violations.Count -gt 0) {
    Write-Host "Forbidden Google dependencies detected!" -ForegroundColor Red
    $violations | ForEach-Object { Write-Host "  $_" -ForegroundColor Red }
    exit 1
}

Write-Host "Zero forbidden Google dependencies found across $($files.Count) Android source/config files." -ForegroundColor Green
exit 0
