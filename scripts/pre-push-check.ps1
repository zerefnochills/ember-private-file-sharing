# Quick safety check before pushing.
# Usage: scripts\pre-push-check.ps1        (fast checks)
#        scripts\pre-push-check.ps1 -Full  (also runs scripts\check.ps1)
param([switch]$Full)

if ($PSScriptRoot) { Set-Location (Join-Path $PSScriptRoot "..") }
$root = git rev-parse --show-toplevel
if (-not $root) { Write-Host "Not inside a git repository."; exit 1 }
Set-Location $root

$script:failed = $false
function Bad($m)  { Write-Host "FAIL: $m" -ForegroundColor Red;    $script:failed = $true }
function Warn($m) { Write-Host "WARN: $m" -ForegroundColor Yellow }

Write-Host "== Branch =="
$branch = git branch --show-current
if (-not $branch) { $branch = "DETACHED HEAD" }
Write-Host "On branch: $branch"
$upstream = git rev-parse --abbrev-ref '@{u}' 2>$null
if ($LASTEXITCODE -eq 0 -and $upstream) {
    Write-Host "Commits not yet pushed to ${upstream}:"
    git log --oneline "$upstream..HEAD"
} else {
    Write-Host "No upstream yet: first push needs 'git push -u origin $branch'"
}

Write-Host "== Uncommitted changes =="
$status = git status --porcelain
if ($status) { Warn "working tree is not clean:"; git status --short } else { Write-Host "clean" }

Write-Host "== Secret-like or local files (tracked, or untracked and not ignored) =="
$all = @(git ls-files) + @(git ls-files --others --exclude-standard) | Sort-Object -Unique
$pattern = '(^|/)\.env($|\.)|\.(pem|key|jks|keystore|p12)$|(^|/)local\.properties$|\.salive$'
$secrets = $all | Where-Object { $_ -match $pattern -and $_ -notmatch '\.env\.example$' }
if ($secrets) { Bad "these files should not be committed:"; $secrets | ForEach-Object { Write-Host "  $_" } } else { Write-Host "none" }

Write-Host "== Large files (over 5 MB) =="
$large = $all | Where-Object { (Test-Path -LiteralPath $_ -PathType Leaf) -and ((Get-Item -LiteralPath $_).Length -gt 5MB) }
if ($large) { Warn "large files:"; $large | ForEach-Object { Write-Host "  $_" } } else { Write-Host "none" }

Write-Host "== Merge conflict markers =="
$markers = git grep -n -E '^(<<<<<<< |>>>>>>> )' -- . ':!*.md' 2>$null
if ($markers) { Bad "conflict markers found:"; $markers | ForEach-Object { Write-Host "  $_" } } else { Write-Host "none" }

Write-Host "== Banned marketing words =="
if (Test-Path "scripts\check_banned_words.ps1") {
    & powershell -NoProfile -ExecutionPolicy Bypass -File "scripts\check_banned_words.ps1" *> $null
    if ($LASTEXITCODE -eq 0) { Write-Host "clean" } else { Bad "banned words found (run scripts\check_banned_words.ps1)" }
} else {
    Warn "scripts\check_banned_words.ps1 not found"
}

if ($Full) {
    Write-Host "== Full Rust gate =="
    & powershell -NoProfile -ExecutionPolicy Bypass -File "scripts\check.ps1"
    if ($LASTEXITCODE -ne 0) { Bad "scripts\check.ps1 failed" }
}

Write-Host ""
if ($script:failed) { Write-Host "NOT READY: fix the FAIL lines above" -ForegroundColor Red; exit 1 }
Write-Host "READY TO PUSH" -ForegroundColor Green
exit 0
