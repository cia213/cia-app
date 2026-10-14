param([string]$PackagePath = '')
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = New-Object Text.UTF8Encoding($false)
$Repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$Bootstrap = Join-Path $Repository 'src-tauri/resources/bootstrap/bootstrap-rife.ps1'
$HealthScript = Join-Path $Repository 'src-tauri/resources/runtime_health.py'
. (Join-Path $Repository 'src-tauri/resources/bootstrap/bootstrap-python.ps1')
$Root = Join-Path $Repository ('src-tauri/target/test-generation-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $Root | Out-Null
try {
  # A second process must fail before any downloads or pointer changes while the
  # first instance owns the installation lock. No GPU is needed for this test.
  $Pointer = Join-Path $Root 'active.json'
  [IO.File]::WriteAllText($Pointer, '{"schemaVersion":1,"generationId":"existing-working-generation"}')
  $Before = (Get-FileHash -LiteralPath $Pointer).Hash
  $Lock = [IO.File]::Open((Join-Path $Root '.install.lock'), [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
  $PreviousPreference = $ErrorActionPreference
  try {
    $ErrorActionPreference = 'Continue'
    $Output = & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $Bootstrap -RuntimeRoot $Root -HealthScript $HealthScript 2>&1
    $Code = $LASTEXITCODE
  } finally { $ErrorActionPreference = $PreviousPreference; $Lock.Dispose() }
  if ($Code -eq 0 -or "$Output" -notmatch 'Another cia render instance') { throw 'Concurrent installation was not rejected with a useful diagnostic.' }
  if ((Get-FileHash -LiteralPath $Pointer).Hash -ne $Before) { throw 'The rejected installation modified the active pointer.' }
  if (Test-Path -LiteralPath (Join-Path $Root '.cache')) { throw 'The rejected installation started downloading artifacts.' }

  # A verified cached download must be reused; a bad cached file must not be
  # accepted simply because its destination exists.
  $Cached = Join-Path $Root 'cached.bin'
  [IO.File]::WriteAllText($Cached, 'verified cached content')
  $Hash = (Get-FileHash -LiteralPath $Cached).Hash
  $Result = Get-RifeArtifact 'https://invalid.example/not-requested' $Cached $Hash 'fixture'
  if ($Result -ne $Cached) { throw 'A verified cached artifact was not reused.' }

  $tokens=$null; $errors=$null
  $ast=[Management.Automation.Language.Parser]::ParseFile($Bootstrap,[ref]$tokens,[ref]$errors)
  if ($errors.Count) { throw 'The installer contains a PowerShell syntax error.' }
  $resolver=$ast.Find({param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Resolve-ManagedPath'}, $true)
  Invoke-Expression $resolver.Extent.Text
  $RuntimeRoot=$Root
  $Rejected=$false
  try { Resolve-ManagedPath '../outside' | Out-Null } catch { $Rejected=$true }
  if (-not $Rejected) { throw 'A runtime pointer was allowed to escape its managed directory.' }
  $Python = Get-Command python.exe -ErrorAction SilentlyContinue | Select-Object -First 1
  if ($Python) {
    $probe=$ast.Find({param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Invoke-RifeHealth'}, $true)
    Invoke-Expression $probe.Extent.Text
    $HealthScript=Join-Path $Root 'slow-health.py'
    [IO.File]::WriteAllText($HealthScript, 'import time; time.sleep(30)')
    $Timer=[Diagnostics.Stopwatch]::StartNew()
    $Result=Invoke-RifeHealth $Python.Source $Root (Join-Path $Root 'flownet.pkl') (Join-Path $Root 'runtime-manifest.json') 300
    $Timer.Stop()
    if ($Result -or $Timer.Elapsed.TotalSeconds -gt 5) { throw 'A stalled health probe exceeded its deadline or reported readiness.' }
  }
  Write-Output 'Concurrent install rejection, preserved active pointer, verified cache reuse, pointer containment, and bounded health probe: OK'
} finally {
  Remove-RifeDirectory $Root (Join-Path $Repository 'src-tauri/target')
}
