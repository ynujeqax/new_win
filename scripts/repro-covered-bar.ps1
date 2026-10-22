# Repro for "the bar stays shown by itself": a topmost window (like Screen Snipping) covers the
# bar while the pointer is on it, then goes away while the pointer moves down.
# Prints the visibility state NewWin logged at the end; "Hidden" means the bar hid as it should.
Add-Type -AssemblyName System.Windows.Forms
Add-Type -Namespace Repro -Name U -MemberDefinition '[DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);'
$log = Join-Path $env:LOCALAPPDATA 'NewWin\logs\newwin.log'
$screen = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds

[Repro.U]::SetCursorPos($screen.Width / 2, 0) | Out-Null
Start-Sleep -Milliseconds 200
[Repro.U]::SetCursorPos($screen.Width / 2, 15) | Out-Null
Start-Sleep -Milliseconds 600

$cover = New-Object System.Windows.Forms.Form
$cover.FormBorderStyle = 'None'
$cover.StartPosition = 'Manual'
$cover.TopMost = $true
$cover.ShowInTaskbar = $false
$cover.Opacity = 0.3
$cover.Bounds = New-Object System.Drawing.Rectangle(0, 0, $screen.Width, 200)
$cover.Show()
[System.Windows.Forms.Application]::DoEvents()
Start-Sleep -Milliseconds 400
# The pointer leaves while the cover is still up, as when finishing a screen snip.
[Repro.U]::SetCursorPos($screen.Width / 2, $screen.Height / 2) | Out-Null
[System.Windows.Forms.Application]::DoEvents()
Start-Sleep -Milliseconds 200
$cover.Close()
[System.Windows.Forms.Application]::DoEvents()
Start-Sleep -Milliseconds 1500

$last = Select-String -Path $log -Pattern 'visibility .* -> (\w+)' | Select-Object -Last 1
$state = $last.Matches[0].Groups[1].Value
"final visibility state: $state"
if ($state -ne 'Hidden') { exit 1 }
