# A full-width window of moving color stripes (a stand-in for a playing video), for measuring the
# glass: run it under the bar (-Top 0) or elsewhere on the screen (-Top 600).
# Usage: powershell -File scripts/anim-window.ps1 -Top 0 -Seconds 120
param(
    [int]$Top = 0,
    [int]$Height = 140,
    [int]$Seconds = 120
)

Add-Type -AssemblyName System.Windows.Forms, System.Drawing
$screen = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
$form = New-Object System.Windows.Forms.Form
$form.FormBorderStyle = 'None'
$form.StartPosition = 'Manual'
$form.ShowInTaskbar = $false
$form.Bounds = New-Object System.Drawing.Rectangle(0, $Top, $screen.Width, $Height)
$colors = @('#2E57E4', '#12A7F3', '#5C3329', '#EBC05B', '#FFFFFF', '#101010') | ForEach-Object { [System.Drawing.ColorTranslator]::FromHtml($_) }
$brushes = $colors | ForEach-Object { New-Object System.Drawing.SolidBrush $_ }
$script:phase = 0
$form.Add_Paint({
    param($s, $e)
    $x = - ($script:phase % 360)
    $i = 0
    while ($x -lt $form.ClientSize.Width) {
        $e.Graphics.FillRectangle($brushes[$i % $brushes.Count], $x, 0, 60, $form.ClientSize.Height)
        $x += 60; $i++
    }
})
$timer = New-Object System.Windows.Forms.Timer
$timer.Interval = 16
$timer.Add_Tick({ $script:phase += 6; $form.Invalidate() })
$stop = New-Object System.Windows.Forms.Timer
$stop.Interval = $Seconds * 1000
$stop.Add_Tick({ $form.Close() })
$timer.Start(); $stop.Start()
[System.Windows.Forms.Application]::Run($form)
