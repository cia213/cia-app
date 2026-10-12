# The official NuGet package provides a complete Python runtime without an MSI
# installation or changes to the registry, PATH, or an existing Python runtime.
$RifePythonVersion = '3.11.9'
$RifePythonPackageUrl = 'https://api.nuget.org/v3-flatcontainer/python/3.11.9/python.3.11.9.nupkg'
$RifePythonPackageHash = '9283876D58C017E0E846F95B490DA3BCA0FC0A6EE1134B2870677CFB7EEC3C67'

function Remove-RifeDirectory([string]$Path, [string]$RuntimeRoot) {
  $Root = [System.IO.Path]::GetFullPath($RuntimeRoot).TrimEnd('\') + '\'
  $Target = [System.IO.Path]::GetFullPath($Path)
  if (-not $Target.StartsWith($Root, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw 'The directory to remove must remain inside the application RIFE runtime directory.'
  }
  if (Test-Path -LiteralPath $Target) {
    Remove-Item -LiteralPath $Target -Recurse -Force
  }
}

function Test-RifePython([string]$Executable) {
  if (-not (Test-Path -LiteralPath $Executable -PathType Leaf)) { return $false }
  $HomeDirectory = Split-Path -Parent $Executable
  foreach ($Required in @('python311.dll', 'Lib/venv/__init__.py', 'Lib/ensurepip/__init__.py')) {
    if (-not (Test-Path -LiteralPath (Join-Path $HomeDirectory $Required) -PathType Leaf)) { return $false }
  }
  try {
    & $Executable -I -c "import sys,struct,ssl,venv,ensurepip; assert sys.version_info[:3] == (3,11,9); assert struct.calcsize('P') == 8" 2>$null
    return $LASTEXITCODE -eq 0
  } catch { return $false }
}

function Initialize-RifePython([string]$RuntimeRoot, [string]$PackagePath = '') {
  $PythonHome = Join-Path $RuntimeRoot 'python'
  $BasePython = Join-Path $PythonHome 'python.exe'
  if (Test-RifePython $BasePython) { return $BasePython }

  $Staging = Join-Path $RuntimeRoot '.download-staging'
  New-Item -ItemType Directory -Force -Path $Staging | Out-Null
  $Archive = Join-Path $Staging 'python-3.11.9.zip'
  if ($PackagePath) {
    Copy-Item -LiteralPath $PackagePath -Destination $Archive -Force
  } else {
    [Console]::WriteLine('cia render SETUP: Downloading the private Python 3.11.9 runtime...')
    Invoke-WebRequest -UseBasicParsing -Uri $RifePythonPackageUrl -OutFile $Archive
  }
  if ((Get-FileHash -LiteralPath $Archive -Algorithm SHA256).Hash -ne $RifePythonPackageHash) {
    throw 'The Python package SHA-256 hash does not match the expected official release.'
  }

  $Extract = Join-Path $Staging 'python-extract'
  Remove-RifeDirectory $Extract $RuntimeRoot
  Expand-Archive -LiteralPath $Archive -DestinationPath $Extract -Force
  $Tools = Join-Path $Extract 'tools'
  if (-not (Test-RifePython (Join-Path $Tools 'python.exe'))) {
    throw 'The extracted Python package is incomplete or unusable (Python 3.11.9 x64 required).'
  }

  # Replace the existing runtime only after fully validating the new Python.
  $Backup = Join-Path $Staging ('python-previous-' + [guid]::NewGuid().ToString('N'))
  if (Test-Path -LiteralPath $PythonHome) {
    Move-Item -LiteralPath $PythonHome -Destination $Backup
  }
  try {
    Move-Item -LiteralPath $Tools -Destination $PythonHome
    if (-not (Test-RifePython $BasePython)) { throw 'The private Python runtime does not start after extraction.' }
  } catch {
    Remove-RifeDirectory $PythonHome $RuntimeRoot
    if (Test-Path -LiteralPath $Backup) {
      Move-Item -LiteralPath $Backup -Destination $PythonHome
    }
    throw
  }
  Remove-RifeDirectory $Backup $RuntimeRoot
  Remove-RifeDirectory $Extract $RuntimeRoot
  return $BasePython
}
