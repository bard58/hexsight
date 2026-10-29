# Install Hexsight for the current user (no admin): copy the exe and add shortcuts.
# Usage: install.ps1 -Exe <path to hexsight.exe>
# Uninstall: delete %LOCALAPPDATA%\Programs\Hexsight, the two shortcuts, and %LOCALAPPDATA%\Hexsight.
# The script also removes the install of the old name (see $legacy below).
param([Parameter(Mandatory = $true)][string]$Exe)
$programs = Join-Path $env:LOCALAPPDATA "Programs"
$dir = Join-Path $programs "Hexsight"
# Legacy: the app had another name before Hexsight. Its install folder, its process name,
# and its shortcuts are removed here. Keep this name; it is the only one that may stay.
$legacy = "Augur"
$legacyDir = Join-Path $programs $legacy
New-Item -ItemType Directory -Force $dir | Out-Null
# Close a running copy, but only a copy that was installed under %LOCALAPPDATA%\Programs.
# A copy in another folder (for example the cargo target folder) keeps running.
$names = @("hexsight", $legacy.ToLower())
Get-Process -Name $names -ErrorAction SilentlyContinue |
  Where-Object { $_.Path -and $_.Path.StartsWith($programs + "\", [StringComparison]::OrdinalIgnoreCase) } |
  Stop-Process -Force
Start-Sleep -Milliseconds 300
if (Test-Path -LiteralPath $legacyDir) {
  Remove-Item -Recurse -Force -LiteralPath $legacyDir -ErrorAction SilentlyContinue
  "removed: $legacyDir"
}
$target = Join-Path $dir "Hexsight.exe"
Copy-Item -Force $Exe $target
$shell = New-Object -ComObject WScript.Shell
$folders = @([Environment]::GetFolderPath("Programs"), [Environment]::GetFolderPath("Desktop"))
foreach ($folder in $folders) {
  $old = Join-Path $folder "$legacy.lnk"
  if (Test-Path -LiteralPath $old) {
    Remove-Item -Force -LiteralPath $old
    "removed: $old"
  }
}
foreach ($folder in $folders) {
  $lnk = Join-Path $folder "Hexsight.lnk"
  $s = $shell.CreateShortcut($lnk)
  $s.TargetPath = $target
  $s.WorkingDirectory = $dir
  $s.Description = "Hexsight - ARAM Mayhem Helper"
  $s.IconLocation = "$target,0"
  $s.Save()
  "shortcut: $lnk"
}
"installed: $target ($([math]::Round((Get-Item $target).Length / 1MB, 2)) MB)"
