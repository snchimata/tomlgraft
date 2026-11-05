param([Parameter(Mandatory = $true)][string] $Cli)
$ErrorActionPreference = 'Stop'
$Cli = (Resolve-Path -LiteralPath $Cli).Path
$testRoot = Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid().ToString())
[IO.Directory]::CreateDirectory($testRoot) | Out-Null
try {
    $manifest = Join-Path $testRoot 'pyproject.toml'
    [IO.File]::WriteAllText($manifest, "[project]`r`nversion = '1' # keep")
    $report = & $Cli set $manifest project.version "'1.4.2'" --format json | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0 -or !$report.committed -or ![IO.File]::ReadAllText($manifest).Contains("version = '1.4.2' # keep")) { throw 'PowerShell string value smoke failed' }
    $report = & $Cli dep add $manifest 'pytest>=8' --group dev --format json | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0 -or !$report.committed) { throw 'PowerShell dependency smoke failed' }
    $batch = Join-Path $testRoot 'changes.json'
    [IO.File]::WriteAllText($batch, '{"schema_version":1,"operations":[{"op":"set","path":["tool","example.org","enabled"],"value_toml":"true"}]}')
    $report = & $Cli batch $manifest --patch $batch --dry-run --format json | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0 -or $report.status -ne 'would_change' -or $report.committed) { throw 'PowerShell batch preview smoke failed' }
    Write-Output 'PowerShell examples passed'
} finally {
    # The root is an absolute, freshly created GUID directory owned by this test.
    $resolvedTestRoot = [IO.Path]::GetFullPath($testRoot)
    $resolvedTempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
    if (!$resolvedTestRoot.StartsWith($resolvedTempRoot, [StringComparison]::OrdinalIgnoreCase)) { throw 'Refusing cleanup outside temp root' }
    Remove-Item -LiteralPath $resolvedTestRoot -Recurse -Force
}
