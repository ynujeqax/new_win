# Builds target\installer\NewWin-Setup-<version>.exe: release build, then Inno Setup.
# Run from the repository root: .\scripts\build-installer.ps1
# No $ErrorActionPreference = "Stop": Windows PowerShell 5 would stop on cargo's progress lines on
# stderr. Every native step checks $LASTEXITCODE instead.
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$iscc = @(
    (Get-Command ISCC.exe -ErrorAction SilentlyContinue | ForEach-Object Source),
    "$env:LOCALAPPDATA\Programs\Inno Setup 7\ISCC.exe",
    "$env:ProgramFiles\Inno Setup 7\ISCC.exe",
    "${env:ProgramFiles(x86)}\Inno Setup 7\ISCC.exe"
) | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
if (-not $iscc) { throw "Inno Setup 7 (ISCC.exe) not found. Install it from https://jrsoftware.org/isdl.php" }

# The exe files are replaced by the build: a running NewWin from target\release has to go first.
$newwin = Join-Path $root "target\release\NewWin.exe"
$wasRunning = [bool](Get-Process NewWin -ErrorAction SilentlyContinue | Where-Object Path -eq $newwin)
if (Test-Path $newwin) { & $newwin --quit | Out-Null }

try {
    $env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

    $version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
    & $iscc "/DAppVersion=$version" installer\newwin.iss
    if ($LASTEXITCODE -ne 0) { throw "ISCC failed" }
    Write-Output (Join-Path $root "target\installer\NewWin-Setup-$version.exe")
} finally {
    # Back to how it was, also after a failed build.
    if ($wasRunning -and (Test-Path $newwin)) { Start-Process $newwin }
}
