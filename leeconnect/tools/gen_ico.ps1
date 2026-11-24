Add-Type -AssemblyName System.Drawing

function New-RoundedRectanglePath($rect, $radius) {
    $path = New-Object System.Drawing.Drawing2D.GraphicsPath
    $d = $radius * 2
    $path.AddArc($rect.X, $rect.Y, $d, $d, 180, 90)
    $path.AddArc($rect.Right - $d, $rect.Y, $d, $d, 270, 90)
    $path.AddArc($rect.Right - $d, $rect.Bottom - $d, $d, $d, 0, 90)
    $path.AddArc($rect.X, $rect.Bottom - $d, $d, $d, 90, 90)
    $path.CloseFigure()
    return $path
}

function Generate-Logo($size) {
    $bmp = New-Object System.Drawing.Bitmap $size, $size
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality

    $s = [float]$size
    $pad = $s * 0.05
    $rect = New-Object System.Drawing.RectangleF $pad, $pad, ($s - 2*$pad), ($s - 2*$pad)
    $radius = $s * 0.22

    $cTop = [System.Drawing.Color]::FromArgb(255, 38, 115, 254)   # #2673FE
    $cBot = [System.Drawing.Color]::FromArgb(255, 20, 75, 233)    # #144BE9
    $brush = New-Object System.Drawing.Drawing2D.LinearGradientBrush $rect, $cTop, $cBot, 90.0
    $path = New-RoundedRectanglePath $rect $radius
    $g.FillPath($brush, $path)

    $borderPen = New-Object System.Drawing.Pen ([System.Drawing.Color]::FromArgb(90, 255, 255, 255)), ($s * 0.015)
    $g.DrawPath($borderPen, $path)

    # Monitor Frame
    $penWidth = [Math]::Max(1.0, ($s * 0.065))
    $whitePen = New-Object System.Drawing.Pen @([System.Drawing.Color]::White, [float]$penWidth)
    $whitePen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
    $whitePen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
    $whitePen.LineJoin = [System.Drawing.Drawing2D.LineJoin]::Round

    $screenPadX = $s * 0.22
    $screenPadY = $s * 0.24
    $screenW = $s - 2*$screenPadX
    $screenH = $s * 0.40
    $screenRect = New-Object System.Drawing.RectangleF $screenPadX, $screenPadY, $screenW, $screenH
    $screenPath = New-RoundedRectanglePath $screenRect ($s * 0.05)
    $g.DrawPath($whitePen, $screenPath)

    # Stand and base
    $g.DrawLine($whitePen, [float]($s * 0.5), [float]($screenPadY + $screenH), [float]($s * 0.5), [float]($screenPadY + $screenH + $s * 0.10))
    $g.DrawLine($whitePen, [float]($s * 0.38), [float]($screenPadY + $screenH + $s * 0.10), [float]($s * 0.62), [float]($screenPadY + $screenH + $s * 0.10))

    # Pulse waveform inside monitor
    if ($size -ge 32) {
        $cyanWidth = [Math]::Max(1.0, ($s * 0.04))
        $cyanPen = New-Object System.Drawing.Pen @([System.Drawing.Color]::FromArgb(255, 125, 211, 252), [float]$cyanWidth)
        $cyanPen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
        $cyanPen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round

        $p1 = New-Object System.Drawing.PointF ($s * 0.31), ($screenPadY + $screenH * 0.52)
        $p2 = New-Object System.Drawing.PointF ($s * 0.42), ($screenPadY + $screenH * 0.52)
        $p3 = New-Object System.Drawing.PointF ($s * 0.48), ($screenPadY + $screenH * 0.28)
        $p4 = New-Object System.Drawing.PointF ($s * 0.54), ($screenPadY + $screenH * 0.72)
        $p5 = New-Object System.Drawing.PointF ($s * 0.60), ($screenPadY + $screenH * 0.52)
        $p6 = New-Object System.Drawing.PointF ($s * 0.69), ($screenPadY + $screenH * 0.52)
        $g.DrawLines($cyanPen, [System.Drawing.PointF[]]@($p1, $p2, $p3, $p4, $p5, $p6))
    }

    $g.Dispose()
    return $bmp
}

$sizes = @(256, 48, 32, 16)
$pngBytesList = @()

foreach ($sz in $sizes) {
    $bmp = Generate-Logo $sz
    $ms = New-Object System.IO.MemoryStream
    $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
    $pngBytesList += ,$ms.ToArray()
    $ms.Dispose()
    $bmp.Dispose()
}

$icoPath = "E:\code\server-manga-log\leeconnect\assets\leeconnect.ico"
$dir = [System.IO.Path]::GetDirectoryName($icoPath)
if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Force -Path $dir | Out-Null }

$fs = [System.IO.File]::Create($icoPath)
$bw = New-Object System.IO.BinaryWriter $fs

# Header: reserved (0), type (1), count
$bw.Write([uint16]0)
$bw.Write([uint16]1)
$bw.Write([uint16]$sizes.Count)

$offset = 6 + ($sizes.Count * 16)

for ($i = 0; $i -lt $sizes.Count; $i++) {
    $sz = $sizes[$i]
    $data = $pngBytesList[$i]
    $w = if ($sz -ge 256) { [byte]0 } else { [byte]$sz }
    $h = if ($sz -ge 256) { [byte]0 } else { [byte]$sz }

    $bw.Write([byte]$w)
    $bw.Write([byte]$h)
    $bw.Write([byte]0) # colors
    $bw.Write([byte]0) # reserved
    $bw.Write([uint16]1) # planes
    $bw.Write([uint16]32) # bpp
    $bw.Write([uint32]$data.Length)
    $bw.Write([uint32]$offset)

    $offset += $data.Length
}

foreach ($data in $pngBytesList) {
    $bw.Write($data)
}

$bw.Flush()
$bw.Dispose()
$fs.Dispose()

Write-Host "Generated $icoPath ($($sizes.Count) resolutions)"
