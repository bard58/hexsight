# Measure the memory of Hexsight and Blitz on this PC, for a fair "lighter than" claim.
# Sums all processes of each app (Blitz is Electron and runs several Blitz.exe processes).
# Usage: powershell -NoProfile -File tools\measure_ram.ps1 [-Minutes 5]
# Start both apps first, with the League client open, and keep them on the same screen.
param([int]$Minutes = 5, [string[]]$Apps = @('Hexsight', 'Blitz'))

$samples = @{}
foreach ($a in $Apps) { $samples[$a] = New-Object System.Collections.Generic.List[object] }
$end = (Get-Date).AddMinutes($Minutes)
while ((Get-Date) -lt $end) {
  foreach ($a in $Apps) {
    $ps = @(Get-Process -Name $a -ErrorAction SilentlyContinue)
    if ($ps.Count -gt 0) {
      $samples[$a].Add([pscustomobject]@{
        Count   = $ps.Count
        Private = ($ps | Measure-Object PrivateMemorySize64 -Sum).Sum / 1MB
        Working = ($ps | Measure-Object WorkingSet64 -Sum).Sum / 1MB
      })
    }
  }
  Start-Sleep -Seconds 5
}

"Date: $((Get-Date).ToString('yyyy-MM-dd HH:mm'))  Windows: $([Environment]::OSVersion.Version)  Duration: $Minutes min"
foreach ($a in $Apps) {
  $s = $samples[$a]
  $exe = (Get-Process -Name $a -ErrorAction SilentlyContinue | Select-Object -First 1).Path
  $ver = if ($exe) { (Get-Item $exe).VersionInfo.ProductVersion } else { '?' }
  if ($s.Count -eq 0) { "{0,-9} not running" -f $a; continue }
  "{0,-9} v{1}  processes {2}  private avg {3:N0} MB, max {4:N0} MB  working set avg {5:N0} MB, max {6:N0} MB" -f $a, $ver,
    ($s | Measure-Object Count -Maximum).Maximum,
    ($s | Measure-Object Private -Average).Average, ($s | Measure-Object Private -Maximum).Maximum,
    ($s | Measure-Object Working -Average).Average, ($s | Measure-Object Working -Maximum).Maximum
}
