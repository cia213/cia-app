param(
  [Parameter(Mandatory = $true)][string]$RuntimeRoot,
  [string]$HealthScript = (Join-Path $PSScriptRoot '../runtime_health.py'),
  [switch]$ForceRepair
)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)
$OutputEncoding = [Console]::OutputEncoding
$env:PYTHONUTF8 = '1'
$env:PYTHONNOUSERSITE = '1'
Remove-Item Env:PYTHONHOME -ErrorAction SilentlyContinue
Remove-Item Env:PYTHONPATH -ErrorAction SilentlyContinue
# Artifact selection is independent of user pip.ini and PIP_* variables. The
# Windows HTTP transport still supports the machine's proxy/certificate setup.
Get-ChildItem Env: | Where-Object { $_.Name.StartsWith('PIP_') } | ForEach-Object { Remove-Item -LiteralPath ('Env:' + $_.Name) }
$env:PIP_CONFIG_FILE = 'NUL'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$InstallLock = $null

function Write-Step([int]$Step, [string]$Label) { [Console]::WriteLine("CIA_PROGRESS step=$Step total=8 label=$Label") }
function Assert-ExitCode([string]$Step) {
  if ($LASTEXITCODE -ne 0) { throw "$Step failed with exit code $LASTEXITCODE. Review the installation log." }
}
function Resolve-ManagedPath([string]$Relative) {
  if (-not $Relative -or [IO.Path]::IsPathRooted($Relative) -or $Relative.Split('/\') -contains '..') {
    throw 'The active runtime pointer contains an invalid relative path.'
  }
  $Path = [IO.Path]::GetFullPath((Join-Path $RuntimeRoot $Relative))
  if (-not $Path.StartsWith($RuntimeRoot.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase)) {
    throw 'The active runtime pointer points outside the managed directory.'
  }
  return $Path
}
function Invoke-RifeHealth([string]$Python, [string]$Project, [string]$Model, [string]$Manifest, [int]$TimeoutMilliseconds = 90000) {
  $Process = New-Object Diagnostics.Process
  $Process.StartInfo.FileName = $Python
  $Process.StartInfo.UseShellExecute = $false
  $Process.StartInfo.CreateNoWindow = $true
  $Process.StartInfo.RedirectStandardOutput = $true
  $Process.StartInfo.RedirectStandardError = $true
  $Process.StartInfo.StandardOutputEncoding = New-Object Text.UTF8Encoding($false)
  $Process.StartInfo.StandardErrorEncoding = New-Object Text.UTF8Encoding($false)
  $Arguments = @('-I','-X','utf8',$HealthScript,'--rife-dir',$Project,'--model-dir',(Split-Path -Parent $Model),'--manifest',$Manifest,'--json')
  # Windows command-line quoting, passed directly to CreateProcess (no shell).
  $Process.StartInfo.Arguments = ($Arguments | ForEach-Object { '"' + ($_ -replace '(\\*)"', '$1$1\"' -replace '(\\+)$', '$1$1') + '"' }) -join ' '
  try {
    if (-not $Process.Start()) { return $null }
    $Stdout = $Process.StandardOutput.ReadToEndAsync()
    $Stderr = $Process.StandardError.ReadToEndAsync()
    if (-not $Process.WaitForExit($TimeoutMilliseconds)) {
      $Process.Kill()
      $Process.WaitForExit(5000) | Out-Null
      [Console]::WriteLine("cia render SETUP: The runtime health check timed out after $($TimeoutMilliseconds / 1000) seconds. Retry after checking disk and GPU driver health.")
      return $null
    }
    $Output = $Stdout.GetAwaiter().GetResult()
    $Diagnostic = $Stderr.GetAwaiter().GetResult()
    try { $Health = (($Output -split '[\r\n]+' | Where-Object { $_ }) | Select-Object -Last 1) | ConvertFrom-Json }
    catch { [Console]::WriteLine('cia render SETUP: The runtime health check returned no valid result.'); return $null }
    if ($Process.ExitCode -ne 0 -or $Health.status -ne 'ready') {
      if ($Health.detail) { [Console]::WriteLine("cia render SETUP: Health check: $($Health.detail)") }
      return $null
    }
    return $Health
  } finally { $Process.Dispose() }
}
try {
  . (Join-Path $PSScriptRoot 'bootstrap-python.ps1')
  $Profile = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'rife-profile.json') -Raw | ConvertFrom-Json
  $Requirements = Join-Path $PSScriptRoot 'rife-requirements.lock'
  if (-not (Test-Path -LiteralPath $HealthScript -PathType Leaf)) { throw 'The bundled runtime health probe is missing. Reinstall cia render.' }
  $RuntimeRoot = [IO.Path]::GetFullPath($RuntimeRoot)
  New-Item -ItemType Directory -Force -Path $RuntimeRoot | Out-Null
  if ((Get-Item -LiteralPath $RuntimeRoot -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) {
    throw 'The managed runtime directory must not be a junction or symbolic link.'
  }
  try {
    $InstallLock = [IO.File]::Open((Join-Path $RuntimeRoot '.install.lock'), [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
  } catch { throw 'Another cia render instance is installing or repairing this environment. Wait for it to finish and retry.' }
  $ActivePath = Join-Path $RuntimeRoot 'active.json'
  if (-not $ForceRepair -and (Test-Path -LiteralPath $ActivePath)) {
    try {
      $Active = Get-Content -LiteralPath $ActivePath -Raw | ConvertFrom-Json
      $ActivePython = Resolve-ManagedPath $Active.pythonExecutable
      $ActiveProject = Resolve-ManagedPath $Active.rifeDirectory
      $ActiveModel = Resolve-ManagedPath $Active.modelFile
      $ActiveManifest = Resolve-ManagedPath $Active.manifest
      if ($Active.gpuIndex -match '^\d+$') { $env:CUDA_VISIBLE_DEVICES = $Active.gpuIndex }
      $Health = Invoke-RifeHealth $ActivePython $ActiveProject $ActiveModel $ActiveManifest
      if ($Health) {
        [Console]::WriteLine('cia render SETUP: The managed environment passed its full health check.')
        [Console]::WriteLine("CIA_RENDER_RIFE_READY|$ActivePython|$ActiveProject|$ActiveModel")
        exit 0
      }
    } catch { [Console]::WriteLine('cia render SETUP: The previous environment needs repair; preparing a replacement.') }
  }
  Write-Step 1 'Checking GPU and driver compatibility'
  $Smi = Get-Command nvidia-smi.exe -ErrorAction SilentlyContinue | Select-Object -First 1
  $SmiPath = if ($Smi) { $Smi.Source } else { Join-Path ([Environment]::SystemDirectory) 'nvidia-smi.exe' }
  if (-not (Test-Path -LiteralPath $SmiPath -PathType Leaf)) {
    throw 'No NVIDIA driver was detected. Interpolation requires a supported NVIDIA GPU; RENDER remains available.'
  }
  $GpuLines = & $SmiPath --query-gpu=index,name,driver_version,compute_cap,memory.total --format=csv,noheader,nounits
  Assert-ExitCode 'NVIDIA GPU detection'
  $Gpus = @($GpuLines | ConvertFrom-Csv -Header index,name,driver,capability,memory)
  $Capabilities = @($Profile.supportedComputeCapabilities | ForEach-Object { "$_".Replace(' ', '.') })
  $Supported = @($Gpus | Where-Object {
    $Capabilities -contains $_.capability.Trim() -and [version]$_.driver.Trim() -ge [version]$Profile.minimumDriver -and [double]$_.memory.Trim() -ge 2048
  })
  if ($Supported.Count -eq 0) {
    throw "No supported GPU/driver combination was found. Interpolation requires NVIDIA compute capability 7.5, 8.0, 8.6, 8.9, 9.0, 10.0 or 12.0, at least 2 GiB VRAM and driver $($Profile.minimumDriver) or newer. RENDER remains available."
  }
  # cuda:0 must refer to a supported adapter on mixed/older multi-GPU systems.
  $Selected = $Supported | Sort-Object { [double]$_.memory.Trim() } -Descending | Select-Object -First 1
  $env:CUDA_VISIBLE_DEVICES = $Selected.index.Trim()
  [Console]::WriteLine("cia render SETUP: Using $($Selected.name.Trim()), driver $($Selected.driver.Trim()), compute $($Selected.capability.Trim()).")
  Write-Step 2 'Checking storage and preparing a new generation'
  $Disk = New-Object IO.DriveInfo ([IO.Path]::GetPathRoot($RuntimeRoot))
  if ($Disk.AvailableFreeSpace -lt $Profile.minimumFreeBytes) {
    throw 'Interpolation installation needs 14 GiB free space for verified downloads, extraction and rollback. Free space on the runtime drive and retry.'
  }
  $Generations = Join-Path $RuntimeRoot 'generations'
  New-Item -ItemType Directory -Force -Path $Generations | Out-Null
  # Only explicitly marked, app-owned, abandoned candidates are cleaned. This
  # operation is serialized by the lock and never removes a working generation.
  Get-ChildItem -LiteralPath $Generations -Directory | ForEach-Object {
    $Marker = Join-Path $_.FullName 'install-state.json'
    if (Test-Path -LiteralPath $Marker) {
      try {
        $State = Get-Content -LiteralPath $Marker -Raw | ConvertFrom-Json
        if ($State.owner -eq 'cia-render' -and $State.status -eq 'building') { Remove-RifeDirectory $_.FullName $RuntimeRoot }
      } catch { [Console]::WriteLine('cia render SETUP: An abandoned candidate could not be cleaned; it will not be reused.') }
    }
  }
  $GenerationId = $Profile.profileId + '-' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmss') + '-' + [guid]::NewGuid().ToString('N').Substring(0,8)
  $GenerationRoot = Join-Path $Generations $GenerationId
  New-Item -ItemType Directory -Path $GenerationRoot | Out-Null
  @{owner='cia-render';status='building'} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $GenerationRoot 'install-state.json') -Encoding UTF8
  $Cache = Join-Path $RuntimeRoot '.cache'
  New-Item -ItemType Directory -Force -Path $Cache | Out-Null
  Write-Step 3 'Preparing private Python'
  $PythonArchive = Get-RifeArtifact $RifePythonPackageUrl (Join-Path $Cache 'python-3.11.9.zip') $RifePythonPackageHash 'Python 3.11.9'
  $BasePython = Initialize-RifePython -RuntimeRoot $GenerationRoot -PackagePath $PythonArchive
  $VenvRoot = Join-Path $GenerationRoot 'venv'
  & $BasePython -I -X utf8 -m venv $VenvRoot
  Assert-ExitCode 'Private virtual environment creation'
  $VenvPython = Join-Path $VenvRoot 'Scripts/python.exe'
  Write-Step 4 'Downloading the verified CUDA dependency set'
  $WheelRoot = Join-Path $Cache 'wheels'
  foreach ($Wheel in $Profile.wheels) { Get-RifeArtifact $Wheel.url (Join-Path $WheelRoot $Wheel.file) $Wheel.sha256 $Wheel.file | Out-Null }
  Write-Step 5 'Installing pinned dependencies offline'
  & $VenvPython -I -X utf8 -m pip --isolated install --disable-pip-version-check --no-index --find-links $WheelRoot --require-hashes --only-binary=:all: --no-deps -r $Requirements
  Assert-ExitCode 'Verified CUDA dependency installation'
  & $VenvPython -I -X utf8 -m pip --isolated check
  Assert-ExitCode 'Dependency consistency verification'
  Write-Step 6 'Preparing the verified RIFE project and model'
  $ProjectArchive = Get-RifeArtifact $Profile.projectArchive.url (Join-Path $Cache $Profile.projectArchive.file) $Profile.projectArchive.sha256 'Practical-RIFE source'
  $ProjectExtract = Join-Path $GenerationRoot 'project-extract'
  Expand-Archive -LiteralPath $ProjectArchive -DestinationPath $ProjectExtract
  $ProjectSource = Get-ChildItem -LiteralPath $ProjectExtract -Directory | Where-Object { Test-Path -LiteralPath (Join-Path $_.FullName 'inference_video.py') } | Select-Object -First 1
  if (-not $ProjectSource) { throw 'The verified Practical-RIFE archive is incomplete.' }
  $ProjectRoot = Join-Path $GenerationRoot 'Practical-RIFE'
  Move-Item -LiteralPath $ProjectSource.FullName -Destination $ProjectRoot
  Remove-RifeDirectory $ProjectExtract $GenerationRoot
  $ModelArchive = Get-RifeArtifact $Profile.modelArchive.url (Join-Path $Cache $Profile.modelArchive.file) $Profile.modelArchive.sha256 'RIFE 4.26 model'
  $ModelExtract = Join-Path $GenerationRoot 'model-extract'
  Expand-Archive -LiteralPath $ModelArchive -DestinationPath $ModelExtract
  $ModelSource = Get-ChildItem -LiteralPath $ModelExtract -Recurse -Filter flownet.pkl -File | Select-Object -First 1
  if (-not $ModelSource) { throw 'The verified RIFE model archive is incomplete.' }
  $ModelDirectory = Join-Path $ProjectRoot 'train_log'
  New-Item -ItemType Directory -Force -Path $ModelDirectory | Out-Null
  Get-ChildItem -LiteralPath (Split-Path -Parent $ModelSource.FullName) -File | ForEach-Object { Copy-Item -LiteralPath $_.FullName -Destination (Join-Path $ModelDirectory $_.Name) -Force }
  Remove-RifeDirectory $ModelExtract $GenerationRoot
  $ModelTarget = Join-Path $ModelDirectory 'flownet.pkl'
  Remove-RifeDirectory (Join-Path $GenerationRoot '.download-staging') $GenerationRoot
  $Files = @{}
  # Hash the complete installed generation, including dependency DLLs/source.
  # Bytecode caches are interpreter-generated and excluded; source is verified.
  Get-ChildItem -LiteralPath $GenerationRoot -Recurse -File | Where-Object {
    $_.Name -ne 'install-state.json' -and $_.Extension -ne '.pyc' -and $_.FullName -notmatch '[\\/]__pycache__[\\/]'
  } | ForEach-Object {
    $Relative = $_.FullName.Substring($GenerationRoot.Length + 1).Replace('\','/')
    $Files[$Relative] = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
  }
  foreach ($Property in $Profile.projectFiles.PSObject.Properties) { $Files['Practical-RIFE/' + $Property.Name] = $Property.Value }
  foreach ($Property in $Profile.modelFiles.PSObject.Properties) { $Files['Practical-RIFE/train_log/' + $Property.Name] = $Property.Value }
  $ManifestPath = Join-Path $GenerationRoot 'runtime-manifest.json'
  @{schemaVersion=1;profileId=$Profile.profileId;generationId=$GenerationId;packages=$Profile.packages;files=$Files;gpuIndex=$Selected.index.Trim();gpuName=$Selected.name.Trim();driver=$Selected.driver.Trim()} | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $ManifestPath -Encoding UTF8
  Write-Step 7 'Verifying CUDA kernels and real model inference'
  $Health = Invoke-RifeHealth $VenvPython $ProjectRoot $ModelTarget $ManifestPath
  if (-not $Health) { throw 'The new runtime failed its model inference self-test. The previous environment was preserved. Review the installation log.' }
  # Create each generation at its final unique path before venv creation: moving
  # the directory afterwards would invalidate absolute pyvenv.cfg paths.
  @{owner='cia-render';status='ready'} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $GenerationRoot 'install-state.json') -Encoding UTF8
  $RelativeRoot = 'generations/' + $GenerationId
  $Pointer = @{schemaVersion=1;profileId=$Profile.profileId;generationId=$GenerationId;pythonExecutable=$RelativeRoot+'/venv/Scripts/python.exe';rifeDirectory=$RelativeRoot+'/Practical-RIFE';modelFile=$RelativeRoot+'/Practical-RIFE/train_log/flownet.pkl';manifest=$RelativeRoot+'/runtime-manifest.json';gpuIndex=$Selected.index.Trim()}
  $TemporaryPointer = Join-Path $RuntimeRoot ('active-' + [guid]::NewGuid().ToString('N') + '.tmp')
  [IO.File]::WriteAllText($TemporaryPointer, ($Pointer | ConvertTo-Json -Depth 4), (New-Object Text.UTF8Encoding($false)))
  if (Test-Path -LiteralPath $ActivePath) { [IO.File]::Replace($TemporaryPointer, $ActivePath, (Join-Path $RuntimeRoot 'active-previous.json'), $true) }
  else { [IO.File]::Move($TemporaryPointer, $ActivePath) }
  Write-Step 8 'Interpolation environment ready'
  [Console]::WriteLine("CIA_RENDER_RIFE_READY|$VenvPython|$ProjectRoot|$ModelTarget")
} catch {
  $Message = $_.Exception.Message -replace '[\r\n]+', ' '
  [Console]::Error.WriteLine("CIA_INSTALL_ERROR $Message")
  exit 1
} finally {
  if ($InstallLock) { $InstallLock.Dispose() }
  # Failed candidates remain marked building, ready for cleanup on the next
  # repair. Existing and previous working environments are never removed here.
}
