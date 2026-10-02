#!/usr/bin/env pwsh
# Regenerates THIRD_PARTY_NOTICES.md at the repo root.
#
#   powershell -ExecutionPolicy Bypass -File tools/gen-third-party-notices.ps1
#
# Step 1: `cargo about` enumerates every crate linked into the Windows release
#         binary and reproduces each crate's own license file verbatim.
# Step 2: cargo-about cannot resolve the non-SPDX id `LicenseRef-UFL-1.0`
#         (the Ubuntu Font shipped by epaint_default_fonts) to a license file,
#         so it fills that section with the wrong text. We overwrite that one
#         section with the real `fonts/UFL.txt` from the epaint_default_fonts
#         source package, checked against a pinned SHA-256.
#
# Requires: cargo-about  (`cargo install cargo-about --locked --features cli`)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$out = Join-Path $root 'THIRD_PARTY_NOTICES.md'
cargo about generate about.hbs --output-file $out
if ($LASTEXITCODE -ne 0) { throw "cargo about failed ($LASTEXITCODE)" }

# --- locate fonts/UFL.txt in the epaint_default_fonts source package ----------
$uflPinnedSha = '2f0015108d68627bd788d313f529c21ff4da2c2c42a5e1f3883acc83480f9002'
$srcRoots = Get-ChildItem "$env:USERPROFILE\.cargo\registry\src" -Directory -ErrorAction Stop
$uflFile = $srcRoots |
    ForEach-Object { Get-ChildItem $_.FullName -Directory -Filter 'epaint_default_fonts-*' } |
    ForEach-Object { Join-Path $_.FullName 'fonts/UFL.txt' } |
    Where-Object { Test-Path $_ } |
    Select-Object -First 1
if (-not $uflFile) { throw 'fonts/UFL.txt not found in the cargo registry source cache' }

$uflSha = (Get-FileHash $uflFile -Algorithm SHA256).Hash.ToLower()
if ($uflSha -ne $uflPinnedSha) {
    throw "fonts/UFL.txt SHA-256 changed: $uflSha (expected $uflPinnedSha). Bump the pin after reviewing the new text."
}
$uflText = (Get-Content $uflFile -Raw).TrimEnd() + "`n"

# --- insert / overwrite the LicenseRef-UFL-1.0 section ----------------------
$lines = @(Get-Content $out)

$section = @(
    '### Ubuntu Font Licence 1.0 (SPDX: `LicenseRef-UFL-1.0`)'
    ''
    'Applies to:'
    ''
    '- epaint_default_fonts 0.29.1'
    ''
    '```text'
) + ($uflText -split "`n") + @('```')

$startHdr = ($lines | Select-String -Pattern '^### .*`LicenseRef-UFL-1\.0`' | Select-Object -First 1).LineNumber

if ($startHdr) {
    # cargo-about emitted a (wrong-text) section: replace its whole body.
    $start = $startHdr - 1
    $end = $lines.Length - 1
    for ($i = $start + 1; $i -lt $lines.Length; $i++) {
        if ($lines[$i] -match '^### ') { $end = $i - 1; break }
    }
    $new = @()
    if ($start -gt 0) { $new += $lines[0..($start - 1)] }
    $new += $section
    if ($end -lt $lines.Length - 1) { $new += $lines[($end + 1)..($lines.Length - 1)] }
    $note = 'replaced'
}
else {
    # cargo-about skipped it entirely: append after the last license section.
    $new = $lines + @('') + $section + @('')
    $note = 'appended'
}

Set-Content -Path $out -Value $new -Encoding utf8
Write-Host "Wrote $out ($($new.Length) lines); UFL section $note from $uflFile"
