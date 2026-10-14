# The official NuGet package provides a complete Python runtime without an MSI
# installation or changes to the registry, PATH, or an existing Python runtime.
$RifePythonVersion = '3.11.9'
$RifePythonPackageUrl = 'https://api.nuget.org/v3-flatcontainer/python/3.11.9/python.3.11.9.nupkg'
$RifePythonPackageHash = '9283876D58C017E0E846F95B490DA3BCA0FC0A6EE1134B2870677CFB7EEC3C67'

# A desktop app can inherit PSModulePath from PowerShell 7 or an IDE. Resolve
# Windows PowerShell's own modules explicitly instead of depending on that path.
foreach ($Module in @('Microsoft.PowerShell.Utility', 'Microsoft.PowerShell.Archive')) {
  Import-Module -Name (Join-Path $PSHOME ('Modules/' + $Module)) -ErrorAction Stop
}

function Remove-RifeDirectory([string]$Path, [string]$RuntimeRoot) {
  $Root = [System.IO.Path]::GetFullPath($RuntimeRoot).TrimEnd('\') + '\'
  $Target = [System.IO.Path]::GetFullPath($Path)
  if (-not $Target.StartsWith($Root, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw 'The directory to remove must remain inside the application RIFE runtime directory.'
  }
  if (Test-Path -LiteralPath $Target) {
    if ((Get-Item -LiteralPath $Target -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) {
      throw 'Runtime cleanup refused a directory junction or symbolic link.'
    }
    Remove-Item -LiteralPath $Target -Recurse -Force
  }
}

function Get-RifeArtifact([string]$Url, [string]$Destination, [string]$Hash, [string]$Label) {
  if ((Test-Path -LiteralPath $Destination -PathType Leaf) -and
      (Get-FileHash -LiteralPath $Destination -Algorithm SHA256).Hash -eq $Hash) {
    [Console]::WriteLine("cia render SETUP: Reusing verified $Label.")
    return $Destination
  }
  if (Test-Path -LiteralPath $Destination) { Remove-Item -LiteralPath $Destination -Force }
  New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Destination) | Out-Null
  $Partial = "$Destination.part"
  # Cancellation may happen after the final byte but before promotion. Reuse a
  # complete verified partial instead of asking the server for an invalid range.
  if ((Test-Path -LiteralPath $Partial -PathType Leaf) -and
      (Get-FileHash -LiteralPath $Partial -Algorithm SHA256).Hash -eq $Hash) {
    Move-Item -LiteralPath $Partial -Destination $Destination -Force
    return $Destination
  }
  for ($Attempt = 1; $Attempt -le 3; $Attempt++) {
    $Response = $null; $InputStream = $null; $OutputStream = $null
    try {
      $Offset = if (Test-Path -LiteralPath $Partial) { (Get-Item -LiteralPath $Partial).Length } else { 0 }
      $Request = [Net.HttpWebRequest]::Create($Url)
      $Request.Timeout = 30000
      $Request.ReadWriteTimeout = 45000
      $Request.UserAgent = 'cia-render/1.3 runtime installer'
      if ($Offset -gt 0) { $Request.AddRange([long]$Offset) }
      $Response = $Request.GetResponse()
      if ($Offset -gt 0 -and [int]$Response.StatusCode -ne 206) { $Offset = 0 }
      if ($Offset -gt 0 -and -not $Response.Headers['Content-Range'].StartsWith("bytes $Offset-")) {
        throw 'The download server returned an invalid resume range.'
      }
      $Mode = if ($Offset -gt 0) { [IO.FileMode]::Append } else { [IO.FileMode]::Create }
      $OutputStream = [IO.File]::Open($Partial, $Mode, [IO.FileAccess]::Write, [IO.FileShare]::None)
      $InputStream = $Response.GetResponseStream()
      $Buffer = New-Object byte[] (1024 * 1024)
      $Bytes = [long]$Offset
      $Total = if ($Response.ContentLength -gt 0) { $Offset + $Response.ContentLength } else { 0 }
      $LastProgress = [DateTime]::UtcNow
      [Console]::WriteLine("CIA_DOWNLOAD bytes=$Bytes total=$Total label=$Label")
      while (($Read = $InputStream.Read($Buffer, 0, $Buffer.Length)) -gt 0) {
        $OutputStream.Write($Buffer, 0, $Read)
        $Bytes += $Read
        if (([DateTime]::UtcNow - $LastProgress).TotalSeconds -ge 2) {
          [Console]::WriteLine("CIA_DOWNLOAD bytes=$Bytes total=$Total label=$Label")
          $LastProgress = [DateTime]::UtcNow
        }
      }
      $OutputStream.Flush($true)
      $OutputStream.Dispose(); $OutputStream = $null
      if ((Get-FileHash -LiteralPath $Partial -Algorithm SHA256).Hash -ne $Hash) {
        Remove-Item -LiteralPath $Partial -Force
        throw "The $Label SHA-256 check failed. The artifact was rejected."
      }
      Move-Item -LiteralPath $Partial -Destination $Destination -Force
      [Console]::WriteLine("CIA_DOWNLOAD bytes=$Bytes total=$Bytes label=$Label")
      return $Destination
    } catch {
      if ($_.Exception.Response -and [int]$_.Exception.Response.StatusCode -eq 416 -and
          (Test-Path -LiteralPath $Partial)) {
        Remove-Item -LiteralPath $Partial -Force
      }
      if ($Attempt -eq 3) { throw "Unable to download $Label after 3 attempts: $($_.Exception.Message)" }
      [Console]::WriteLine("cia render SETUP: Retrying $Label ($Attempt/3)...")
      Start-Sleep -Seconds $Attempt
    } finally {
      if ($OutputStream) { $OutputStream.Dispose() }
      if ($InputStream) { $InputStream.Dispose() }
      if ($Response) { $Response.Dispose() }
    }
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
    Get-RifeArtifact $RifePythonPackageUrl $Archive $RifePythonPackageHash 'Python 3.11.9' | Out-Null
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
