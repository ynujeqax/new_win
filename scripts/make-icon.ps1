# Draws assets\newwin.ico: a white capsule outline (the bar) on a transparent background,
# one PNG frame per size. Run from the repository root: .\scripts\make-icon.ps1
param([string]$Out = "assets\newwin.ico")

Add-Type -AssemblyName System.Drawing

$sizes = 16, 20, 24, 32, 40, 48, 64, 256
$frames = foreach ($size in $sizes) {
    $bmp = New-Object System.Drawing.Bitmap $size, $size, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $g.Clear([System.Drawing.Color]::Transparent)

    $stroke = [Math]::Max(1, [Math]::Round($size / 16))
    $w = $size * 0.8
    $h = $size * 0.36
    $x = ($size - $w) / 2
    $y = ($size - $h) / 2
    # The pen is centred on the path: inset by half the stroke so it stays inside the frame.
    $inset = $stroke / 2
    $rx = $x + $inset; $ry = $y + $inset; $rw = $w - $stroke; $rh = $h - $stroke
    $d = $rh
    $path = New-Object System.Drawing.Drawing2D.GraphicsPath
    $path.AddArc($rx, $ry, $d, $d, 90, 180)
    $path.AddArc($rx + $rw - $d, $ry, $d, $d, 270, 180)
    $path.CloseFigure()
    $pen = New-Object System.Drawing.Pen ([System.Drawing.Color]::White), $stroke
    $g.DrawPath($pen, $path)
    $pen.Dispose(); $path.Dispose(); $g.Dispose()

    $ms = New-Object System.IO.MemoryStream
    if ($size -ge 256) {
        # Vista+ icons store the large frame as PNG.
        $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
    } else {
        # Small frames as a 32-bit DIB, which every icon API reads: BITMAPINFOHEADER with a
        # doubled height, bottom-up BGRA rows, then an all-zero AND mask (alpha does the work).
        $w8 = New-Object System.IO.BinaryWriter $ms
        $w8.Write([uint32]40); $w8.Write([int32]$size); $w8.Write([int32]($size * 2))
        $w8.Write([uint16]1); $w8.Write([uint16]32); $w8.Write([uint32]0)
        $maskRow = [Math]::Ceiling($size / 32) * 4
        $w8.Write([uint32]($size * $size * 4 + $maskRow * $size))
        $w8.Write([int32]0); $w8.Write([int32]0); $w8.Write([uint32]0); $w8.Write([uint32]0)
        for ($row = $size - 1; $row -ge 0; $row--) {
            for ($col = 0; $col -lt $size; $col++) {
                $c = $bmp.GetPixel($col, $row)
                $w8.Write([byte]$c.B); $w8.Write([byte]$c.G); $w8.Write([byte]$c.R); $w8.Write([byte]$c.A)
            }
        }
        $w8.Write((New-Object byte[] ($maskRow * $size)))
        $w8.Flush()
    }
    $bmp.Dispose()
    [pscustomobject]@{ Size = $size; Bytes = $ms.ToArray() }
}

# ICO: ICONDIR (6 bytes), one ICONDIRENTRY (16 bytes) per frame, then the PNG data.
$file = New-Object System.IO.MemoryStream
$bw = New-Object System.IO.BinaryWriter $file
$bw.Write([uint16]0); $bw.Write([uint16]1); $bw.Write([uint16]$frames.Count)
$offset = 6 + 16 * $frames.Count
foreach ($f in $frames) {
    $dim = if ($f.Size -ge 256) { 0 } else { $f.Size }
    $bw.Write([byte]$dim); $bw.Write([byte]$dim); $bw.Write([byte]0); $bw.Write([byte]0)
    $bw.Write([uint16]1); $bw.Write([uint16]32)
    $bw.Write([uint32]$f.Bytes.Length); $bw.Write([uint32]$offset)
    $offset += $f.Bytes.Length
}
foreach ($f in $frames) { $bw.Write($f.Bytes) }
$bw.Flush()
[System.IO.File]::WriteAllBytes((Join-Path (Get-Location) $Out), $file.ToArray())
Write-Output "wrote $Out ($($frames.Count) sizes, $($file.Length) bytes)"
