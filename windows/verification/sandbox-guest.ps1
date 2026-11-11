# Guest only. Never launch these executables on the host.
[CmdletBinding()]
param([switch]$KeepOpen)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
# Preserve mapping scope for the unchanged P0 seven-case cleanup mocks.
$inputDirectory = 'C:\p0-input'; $outputDirectory = 'C:\p0-output'; $packageDirectory = 'C:\next-package'
$sandboxAccount = ([Environment]::UserName -ieq 'WDAGUtilityAccount')
$keepSession = [bool]$KeepOpen -or -not $sandboxAccount
$report = [ordered]@{
    SchemaVersion = 1; Passed = $false; StartUtc = [DateTime]::UtcNow.ToString('o'); EndUtc = $null
    PackageDirectory = $packageDirectory; Runtime = $null; FrameworksBefore = @(); FrameworksAfter = @()
    Configuration = $null; PackageVerifier = $null; PackageVerifierReport = $null
    Tests = @(); Cases = @(); Probes = @(); CleanupTests = $null; Diagnostics = @()
}
function Invoke-BoundedChild([string]$Name, [string]$FileName, [string]$Arguments) {
    $result = [ordered]@{ Name = $Name; FileName = $FileName; Arguments = $Arguments
        StartUtc = [DateTime]::UtcNow.ToString('o'); EndUtc = $null; DurationMilliseconds = $null
        ExitCode = $null; TimedOut = $false; Stdout = ''; Stderr = ''; Diagnostics = @(); Passed = $false }
    $process = [Diagnostics.Process]::new(); $info = [Diagnostics.ProcessStartInfo]::new()
    $info.FileName = $FileName; $info.Arguments = $Arguments; $info.WorkingDirectory = $packageDirectory
    $info.UseShellExecute = $false; $info.CreateNoWindow = $true
    $info.RedirectStandardOutput = $true; $info.RedirectStandardError = $true; $process.StartInfo = $info
    $started = $false; $stdout = $null; $stderr = $null; $clock = [Diagnostics.Stopwatch]::StartNew()
    try {
        if (-not $process.Start()) { throw "Unable to start $Name" }; $started = $true
        $stdout = $process.StandardOutput.ReadToEndAsync(); $stderr = $process.StandardError.ReadToEndAsync()
        if (-not $process.WaitForExit([Math]::Max(0, 30000 - [int]$clock.ElapsedMilliseconds))) { $result.TimedOut = $true; throw "$Name exceeded 30s child watchdog" }
        $result.ExitCode = $process.ExitCode
    } catch { $result.Diagnostics += $_.Exception.ToString() }
    finally {
        if ($started -and -not $process.HasExited) {
            try { $process.Kill(); $null = $process.WaitForExit(1000) } catch { $result.Diagnostics += $_.Exception.ToString() }
        }
        if ($started -and $process.HasExited) { $result.ExitCode = $process.ExitCode }
        foreach ($capture in @(@{ Task = $stdout; Key = 'Stdout' }, @{ Task = $stderr; Key = 'Stderr' })) {
            if ($null -ne $capture.Task) {
                try { if ($capture.Task.Wait(1000)) { $result[$capture.Key] = $capture.Task.Result } else { $result.Diagnostics += "$($capture.Key) did not close" } }
                catch { $result.Diagnostics += $_.Exception.ToString() }
            }
        }
        $result.DurationMilliseconds = $clock.ElapsedMilliseconds; $result.EndUtc = [DateTime]::UtcNow.ToString('o'); $process.Dispose()
    }
    [pscustomobject]$result
}
function Get-FrameworkInventory {
    @(Get-AppxPackage 'Microsoft.WindowsAppRuntime.*' | Where-Object { $_.Name -notmatch '\.CBS\.' } | Select-Object Name, Version, Architecture)
}
function Assert-PackageLeaf([string]$Name) {
    if ($Name -notmatch '^[a-zA-Z0-9][a-zA-Z0-9._-]*$' -or $Name -in @('.', '..')) { throw "Invalid package leaf: $Name" }
    if (-not (Test-Path -LiteralPath (Join-Path $packageDirectory $Name) -PathType Leaf)) { throw "Package file missing: $Name" }
}
try {
    if (-not $sandboxAccount) { throw 'Guest requires WDAGUtilityAccount; do not invoke on host.' }
    foreach ($path in @($inputDirectory, $outputDirectory)) { if (-not (Test-Path -LiteralPath $path -PathType Container)) { throw "Sandbox mapping missing: $path" } }
    $configuration = Get-Content -LiteralPath (Join-Path $inputDirectory 'configuration.json') -Raw -Encoding UTF8 | ConvertFrom-Json
    $report.Configuration = $configuration
    $keepSession = $keepSession -or [bool]$configuration.KeepOpen -or [bool]$configuration.HostManagedCleanup
    if ($configuration.SchemaVersion -ne 1 -or $configuration.Application -cne 'panechord-windows-next.exe' -or
        $configuration.SetupVersion -ne '0.100.0' -or $configuration.ReactorVersion -ne '0.100.0' -or
        $configuration.SetupRuntimeNuGetVersion -ne '2.4.0' -or $configuration.WebView2NuGetVersion -ne '1.0.4078.44' -or
        $configuration.ChildTimeoutSeconds -ne 30 -or
        $configuration.RuntimeNuGetVersion -ne '2.4.0' -or $configuration.RuntimeCompatibilityProbe -ne $false) { throw 'Unexpected pinned configuration; legacy probes are not supported.' }
    $windowsPackages = @($configuration.WindowsPackages)
    if ($windowsPackages.Count -eq 0) { throw 'Missing registry dependency inventory.' }
    foreach ($package in $windowsPackages) {
        if ($package.source -cne 'registry+https://github.com/rust-lang/crates.io-index') { throw 'Non-registry Windows dependency in configuration.' }
    }
    foreach ($name in @('windows-reactor', 'windows-reactor-setup')) {
        $packages = @($windowsPackages | Where-Object { $_.name -eq $name })
        if ($packages.Count -ne 1) { throw "Missing/duplicate pinned dependency: $name" }
        if ($packages[0].version -ne '0.100.0') { throw "Unexpected dependency version: $name" }
    }
    $report.Runtime = [ordered]@{ DotNetVersion = [Environment]::Version.ToString(); PowerShellVersion = $PSVersionTable.PSVersion.ToString()
        OS = (Get-CimInstance Win32_OperatingSystem | Select-Object Caption, Version, BuildNumber, OSArchitecture); Is64BitProcess = [Environment]::Is64BitProcess }
    $report.FrameworksBefore = @(Get-FrameworkInventory)
    if ($report.FrameworksBefore.Count -gt 0) { throw 'Installed Windows App Runtime frameworks found; no installation/removal attempted.' }
    if (Test-Path -LiteralPath $packageDirectory) { throw 'Relocation destination must be new.' }
    $null = New-Item -ItemType Directory -Path $packageDirectory
    foreach ($entry in @(Get-ChildItem -LiteralPath $inputDirectory -Recurse -Force)) {
        if (($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Reparse point in payload.' }
    }
    Get-ChildItem -LiteralPath $inputDirectory -Force | ForEach-Object { Copy-Item -LiteralPath $_.FullName -Destination $packageDirectory -Recurse }
    $inventoryDocument = Get-Content -LiteralPath (Join-Path $packageDirectory 'inventory.json') -Raw -Encoding UTF8 | ConvertFrom-Json
    $inventory = @($inventoryDocument)
    if ($inventory.Count -eq 0) { throw 'Empty payload inventory.' }
    foreach ($entry in $inventory) {
        $path = [IO.Path]::GetFullPath((Join-Path $packageDirectory $entry.Name))
        if (-not $path.StartsWith($packageDirectory + '\', [StringComparison]::OrdinalIgnoreCase)) { throw "Escaping inventory entry: $($entry.Name)" }
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "Missing relocated entry: $($entry.Name)" }
        if ((Get-Item -LiteralPath $path).Length -ne $entry.Length -or (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ne $entry.SHA256) { throw "Relocation hash/size mismatch: $($entry.Name)" }
    }
    foreach ($name in @($configuration.CriticalFiles) + @($configuration.Crt.Files) + @('verify-package.ps1', 'sandbox.ps1', 'sandbox-cleanup-tests.ps1', $configuration.Application)) { Assert-PackageLeaf $name }
    foreach ($name in $configuration.CriticalDirectories) {
        if ($name -notmatch '^[a-zA-Z0-9][a-zA-Z0-9._-]*$') { throw 'Invalid resource directory name.' }
        $path = Join-Path $packageDirectory $name
        if (-not (Test-Path -LiteralPath $path -PathType Container) -or @(Get-ChildItem -LiteralPath $path -Recurse -File).Count -eq 0) { throw "Resource directory missing/empty: $name" }
    }
    $tests = @($configuration.Tests)
    if ($tests.Count -eq 0) { throw 'No staged tests; refusing invented success.' }
    foreach ($name in $tests) { if ($name -notmatch '^test-\d+\.exe$') { throw 'Invalid test executable name.' }; Assert-PackageLeaf $name }
    Assert-PackageLeaf $configuration.CasesFile
    $casesDocument = Get-Content -LiteralPath (Join-Path $packageDirectory $configuration.CasesFile) -Raw -Encoding UTF8 | ConvertFrom-Json
    $cases = @($casesDocument)
    $expectedArguments = @('--make-error', '--task-panic', '--reject-wake', '--p1-smoke')
    if ($cases.Count -ne 4) { throw 'Expected three fault cases and full P1 smoke.' }
    for ($i = 0; $i -lt 4; $i++) {
        if ($cases[$i].Arguments -cne $expectedArguments[$i] -or $cases[$i].ExpectedExitCode -ne $(if ($i -eq 3) { 0 } else { 1 }) -or [bool]$cases[$i].Fault -ne ($i -ne 3)) { throw 'Unexpected case contract.' }
    }
    $originPath = Join-Path $outputDirectory 'package-verifier.json'
    $powershell = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    # No AllowInstalledRuntime. --smoke is the same full dynamic/native fixture.
    $arguments = '-NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "' + (Join-Path $packageDirectory 'verify-package.ps1') + '" -PackageDirectory "' + $packageDirectory + '" -ReportPath "' + $originPath + '"'
    $verification = Invoke-BoundedChild 'package-verifier' $powershell $arguments
    if (Test-Path -LiteralPath $originPath -PathType Leaf) {
        try { $report.PackageVerifierReport = Get-Content -LiteralPath $originPath -Raw -Encoding UTF8 | ConvertFrom-Json } catch { $verification.Diagnostics += $_.Exception.ToString() }
    } else { $verification.Diagnostics += 'Package verifier produced no JSON.' }
    $origin = $report.PackageVerifierReport
    $verification.Passed = ($verification.ExitCode -eq 0 -and -not $verification.TimedOut -and $verification.Diagnostics.Count -eq 0 -and $null -ne $origin -and
        $origin.Passed -eq $true -and $origin.CleanEnvironmentCandidate -eq $true -and $origin.AllowInstalledRuntime -eq $false)
    $report.PackageVerifier = $verification
    $cleanupArguments = '-NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "' + (Join-Path $packageDirectory 'sandbox-cleanup-tests.ps1') + '"'
    $cleanup = Invoke-BoundedChild 'sandbox-cleanup-tests' $powershell $cleanupArguments
    $cleanup.Passed = ($cleanup.ExitCode -eq 0 -and -not $cleanup.TimedOut -and $cleanup.Diagnostics.Count -eq 0 -and $cleanup.Stdout.Contains('SANDBOX_CLEANUP_TESTS_PASS cases=7'))
    $report.CleanupTests = $cleanup
    foreach ($name in $tests) {
        $result = Invoke-BoundedChild $name (Join-Path $packageDirectory $name) '--test-threads=1 --nocapture'
        $result.Passed = ($result.ExitCode -eq 0 -and -not $result.TimedOut -and $result.Diagnostics.Count -eq 0 -and $result.Stdout -match 'test result: ok\.' -and $result.Stdout -match '[1-9][0-9]* passed;')
        $report.Tests += $result
    }
    foreach ($case in $cases) {
        $result = Invoke-BoundedChild $case.Name (Join-Path $packageDirectory $configuration.Application) $case.Arguments
        $combined = $result.Stdout + "`n" + $result.Stderr
        $result.Passed = ($result.ExitCode -eq $case.ExpectedExitCode -and -not $result.TimedOut -and $result.Diagnostics.Count -eq 0 -and $combined.Contains([string]$case.ExpectedText) -and
            (-not $case.Fault -or ($result.Stdout.Contains('FAULT_HOST_TEARDOWN_PASS') -and $result.Stdout.Contains('FAULT_TEARDOWN_PASS'))))
        # Native teardown PASS must come from driver diagnostics, not smoke's
        # public Host-only checks. Until those diagnostics land, fail closed.
        $result | Add-Member -NotePropertyName ExpectedExitCode -NotePropertyValue $case.ExpectedExitCode
        $result | Add-Member -NotePropertyName RequiresFaultTeardown -NotePropertyValue ([bool]$case.Fault)
        $report.Cases += $result
    }
    # Optional diagnostics do not replace/relax the original full-smoke verdict.
    if ($configuration.PSObject.Properties.Name -contains 'ProbeExecutable' -and $configuration.ProbeExecutable) {
        if ($configuration.ProbeExecutable -cne 'native-family.exe') { throw 'Unexpected probe executable.' }
        Assert-PackageLeaf $configuration.ProbeExecutable
        $probeCases = @($configuration.ProbeCases)
        if ($probeCases.Count -eq 0 -or $probeCases.Count -gt 24 -or @($probeCases | Sort-Object -Unique).Count -ne $probeCases.Count) { throw 'Invalid probe case count/duplicates.' }
        foreach ($case in $probeCases) {
            if ($case -notmatch '^[a-z][a-z0-9-]{0,47}$') { throw 'Unsafe probe argument.' }
            # Shared YAML demo accepts --case; retain positional legacy native probes.
            $probeArguments = if ($case -ceq 'minimal-yaml') { '--case minimal-yaml' } else { $case }
            $result = Invoke-BoundedChild $case (Join-Path $packageDirectory $configuration.ProbeExecutable) $probeArguments
            $result.Passed = ($result.ExitCode -eq 0 -and -not $result.TimedOut -and $result.Diagnostics.Count -eq 0 -and
                $result.Stderr.Contains("PROBE $case NATIVE_MOUNT_EFFECT") -and $result.Stderr.Contains("PROBE $case close-accepted") -and
                $result.Stdout.Contains("NATIVE_PROBE_PASS $case normal-exit"))
            $report.Probes += $result
        }
    }
    $report.FrameworksAfter = @(Get-FrameworkInventory)
    $report.Passed = ($verification.Passed -and $cleanup.Passed -and $report.FrameworksAfter.Count -eq 0 -and $report.Tests.Count -eq $tests.Count -and $report.Cases.Count -eq 4 -and
        @($report.Tests + $report.Cases + $report.Probes | Where-Object { -not $_.Passed }).Count -eq 0)
    if (-not $report.Passed) { $report.Diagnostics += 'Package/test/case/native evidence failed; inspect individual results.' }
} catch { $report.Passed = $false; $report.Diagnostics += $_.Exception.ToString(); $report.Diagnostics += $_.ScriptStackTrace }
finally {
    $report.EndUtc = [DateTime]::UtcNow.ToString('o')
    try {
        $temporary = Join-Path $outputDirectory ('report-' + [guid]::NewGuid().ToString('N') + '.tmp')
        $final = Join-Path $outputDirectory 'report.json'; $encoding = [Text.UTF8Encoding]::new($false)
        [IO.File]::WriteAllText($temporary, ($report | ConvertTo-Json -Depth 16), $encoding)
        Move-Item -LiteralPath $temporary -Destination $final -Force
        [IO.File]::WriteAllText((Join-Path $outputDirectory 'complete.marker'), $report.EndUtc, $encoding)
        Write-Output "Report published: $final Passed=$($report.Passed)"
        if (-not $keepSession) { & (Join-Path $env:SystemRoot 'System32\shutdown.exe') /s /t 0 }
    } catch { Write-Error -Message ("Report persistence failed; keep Sandbox open: " + $_.Exception.ToString()) -ErrorAction Continue }
}
if (-not $report.Passed) { exit 1 }
