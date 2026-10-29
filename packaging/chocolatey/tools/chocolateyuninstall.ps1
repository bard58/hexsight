# DRAFT. Chocolatey removes the files and the shim by itself.
# Hexsight data lives in %LOCALAPPDATA%\Hexsight. This script does not delete it.
# The user can delete that folder (see PRIVACY.md).
$ErrorActionPreference = 'Stop'
Get-Process -Name hexsight -ErrorAction SilentlyContinue | Stop-Process -Force
