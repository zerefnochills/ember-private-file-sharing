# Scripts: check_banned_words.ps1
# Scans user-facing documentation, distribution files, and client source code
# to enforce the hard ban on prohibited marketing buzzwords.

$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")

Write-Host "=== Scanning User-Facing Files for Banned Marketing Buzzwords ===" -ForegroundColor Cyan

# Banned phrases that must NEVER appear in user-facing promotional or application text
$bannedPatterns = @(
    "military[- ]grade",
    "untraceable",
    "leaves no trace",
    "no logs"
)

# User-facing target directories and files
$targetFiles = @(
    "README.md",
    "docs/distribution/store-listing.md",
    "docs/distribution/fdroid-metadata.yml",
    "tools/cli/src/main.rs",
    "tools/admin-cli/src/main.rs",
    "web-receiver/index.html"
)

$violations = 0

foreach ($relPath in $targetFiles) {
    if (Test-Path $relPath) {
        $content = Get-Content $relPath -Raw
        foreach ($pat in $bannedPatterns) {
            if ($content -match "(?i)\b$pat\b") {
                Write-Host ("  [ERROR] Banned buzzword pattern '$pat' found in $relPath") -ForegroundColor Red
                $violations++
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
