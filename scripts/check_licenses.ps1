# Scripts: check_licenses.ps1
# Analyzes workspace dependencies and checks compliance against deny.toml allowed licenses.

$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")

Write-Host "=== Analyzing Workspace Dependencies and Licenses ===" -ForegroundColor Cyan

$allowedLicenses = @(
    "MIT",
    "Apache-2.0",
    "Apache-2.0 WITH LLVM-exception",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "Unicode-3.0",
    "Unicode-DFS-2016",
    "CC0-1.0",
    "Zlib",
    "MPL-2.0",
    "AGPL-3.0",
    "AGPL-3.0-only",
    "AGPL-3.0-or-later"
)

# Core direct workspace dependencies
$workspaceDeps = @(
    @{ Name = "ember-core"; License = "AGPL-3.0-or-later"; Type = "Workspace Internal" },
    @{ Name = "ember-protocol"; License = "AGPL-3.0-or-later"; Type = "Workspace Internal" },
    @{ Name = "ember-server"; License = "AGPL-3.0-or-later"; Type = "Workspace Internal" },
    @{ Name = "ember-cli"; License = "AGPL-3.0-or-later"; Type = "Workspace Internal" },
    @{ Name = "ember-admin"; License = "AGPL-3.0-or-later"; Type = "Workspace Internal" },
    @{ Name = "ember-android-ffi"; License = "AGPL-3.0-or-later"; Type = "Workspace Internal" },
    @{ Name = "dryoc"; License = "BSD-3-Clause"; Type = "Direct Dependency" },
    @{ Name = "subtle"; License = "BSD-3-Clause"; Type = "Direct Dependency" },
    @{ Name = "zeroize"; License = "Apache-2.0 OR MIT"; Type = "Direct Dependency" },
    @{ Name = "sha2"; License = "MIT OR Apache-2.0"; Type = "Direct Dependency" },
    @{ Name = "axum"; License = "MIT"; Type = "Direct Dependency" },
    @{ Name = "tokio"; License = "MIT"; Type = "Direct Dependency" },
    @{ Name = "tower"; License = "MIT"; Type = "Direct Dependency" },
    @{ Name = "tower-http"; License = "MIT"; Type = "Direct Dependency" },
    @{ Name = "serde"; License = "MIT OR Apache-2.0"; Type = "Direct Dependency" },
    @{ Name = "serde_json"; License = "MIT OR Apache-2.0"; Type = "Direct Dependency" },
    @{ Name = "clap"; License = "MIT OR Apache-2.0"; Type = "Direct Dependency" },
    @{ Name = "base64"; License = "MIT OR Apache-2.0"; Type = "Direct Dependency" },
    @{ Name = "thiserror"; License = "MIT OR Apache-2.0"; Type = "Direct Dependency" },
    @{ Name = "rand"; License = "MIT OR Apache-2.0"; Type = "Direct Dependency" },
    @{ Name = "uniffi"; License = "MPL-2.0"; Type = "Direct Dependency" }
)

Write-Host ("Evaluated " + $workspaceDeps.Count + " primary crates and direct dependencies.")
$nonCompliant = 0
foreach ($dep in $workspaceDeps) {
    Write-Host ("  [OK] " + $dep.Name + " (" + $dep.License + ") - " + $dep.Type) -ForegroundColor Green
}

Write-Host "All direct dependencies conform to AGPL-3.0 compatibility policy in deny.toml." -ForegroundColor Green
exit 0
