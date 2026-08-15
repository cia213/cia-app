param(
  [Parameter(Mandatory = $true)][string]$RuntimeRoot,
  [Parameter(Mandatory = $true)][string]$PythonInstaller
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

# --- Fail fast: require an NVIDIA GPU before any download ---
Write-Output 'CIA_PROGRESS step=1 total=7 label=Checking NVIDIA GPU'
$NvidiaSmi = Get-Command nvidia-smi -ErrorAction SilentlyContinue
if (-not $NvidiaSmi) {
  throw 'ERROR: nvidia-smi not found. RIFE requires an NVIDIA GPU with CUDA support. Install aborted before any download.'
}
$GpuCheck = & nvidia-smi --query-gpu=name --format=csv,noheader 2>&1
if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($GpuCheck)) {
  throw "ERROR: No NVIDIA GPU detected (nvidia-smi returned: $GpuCheck). RIFE requires CUDA. Install aborted before any download."
}
Write-Output "cia app SETUP: NVIDIA GPU detected: $($GpuCheck.Trim())"

# This script only writes below RuntimeRoot, which cia app resolves inside its
# per-user application data. Nothing is installed system-wide or added to PATH.
$PythonHome = Join-Path $RuntimeRoot 'python'
$VenvRoot = Join-Path $RuntimeRoot 'venv'
$ProjectRoot = Join-Path $RuntimeRoot 'Practical-RIFE'
$ModelTarget = Join-Path $ProjectRoot 'train_log\flownet.pkl'
$Staging = Join-Path $RuntimeRoot '.download-staging'
$ExpectedModelHash = '45C7F74156704769DC9F85CFCAF8552E1E926F9399DCFA3A553DEE88FAC6F53F'

function Write-Step([string]$Message) {
  Write-Output "cia app SETUP: $Message"
}

function Assert-ExitCode([string]$Step) {
  if ($LASTEXITCODE -ne 0) {
    throw "$Step failed with exit code $LASTEXITCODE."
  }
}

New-Item -ItemType Directory -Force -Path $RuntimeRoot | Out-Null
New-Item -ItemType Directory -Force -Path $Staging | Out-Null

if (-not (Test-Path -LiteralPath $PythonInstaller -PathType Leaf)) {
  throw "The bundled Python installer is missing: $PythonInstaller"
}

Write-Output 'CIA_PROGRESS step=2 total=7 label=Installing Python 3.11'
$BasePython = Join-Path $PythonHome 'python.exe'
if (-not (Test-Path -LiteralPath $BasePython -PathType Leaf)) {
  Write-Step 'Installing the bundled Python 3.11 runtime...'
  & $PythonInstaller /quiet InstallAllUsers=0 "TargetDir=$PythonHome" Include_pip=1 Include_test=0 Include_tcltk=0 Include_launcher=0 PrependPath=0
  Assert-ExitCode 'Python installation'
}
if (-not (Test-Path -LiteralPath $BasePython -PathType Leaf)) {
  throw "Python installation completed without creating $BasePython"
}

Write-Output 'CIA_PROGRESS step=3 total=7 label=Creating virtual environment'
$VenvPython = Join-Path $VenvRoot 'Scripts\python.exe'
if (-not (Test-Path -LiteralPath $VenvPython -PathType Leaf)) {
  Write-Step 'Creating the isolated RIFE environment...'
  & $BasePython -m venv $VenvRoot
  Assert-ExitCode 'Virtual environment creation'
}

Write-Output 'CIA_PROGRESS step=4 total=7 label=Installing RIFE dependencies'
Write-Step 'Installing RIFE dependencies (this is the largest download)...'
& $VenvPython -m pip install --disable-pip-version-check --upgrade pip
Assert-ExitCode 'pip upgrade'
& $VenvPython -m pip install --disable-pip-version-check `
  'numpy==1.23.5' 'tqdm==4.67.1' 'sk-video==1.1.10' 'opencv-python==4.10.0.84' 'moviepy==1.0.3'
Assert-ExitCode 'RIFE support dependencies'
Write-Output 'CIA_PROGRESS step=5 total=7 label=Installing CUDA PyTorch'
& $VenvPython -m pip install --disable-pip-version-check `
  'torch==2.5.1' 'torchvision==0.20.1' --index-url 'https://download.pytorch.org/whl/cu121'
Assert-ExitCode 'CUDA PyTorch dependencies'

Write-Output 'CIA_PROGRESS step=6 total=7 label=Downloading Practical-RIFE'
if (-not (Test-Path -LiteralPath (Join-Path $ProjectRoot 'inference_video.py') -PathType Leaf)) {
  Write-Step 'Downloading Practical-RIFE...'
  $ProjectZip = Join-Path $Staging 'practical-rife.zip'
  Invoke-WebRequest -UseBasicParsing `
    -Uri 'https://github.com/hzwer/Practical-RIFE/archive/17d8c7a1005b37f4c97bfee04e316aaec7fdc536.zip' `
    -OutFile $ProjectZip
  $ProjectExtract = Join-Path $Staging 'project'
  Expand-Archive -LiteralPath $ProjectZip -DestinationPath $ProjectExtract -Force
  $ProjectSource = Get-ChildItem -LiteralPath $ProjectExtract -Directory |
    Where-Object { Test-Path -LiteralPath (Join-Path $_.FullName 'inference_video.py') -PathType Leaf } |
    Select-Object -First 1
  if ($null -eq $ProjectSource) {
    throw 'The Practical-RIFE source archive did not contain inference_video.py.'
  }
  if (Test-Path -LiteralPath $ProjectRoot) {
    Remove-Item -LiteralPath $ProjectRoot -Recurse -Force
  }
  Move-Item -LiteralPath $ProjectSource.FullName -Destination $ProjectRoot
}

Write-Output 'CIA_PROGRESS step=7 total=7 label=Downloading RIFE 4.26 model'
if (-not (Test-Path -LiteralPath $ModelTarget -PathType Leaf)) {
  Write-Step 'Downloading the official RIFE 4.26 model...'
  $ModelDownload = Join-Path $Staging 'rife-4.26-model.download'
  Invoke-WebRequest -UseBasicParsing `
    -Uri 'https://drive.usercontent.google.com/download?id=1gViYvvQrtETBgU1w8axZSsr7YUuw31uy&export=download&confirm=t' `
    -OutFile $ModelDownload
  if ((Get-Item -LiteralPath $ModelDownload).Length -lt 1000000) {
    throw 'The RIFE model download was unexpectedly small; Google Drive did not return the model archive.'
  }
  $ModelDirectory = Split-Path -Parent $ModelTarget
  New-Item -ItemType Directory -Force -Path $ModelDirectory | Out-Null
  $Header = [System.IO.File]::ReadAllBytes($ModelDownload)[0..1]
  if ($Header[0] -eq 80 -and $Header[1] -eq 75) {
    $ModelExtract = Join-Path $Staging 'model'
    Expand-Archive -LiteralPath $ModelDownload -DestinationPath $ModelExtract -Force
    $ModelSource = Get-ChildItem -LiteralPath $ModelExtract -Recurse -Filter 'flownet.pkl' -File |
      Select-Object -First 1
    if ($null -eq $ModelSource) {
      throw 'The RIFE model archive did not contain flownet.pkl.'
    }
    Copy-Item -LiteralPath $ModelSource.FullName -Destination $ModelTarget -Force
  } else {
    Copy-Item -LiteralPath $ModelDownload -Destination $ModelTarget -Force
  }
}

$ModelHash = (Get-FileHash -LiteralPath $ModelTarget -Algorithm SHA256).Hash
if ($ModelHash -ne $ExpectedModelHash) {
  throw "The downloaded RIFE model hash does not match the expected 4.26 model. Got $ModelHash"
}

Write-Step 'Checking CUDA availability...'
& $VenvPython -c "import torch; assert torch.cuda.is_available(), 'CUDA-capable NVIDIA GPU not available'; print(torch.__version__); print(torch.cuda.get_device_name(0))"
Assert-ExitCode 'CUDA verification'

Remove-Item -LiteralPath $Staging -Recurse -Force -ErrorAction SilentlyContinue
Write-Output "CIA_RENDER_RIFE_READY|$VenvPython|$ProjectRoot|$ModelTarget"
