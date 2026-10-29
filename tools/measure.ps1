# Measure memory and idle CPU of process groups over a window.
# Usage: measure.ps1 -Seconds 30 -Names hexsight
param([int]$Seconds = 30, [string[]]$Names = @("hexsight"))
$Names = @($Names | ForEach-Object { $_ -split "," } | Where-Object { $_ })
$cores = [Environment]::ProcessorCount
function Snap($name) {
  $ps = @(Get-Process -Name $name -ErrorAction SilentlyContinue)
  [pscustomobject]@{
    Name = $name; Count = $ps.Count
    WS = ($ps | Measure-Object WorkingSet64 -Sum).Sum
    Priv = ($ps | Measure-Object PrivateMemorySize64 -Sum).Sum
    Cpu = ($ps | ForEach-Object { $_.TotalProcessorTime.TotalMilliseconds } | Measure-Object -Sum).Sum
    Threads = ($ps | ForEach-Object { $_.Threads.Count } | Measure-Object -Sum).Sum
  }
}
$a = @{}; foreach ($n in $Names) { $a[$n] = Snap $n }
Start-Sleep -Seconds $Seconds
foreach ($n in $Names) {
  $b = Snap $n
  $cpuPct = if ($b.Count -gt 0) { [math]::Round(($b.Cpu - $a[$n].Cpu) / ($Seconds * 1000) / $cores * 100, 2) } else { 0 }
  "{0,-6} procs={1} ws={2:N1}MB private={3:N1}MB threads={4} cpu={5}% of all cores over {6}s" -f $n, $b.Count, ($b.WS / 1MB), ($b.Priv / 1MB), $b.Threads, $cpuPct, $Seconds
}
