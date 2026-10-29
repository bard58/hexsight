# Render the app icon (gradient tile with a white "H") at several sizes.
# Usage: make_icon.ps1 <out dir>   -> icon-<size>.png for 16..256
param([string]$OutDir)
Add-Type -AssemblyName System.Drawing
New-Item -ItemType Directory -Force $OutDir | Out-Null
foreach ($s in @(16, 24, 32, 48, 64, 128, 256)) {
  $bmp = [System.Drawing.Bitmap]::new($s, $s, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.SmoothingMode = "AntiAlias"; $g.TextRenderingHint = "AntiAliasGridFit"; $g.InterpolationMode = "HighQualityBicubic"
  $g.Clear([System.Drawing.Color]::Transparent)
  $pad = [math]::Max(1, [math]::Round($s * 0.04))
  $r = [System.Drawing.RectangleF]::new($pad, $pad, $s - 2 * $pad, $s - 2 * $pad)
  $rad = $r.Width * 0.24
  $path = [System.Drawing.Drawing2D.GraphicsPath]::new()
  $d = $rad * 2
  $path.AddArc($r.X, $r.Y, $d, $d, 180, 90)
  $path.AddArc($r.Right - $d, $r.Y, $d, $d, 270, 90)
  $path.AddArc($r.Right - $d, $r.Bottom - $d, $d, $d, 0, 90)
  $path.AddArc($r.X, $r.Bottom - $d, $d, $d, 90, 90)
  $path.CloseFigure()
  $c1 = [System.Drawing.Color]::FromArgb(255, 191, 90, 242)
  $c2 = [System.Drawing.Color]::FromArgb(255, 255, 55, 95)
  $grad = [System.Drawing.Drawing2D.LinearGradientBrush]::new($r, $c1, $c2, 45.0)
  $g.FillPath($grad, $path)
  # Soft top highlight.
  $hl = [System.Drawing.Drawing2D.LinearGradientBrush]::new($r, [System.Drawing.Color]::FromArgb(70, 255, 255, 255), [System.Drawing.Color]::FromArgb(0, 255, 255, 255), 90.0)
  $g.FillPath($hl, $path)
  $fontSize = $s * 0.56
  $font = [System.Drawing.Font]::new("Segoe UI", $fontSize, [System.Drawing.FontStyle]::Bold, [System.Drawing.GraphicsUnit]::Pixel)
  $fmt = [System.Drawing.StringFormat]::new(); $fmt.Alignment = "Center"; $fmt.LineAlignment = "Center"
  $tr = [System.Drawing.RectangleF]::new(0, $s * 0.02, $s, $s)
  $g.DrawString("H", $font, [System.Drawing.Brushes]::White, $tr, $fmt)
  $bmp.Save((Join-Path $OutDir "icon-$s.png"), [System.Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose(); $bmp.Dispose()
}
"icons saved to $OutDir"
