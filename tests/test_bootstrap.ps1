param([string]$PackagePath = '')
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$env:PYTHONIOENCODING = 'utf-8'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)
. (Join-Path $PSScriptRoot '../src-tauri/resources/bootstrap/bootstrap-python.ps1')

$Root = Join-Path $PSScriptRoot ('../src-tauri/target/test-rife-é space-' + [guid]::NewGuid().ToString('N'))
$Root = [IO.Path]::GetFullPath($Root)
$UserPath = [Environment]::GetEnvironmentVariable('PATH', 'User')
$RegisteredPython = Get-ItemProperty 'HKCU:\Software\Python\PythonCore\3.11\InstallPath' -ErrorAction SilentlyContinue | ConvertTo-Json
try {
  $Python = Initialize-RifePython -RuntimeRoot $Root -PackagePath $PackagePath
  if (-not (Test-RifePython $Python)) { throw 'The private Python runtime is not working.' }
  $Archive = Join-Path $Root '.download-staging/python-3.11.9.zip'
  $OriginalHash = (Get-FileHash -LiteralPath $Python).Hash
  $Reused = Initialize-RifePython -RuntimeRoot $Root -PackagePath 'missing-package.zip'
  if ($Reused -ne $Python -or (Get-FileHash -LiteralPath $Python).Hash -ne $OriginalHash) {
    throw 'A valid Python runtime must be reused without downloading or replacing it.'
  }
  $Venv = Join-Path $Root 'venv'
  & $Python -I -m venv $Venv
  if ($LASTEXITCODE -ne 0) { throw 'Private virtual environment creation failed.' }
  $VenvPython = Join-Path $Venv 'Scripts/python.exe'
  & $VenvPython -I -c "import sys,pip,ssl; assert sys.prefix != sys.base_prefix; print('Private Python + venv + pip + SSL: OK')"
  if ($LASTEXITCODE -ne 0) { throw 'The virtual environment is unusable.' }

  # Repair an incomplete installation using the verified package.
  Remove-Item -LiteralPath (Join-Path $Root 'python/python311.dll')
  if (Test-RifePython $Python) { throw 'Validation must reject an incomplete Python runtime.' }
  $RepairPackage = Join-Path $Root 'verified-python.zip'
  Copy-Item -LiteralPath $Archive -Destination $RepairPackage
  $Repaired = Initialize-RifePython -RuntimeRoot $Root -PackagePath $RepairPackage
  if (-not (Test-RifePython $Repaired)) { throw 'The incomplete Python runtime was not repaired.' }
  & $VenvPython -I -c 'import pip,ssl'
  if ($LASTEXITCODE -ne 0) { throw 'The repair broke the existing virtual environment.' }

  $InvalidRoot = Join-Path $Root 'invalid-package'
  $InvalidPackage = Join-Path $Root 'invalid.zip'
  [IO.File]::WriteAllText($InvalidPackage, 'invalid content')
  $Rejected = $false
  try { Initialize-RifePython -RuntimeRoot $InvalidRoot -PackagePath $InvalidPackage | Out-Null }
  catch { $Rejected = $_.Exception.Message.Contains('SHA-256') }
  if (-not $Rejected -or (Test-Path -LiteralPath (Join-Path $InvalidRoot 'python'))) {
    throw 'A package with an incorrect hash must be rejected before extraction.'
  }
  $Rejected = $false
  try { Remove-RifeDirectory $PSScriptRoot $Root } catch { $Rejected = $true }
  if (-not $Rejected) { throw 'Deletion outside the RIFE runtime directory must be rejected.' }
  if ([Environment]::GetEnvironmentVariable('PATH', 'User') -ne $UserPath) { throw 'The user PATH was modified.' }
  $CurrentRegisteredPython = Get-ItemProperty 'HKCU:\Software\Python\PythonCore\3.11\InstallPath' -ErrorAction SilentlyContinue | ConvertTo-Json
  if ($CurrentRegisteredPython -ne $RegisteredPython) { throw 'The registered Python runtime was modified.' }
  Write-Output 'Isolated installation, reuse, repair, integrity, paths with spaces/accents, and unchanged registry/PATH: OK'
} finally {
  Remove-RifeDirectory $Root ([IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../src-tauri/target')))
}
