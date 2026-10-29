# DRAFT. Do not submit before the first release exists.
# The package does not embed the exe. It downloads it from the GitHub release and checks the hash.
$ErrorActionPreference = 'Stop'
$toolsDir = Split-Path -Parent $MyInvocation.MyCommand.Definition

$packageArgs = @{
  packageName    = 'hexsight'
  fileFullPath   = Join-Path $toolsDir 'hexsight.exe'
  url64bit       = 'https://github.com/bard58/hexsight/releases/download/v0.1.0/hexsight-v0.1.0-windows-x64.exe'
  checksum64     = 'REPLACE-WITH-SHA256'
  checksumType64 = 'sha256'
}

Get-ChocolateyWebFile @packageArgs

# Hexsight is a GUI app. An empty "hexsight.exe.gui" file tells Chocolatey to make a GUI shim,
# so no console window opens. Chocolatey reads this file name from the tools folder.
New-Item -ItemType File -Force -Path (Join-Path $toolsDir 'hexsight.exe.gui') | Out-Null
