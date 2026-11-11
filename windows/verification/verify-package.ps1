[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$PackageDirectory,
    [switch]$AllowInstalledRuntime,
    [string]$ReportPath = (Join-Path $env:TEMP 'panechord-windows-next-package.json')
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
# Explicit guard: verifier/application execution belongs ONLY in Sandbox.
if ([Environment]::UserName -ine 'WDAGUtilityAccount') { throw 'Package verifier must run in Windows Sandbox, never on the host.' }
$directory = (Resolve-Path -LiteralPath $PackageDirectory).Path
$exe = Join-Path $directory 'panechord-windows-next.exe'
if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) { throw "Next executable missing: $exe" }
$frameworks = @(Get-AppxPackage 'Microsoft.WindowsAppRuntime.*' | Where-Object { $_.Name -notmatch '\.CBS\.' } | Select-Object Name, Version, Architecture)
if ($frameworks.Count -gt 0 -and -not $AllowInstalledRuntime) { throw 'Installed Runtime found; clean candidate required. No installation/removal attempted.' }
$report = [ordered]@{ FrameworksBeforeLaunch = $frameworks; CleanEnvironmentCandidate = ($frameworks.Count -eq 0)
    AllowInstalledRuntime = [bool]$AllowInstalledRuntime; PackageDirectory = $directory; LoadedRuntimeModules = @()
    NativeIdentities = @(); Stdout = ''; Stderr = ''; ExitCode = $null; Passed = $false; Diagnostics = @() }
$info = [Diagnostics.ProcessStartInfo]::new()
$info.FileName = $exe; $info.Arguments = '--smoke'; $info.WorkingDirectory = $directory
$info.UseShellExecute = $false; $info.RedirectStandardOutput = $true; $info.RedirectStandardError = $true
$process = [Diagnostics.Process]::new(); $process.StartInfo = $info
$lines = [Collections.Generic.List[string]]::new(); $started = $false; $stderr = $null; $stdout = $null
try {
    if (-not $process.Start()) { throw 'Could not start next executable' }; $started = $true
    $stderr = $process.StandardError.ReadToEndAsync()
    $ready = 0; $watchdog = [Diagnostics.Stopwatch]::StartNew()
    while ($ready -lt 2) {
        $remaining = 10000 - [int]$watchdog.ElapsedMilliseconds
        if ($remaining -le 0) { throw 'Native READY watchdog expired (driver HWND callback diagnostics required)' }
        $line = $process.StandardOutput.ReadLineAsync()
        if (-not $line.Wait($remaining)) { throw 'Native READY watchdog expired' }
        if ($null -eq $line.Result) { throw 'Process ended before two native READY windows' }
        $lines.Add($line.Result)
        if ($line.Result.StartsWith('READY ')) { $ready++ }
    }
    # Inventory while both native windows live, before automated close sequence.
    $report.LoadedRuntimeModules = @($process.Modules | Where-Object { $_.ModuleName -match '^(Microsoft\.(UI|WindowsAppRuntime)|Microsoft\.Web\.WebView2\.Core|DWriteCore|CoreMessagingXP).*\.dll$' } | ForEach-Object {
        [pscustomobject]@{ Name = $_.ModuleName; Path = $_.FileName; Version = $_.FileVersionInfo.FileVersion
            AppLocal = $_.FileName.StartsWith($directory.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase) }
    })
    $stdout = $process.StandardOutput.ReadToEndAsync()
    if (-not $process.WaitForExit(15000)) { throw 'Full smoke exit watchdog expired' }
    if (-not $stdout.Wait(1000) -or -not $stderr.Wait(1000)) { throw 'Smoke capture did not close' }
    $report.ExitCode = $process.ExitCode; $report.Stdout = ($lines -join "`n") + "`n" + $stdout.Result; $report.Stderr = $stderr.Result
    if ($report.ExitCode -ne 0 -or -not $report.Stdout.Contains('P1 SMOKE PASS')) { throw 'Full generic Backend native smoke failed' }
    # Stable lifetime-scoped callback contract for the main agent's diagnostics.
    # READY id=<registration> generation=<native-generation> hwnd=<hex-or-decimal>
    # No title enumeration, sibling-private state access or invented READY logs.
    foreach ($match in [regex]::Matches($report.Stdout, '(?m)^READY id=(\S+) generation=(\d+) hwnd=(0x[0-9a-fA-F]+|\d+)(?:\s|$)')) {
        $report.NativeIdentities += [pscustomobject]@{ Id = $match.Groups[1].Value; Generation = $match.Groups[2].Value; Hwnd = $match.Groups[3].Value }
    }
    $main = @($report.NativeIdentities | Where-Object { $_.Id -ceq 'main' })
    if ($report.NativeIdentities.Count -ne 4 -or @($report.NativeIdentities.Generation | Sort-Object -Unique).Count -ne 4 -or
        @($report.NativeIdentities | Where-Object { $_.Generation -eq '0' -or $_.Hwnd -match '^(0x)?0+$' }).Count -gt 0 -or
        $main.Count -ne 2 -or $main[0].Generation -eq $main[1].Generation -or
        @($report.NativeIdentities | Where-Object { $_.Id -ceq 'sibling' }).Count -ne 1 -or
        @($report.NativeIdentities | Where-Object { $_.Id -ceq 'auxiliary' }).Count -ne 1) { throw 'Missing distinct native identities/dynamic windows/ID-reuse evidence from driver callbacks' }
    foreach ($stage in @('STAGE same-caption windows=2', 'STAGE empty-title windows=2', 'STAGE all-controls roundtrip=pass',
        'STAGE main-closed sibling-present', 'STAGE id-reuse id=main', 'STAGE auxiliary-continue windows=1', 'STAGE last-close-request autoexit-required')) {
        if (-not $report.Stdout.Contains($stage)) { throw "Missing scenario evidence: $stage" }
    }
    if ($report.LoadedRuntimeModules.Count -eq 0 -or @($report.LoadedRuntimeModules | Where-Object { -not $_.AppLocal }).Count -ne 0) { throw 'Runtime modules missing or loaded outside package' }
    foreach ($name in @('Microsoft.UI.Xaml.dll', 'Microsoft.UI.Windowing.Core.dll')) {
        if (@($report.LoadedRuntimeModules | Where-Object { $_.Name -ieq $name }).Count -ne 1) { throw "Required module not loaded: $name" }
    }
    $report.Passed = $true; $report.Stdout
    Write-Output "PACKAGE_ORIGIN_PASS clean_candidate=$($report.CleanEnvironmentCandidate) report=$ReportPath"
} catch { $report.Diagnostics += $_.Exception.ToString(); throw }
finally {
    if ($started -and -not $process.HasExited) { $process.Kill(); $null = $process.WaitForExit(1000) }
    if ($started -and $process.HasExited) { $report.ExitCode = $process.ExitCode }
    if ($null -ne $stderr -and $stderr.Wait(1000)) { $report.Stderr = $stderr.Result }
    if (-not $report.Stdout) { $report.Stdout = $lines -join "`n" }
    $report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $ReportPath -Encoding UTF8
    $process.Dispose()
}
