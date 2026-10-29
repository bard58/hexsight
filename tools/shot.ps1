# Capture the main window of a process to a PNG with PrintWindow
# (works when the window is covered). Usage: shot.ps1 <processName> <out.png> [screen]
param([string]$Name = "hexsight", [string]$Out = "shot.png", [string]$Mode = "window")
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class W {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
}
"@
[W]::SetProcessDPIAware() | Out-Null
$p = Get-Process -Name $Name -ErrorAction Stop | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if (-not $p) { Write-Error "no window"; exit 1 }
$r = New-Object W+RECT
[W]::GetWindowRect($p.MainWindowHandle, [ref]$r) | Out-Null
$w = $r.R - $r.L; $h = $r.B - $r.T
$bmp = New-Object System.Drawing.Bitmap $w, $h
$g = [System.Drawing.Graphics]::FromImage($bmp)
if ($Mode -eq "screen") {
  $g.CopyFromScreen($r.L, $r.T, 0, 0, $bmp.Size)
} else {
  $hdc = $g.GetHdc()
  [W]::PrintWindow($p.MainWindowHandle, $hdc, 2) | Out-Null
  $g.ReleaseHdc($hdc)
}
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
"saved $Out ${w}x${h} at $($r.L),$($r.T) ws=$([math]::Round($p.WorkingSet64/1MB,1))MB private=$([math]::Round($p.PrivateMemorySize64/1MB,1))MB"
