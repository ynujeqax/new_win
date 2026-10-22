# Measures NewWin while the PC is idle: average CPU and peak private working set
# of the shell and the watchdog, plus the last first-frame time from the log.
# Usage: powershell -File scripts/measure.ps1 -Minutes 10
param(
    [double]$Minutes = 10,
    [int]$IntervalSeconds = 5,
    [string]$Label = ''
)

$ErrorActionPreference = 'Stop'

function Get-Roles {
    $procs = @(Get-CimInstance Win32_Process -Filter "Name='NewWin.exe'")
    $shell = $procs | Where-Object { $_.CommandLine -match '--shell' } | Select-Object -First 1
    $watchdog = $procs | Where-Object { $_.CommandLine -notmatch '--' } | Select-Object -First 1
    if (-not $shell -or -not $watchdog) {
        throw "NewWin is not running (need both the watchdog and the --shell process)"
    }
    [ordered]@{ shell = [int]$shell.ProcessId; watchdog = [int]$watchdog.ProcessId }
}

# CIM perf data is not localized, unlike Get-Counter paths.
function Get-PrivateMb([int]$processId) {
    $row = Get-CimInstance Win32_PerfRawData_PerfProc_Process -Filter "IDProcess=$processId"
    if (-not $row) { return $null }
    [double]$row.WorkingSetPrivate / 1MB
}

$roles = Get-Roles
$cpuStart = @{}
foreach ($role in $roles.Keys) {
    $cpuStart[$role] = (Get-Process -Id $roles[$role]).TotalProcessorTime
}
$maxMb = @{ shell = 0.0; watchdog = 0.0 }
$started = Get-Date
$deadline = $started.AddMinutes($Minutes)
$restarted = $false

if ($Label) { Write-Host "== $Label ==" }
Write-Host ("Measuring for {0} min (shell pid {1}, watchdog pid {2}). Leave the PC idle." -f $Minutes, $roles.shell, $roles.watchdog)
while ((Get-Date) -lt $deadline) {
    Start-Sleep -Seconds $IntervalSeconds
    foreach ($role in $roles.Keys) {
        $mb = Get-PrivateMb $roles[$role]
        if ($null -eq $mb) { $restarted = $true; continue }
        if ($mb -gt $maxMb[$role]) { $maxMb[$role] = $mb }
    }
    if ($restarted) { break }
}

$elapsed = ((Get-Date) - $started).TotalSeconds
$cpus = [Environment]::ProcessorCount
$cpuPct = @{}
foreach ($role in $roles.Keys) {
    $p = Get-Process -Id $roles[$role] -ErrorAction SilentlyContinue
    if (-not $p) { $restarted = $true; $cpuPct[$role] = [double]::NaN; continue }
    $used = ($p.TotalProcessorTime - $cpuStart[$role]).TotalSeconds
    # Same scale as the "% Processor Time" counter divided by the CPU count (Task Manager's scale).
    $cpuPct[$role] = 100.0 * $used / ($elapsed * $cpus)
}

$log = Join-Path $env:LOCALAPPDATA 'NewWin\logs\newwin.log'
$firstFrame = $null
$hit = Select-String -Path $log -Pattern 'first-frame-ms=(\d+)' -ErrorAction SilentlyContinue | Select-Object -Last 1
if ($hit) { $firstFrame = [int]$hit.Matches[0].Groups[1].Value }

$checks = @(
    @{ name = 'shell CPU avg';       value = ('{0:N4} %' -f $cpuPct.shell);      ok = $cpuPct.shell -le 0.05 }
    @{ name = 'shell private WS';    value = ('{0:N1} MB' -f $maxMb.shell);      ok = $maxMb.shell -lt 20 }
    @{ name = 'watchdog CPU avg';    value = ('{0:N4} %' -f $cpuPct.watchdog);   ok = $true }
    @{ name = 'watchdog private WS'; value = ('{0:N1} MB' -f $maxMb.watchdog);   ok = $maxMb.watchdog -lt 2 }
    @{ name = 'first frame';         value = "$firstFrame ms";                   ok = $null -ne $firstFrame -and $firstFrame -lt 300 }
    @{ name = 'no restart';          value = $(if ($restarted) { 'restarted' } else { 'same pids' }); ok = -not $restarted }
)

Write-Host ("Elapsed {0:N0} s, {1} logical CPUs" -f $elapsed, $cpus)
$failed = $false
foreach ($c in $checks) {
    $verdict = if ($c.ok) { 'PASS' } else { $failed = $true; 'FAIL' }
    Write-Host ('{0,-4}  {1,-20} {2}' -f $verdict, $c.name, $c.value)
}
if ($failed) { exit 1 }
