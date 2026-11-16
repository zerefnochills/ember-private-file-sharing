# Scripts: check_banned_words.ps1
# Scans user-facing documentation, distribution files, and client source code
# to enforce the hard ban on prohibited marketing buzzwords.

$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")

Write-Host "=== Scanning User-Facing Files for Banned Marketing Buzzwords ===" -ForegroundColor Cyan

# Banned phrases that must NEVER appear in user-facing promotional or application text
$bannedPatterns = @(
    "anonymous",
    "untraceable",
    "military[- ]grade",
    "leaves no trace",
    "no logs"
)

# User-facing target directories and files
$targetPaths = @(
    "README.md",
    "docs/distribution/store-listing.md",
    "docs/distribution/fdroid-metadata.yml",
    "tools/cli/src",
    "tools/admin-cli/src",
    "web-receiver/index.html"
)

if (Test-Path "web-receiver/src") {
    $targetPaths += "web-receiver/src"
}
if (Test-Path "android") {
    $targetPaths += "android"
}

$violations = 0

foreach ($relPath in $targetPaths) {
    if (Test-Path $relPath) {
        $files = if (Test-Path $relPath -PathType Container) {
            Get-ChildItem -Path $relPath -Recurse -File |
                Where-Object {
                    $_.FullName -notmatch '[\\/](build|\.gradle|\.kotlin|target)[\\/]' -and
                    $_.Extension -notin @('.class', '.dex', '.jar', '.apk', '.aab')
                } |
                Select-Object -ExpandProperty FullName
        } else {
            @( (Get-Item $relPath).FullName )
        }

        foreach ($file in $files) {
            $content = Get-Content $file -Raw
            foreach ($pat in $bannedPatterns) {
                if ($content -match "(?i)\b$pat\b") {
                    Write-Host ("  [ERROR] Banned buzzword pattern '$pat' found in $file") -ForegroundColor Red
                    $violations++
                }
            }
        }
    }
}

if ($violations -gt 0) {
    Write-Host "Banned words check failed with $violations violation(s)." -ForegroundColor Red
    exit 1
}

Write-Host "Banned words check passed cleanly. Zero prohibited marketing buzzwords found." -ForegroundColor Green
exit 0
