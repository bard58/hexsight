# Draw a synthetic 1920x1080 augment pick screen for OCR tests.
# Usage: fake_pick.ps1 <out.png> "<name 1>" "<name 2>" "<name 3>" [fontFamily]
param([string]$Out, [string]$N1, [string]$N2, [string]$N3, [string]$Font = "Georgia", [int]$Size = 22)
Add-Type -AssemblyName System.Drawing
$w = 1920; $h = 1080
$bmp = New-Object System.Drawing.Bitmap $w, $h
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.SmoothingMode = "AntiAlias"; $g.TextRenderingHint = "AntiAliasGridFit"
# Busy game-like background: gradient plus random blobs.
$bg = New-Object System.Drawing.Drawing2D.LinearGradientBrush((New-Object System.Drawing.Point 0,0), (New-Object System.Drawing.Point $w,$h), [System.Drawing.Color]::FromArgb(255,20,40,60), [System.Drawing.Color]::FromArgb(255,70,90,110))
$g.FillRectangle($bg, 0, 0, $w, $h)
$rnd = New-Object System.Random 7
for ($i = 0; $i -lt 160; $i++) {
  $col = [System.Drawing.Color]::FromArgb(60, $rnd.Next(255), $rnd.Next(255), $rnd.Next(255))
  $g.FillEllipse([System.Drawing.SolidBrush]::new($col), $rnd.Next($w), $rnd.Next($h), $rnd.Next(20,160), $rnd.Next(20,160))
}
# Dim layer like the game does behind the cards.
$g.FillRectangle((New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(150,0,0,0))), 0, 0, $w, $h)
$titleFont = New-Object System.Drawing.Font($Font, $Size, [System.Drawing.FontStyle]::Bold)
$descFont = New-Object System.Drawing.Font("Segoe UI", 13)
$names = @($N1, $N2, $N3)
$xs = @(530, 960, 1390)
for ($k = 0; $k -lt 3; $k++) {
  $cx = $xs[$k]; $cw = 360; $ch = 560; $x0 = $cx - $cw / 2; $y0 = 250
  $g.FillRectangle((New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(235,28,22,48))), $x0, $y0, $cw, $ch)
  $g.DrawRectangle((New-Object System.Drawing.Pen ([System.Drawing.Color]::FromArgb(255,200,160,90)), 3), $x0, $y0, $cw, $ch)
  $g.FillEllipse((New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(255,90,60,150))), $cx - 70, $y0 + 40, 140, 140)
  $fmt = New-Object System.Drawing.StringFormat; $fmt.Alignment = "Center"
  $g.DrawString($names[$k], $titleFont, [System.Drawing.Brushes]::White, (New-Object System.Drawing.RectangleF ($x0 + 10), ($y0 + 210), ($cw - 20), 80), $fmt)
  $desc = "Hasar verdiginde 6 saniyeligine yuk kazanirsin. Her 10 yukte bir ekstra dayaniklilik ve beceri hizi elde edersin."
  $g.DrawString($desc, $descFont, (New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(255,190,190,200))), (New-Object System.Drawing.RectangleF ($x0 + 24), ($y0 + 300), ($cw - 48), 200), $fmt)
}
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
"saved $Out"
