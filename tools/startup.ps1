# Time from process start until the main window exists. Usage: startup.ps1 <exe> [runs]
param([string]$Exe, [int]$Runs = 3)
$times = @()
for ($i = 0; $i -lt $Runs; $i++) {
  Get-Process -Name hexsight -ErrorAction SilentlyContinue | Stop-Process -Force
  Start-Sleep -Milliseconds 500
  $sw = [System.Diagnostics.Stopwatch]::StartNew()
  $p = Start-Process -FilePath $Exe -PassThru
  while ($sw.ElapsedMilliseconds -lt 10000) {
    $p.Refresh()
    if ($p.MainWindowHandle -ne 0) { break }
    Start-Sleep -Milliseconds 10
  }
  $times += $sw.ElapsedMilliseconds
  Stop-Process -Id $p.Id -Force
}
"window after: " + ($times -join " ms, ") + " ms (average " + [math]::Round(($times | Measure-Object -Average).Average) + " ms)"
