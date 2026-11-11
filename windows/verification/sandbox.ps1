# Formal standalone harness. Host builds/stages; executables run ONLY in Sandbox.
[CmdletBinding()]
param(
    [switch]$Run, [switch]$PrepareOnly, [switch]$SkipBuild, [switch]$KeepOpen,
    [string]$ArtifactDirectory = (Join-Path $env:TEMP ('panechord-windows-sandbox-' + [guid]::NewGuid().ToString('N'))),
    [string]$ApplicationExecutable, [string[]]$TestExecutables = @(),
    [string]$ProbeExecutable, [string[]]$ProbeCases = @(),
    [string[]]$StartupExecutables = @(),
    [string[]]$AdditionalTestExecutables = @(),
    # Explicit existing official extracted runtime, never arbitrary system DLLs.
    [string]$RuntimeDirectory,
    [ValidateSet('2.6.4', '2.4.0')][string]$RuntimeNuGetVersion = '2.4.0',
    # Legacy parameter retained only to reject old probe invocations clearly.
    [switch]$AllowRuntimeCompatibilityProbe,
    [string]$WebView2Directory,
    [ValidateRange(180, 240)][int]$WatchdogSeconds = 240
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Assert-PlainTree([string]$Path) {
    $items = @((Get-Item -LiteralPath $Path -Force))
    if ($items[0].PSIsContainer) { $items += @(Get-ChildItem -LiteralPath $Path -Recurse -Force) }
    foreach ($item in $items) {
        if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "Reparse point in staged input: $($item.FullName)" }
    }
}
function Assert-X64Executable([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf) -or [IO.Path]::GetExtension($Path) -ine '.exe') { throw "Explicit executable missing: $Path" }
    Assert-PlainTree $Path
    $stream = [IO.File]::OpenRead($Path); $reader = [IO.BinaryReader]::new($stream)
    try {
        if ($reader.ReadUInt16() -ne 0x5A4D) { throw "Not PE: $Path" }
        $stream.Position = 0x3C; $offset = $reader.ReadInt32()
        if ($offset -lt 64 -or $offset -gt ($stream.Length - 6)) { throw "Invalid PE: $Path" }
        $stream.Position = $offset
        if ($reader.ReadUInt32() -ne 0x4550 -or $reader.ReadUInt16() -ne 0x8664) { throw "Expected x64 PE: $Path" }
    } finally { $reader.Dispose(); $stream.Dispose() }
}
function Invoke-CargoJson([string[]]$CargoArguments, [string]$LogPath) {
    & cargo @CargoArguments 1> $LogPath
    if ($LASTEXITCODE -ne 0) { throw "Cargo exit $LASTEXITCODE; see $LogPath" }
    foreach ($line in [IO.File]::ReadLines($LogPath)) { if (-not [string]::IsNullOrWhiteSpace($line)) { $line | ConvertFrom-Json } }
}
function Assert-CriticalRuntime([string]$Directory, [string]$WebViewDirectory, [string[]]$Files, [string[]]$Directories) {
    foreach ($name in $Files) {
        $root = if ($name -eq 'Microsoft.Web.WebView2.Core.dll') { $WebViewDirectory } else { $Directory }
        $path = Join-Path $root $name
        if (-not (Test-Path -LiteralPath $path -PathType Leaf) -or (Get-Item -LiteralPath $path).Length -eq 0) { throw "Critical package file missing/empty: $path" }
    }
    foreach ($name in $Directories) {
        $path = Join-Path $Directory $name
        if (-not (Test-Path -LiteralPath $path -PathType Container) -or @(Get-ChildItem -LiteralPath $path -Recurse -File).Count -eq 0) { throw "Critical resource directory missing/empty: $path" }
    }
}
function Get-PinnedProvenance([string]$Directory, [string]$WebViewDirectory) {
    # Unchanged hashes of the inspected official P0 Runtime 2.4.0 / WebView2 1.0.4078.44.
    $hashes = @{
        'Microsoft.UI.Xaml.dll' = 'B241F72BFC9460E823D8A122FBDBC2E2D7AC6BA60B9E5BF981E9B5054D4555B5'
        'Microsoft.WindowsAppRuntime.dll' = '945795BF788DB870CAA0B97A010476B6225C6DF22758FE7B4EAEBADA044B4072'
        'Microsoft.Web.WebView2.Core.dll' = '09EB29EE95C89D9E8C5B26BAE58939C95AEC5AE195394ECD4C6C1728296C3D4A'
    }
    foreach ($name in @('Microsoft.UI.Xaml.dll', 'Microsoft.WindowsAppRuntime.dll', 'Microsoft.Web.WebView2.Core.dll')) {
        $root = if ($name -eq 'Microsoft.Web.WebView2.Core.dll') { $WebViewDirectory } else { $Directory }
        $path = Join-Path $root $name; Assert-PlainTree $path
        $signature = Get-AuthenticodeSignature -LiteralPath $path
        if ($signature.Status -ne 'Valid' -or $null -eq $signature.SignerCertificate -or $signature.SignerCertificate.Subject -notmatch 'O=Microsoft Corporation') { throw "Expected valid official Microsoft signature: $path" }
        $hash = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash
        if ($hash -cne $hashes[$name]) { throw "Pinned official Runtime 2.4.0 / WebView2 1.0.4078.44 hash mismatch: $path" }
        [ordered]@{ Name = $name; SHA256 = $hash; FileVersion = (Get-Item -LiteralPath $path).VersionInfo.FileVersion; Signer = $signature.SignerCertificate.Subject; SignatureStatus = [string]$signature.Status }
    }
}
function Assert-ExpandedManifest([string]$GeneratedManifest, [string]$TemplatePath, [string]$ActivationPath) {
    # Reconstruct the formal lossless table expansion, NOT the old full official
    # manifest versus the new template's placeholder. Compare all declarations,
    # including DLL association, threadingModel and the unchanged COM entries.
    $template = Get-Content -LiteralPath $TemplatePath -Raw -Encoding UTF8
    $slot = '<!-- ACTIVATION_CLASSES -->'
    if ([regex]::Matches($template, [regex]::Escape($slot)).Count -ne 1) { throw 'Expected exactly one formal activation expansion slot.' }
    $xml = [Text.StringBuilder]::new(); $fileOpen = $false; $prefix = ''
    $seen = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    foreach ($line in @(Get-Content -LiteralPath $ActivationPath -Encoding UTF8 | ForEach-Object { $_.Trim() } | Where-Object { $_ -and -not $_.StartsWith('#') })) {
        if ($line.StartsWith('@')) {
            $file = $line.Substring(1)
            if ($file -notmatch '^[a-zA-Z0-9._]+$') { throw 'Unsafe activation DLL asset.' }
            if ($fileOpen) { $null = $xml.Append("    </asmv3:file>`n") }
            $null = $xml.Append('    <asmv3:file name="' + $file + '">' + "`n")
            $fileOpen = $true; $prefix = ''
        } elseif ($line.EndsWith(':')) {
            $prefix = $line.Substring(0, $line.Length - 1)
            if ($prefix -notmatch '^[a-zA-Z0-9.]+$') { throw 'Unsafe activation namespace asset.' }
        } else {
            if (-not $fileOpen -or -not $prefix) { throw 'Activation class without DLL/namespace.' }
            foreach ($suffix in ($line -split '\s+')) {
                if ($suffix -notmatch '^[a-zA-Z0-9_]+$') { throw 'Unsafe activation class asset.' }
                $name = $prefix + '.' + $suffix
                if (-not $seen.Add($name)) { throw "Duplicate activation class: $name" }
                $null = $xml.Append('        <winrtv1:activatableClass name="' + $name + '" threadingModel="both"></winrtv1:activatableClass>' + "`n")
            }
        }
    }
    if (-not $fileOpen -or $seen.Count -ne 920) { throw 'Expected exactly 920 unique official activation classes.' }
    $null = $xml.Append("    </asmv3:file>`n")
    $expected = $template.Replace($slot, $xml.ToString())
    $actual = Get-Content -LiteralPath $GeneratedManifest -Raw -Encoding UTF8
    foreach ($text in @($expected, $actual)) {
        if (-not $text.Contains('<description>windows-reactor-self-contained</description>') -or
            [regex]::Matches($text, '<winrtv1:activatableClass ').Count -ne 920) { throw 'Incomplete formal self-contained manifest.' }
    }
    $expected = ([regex]::Replace($expected, '(?s)<!--.*?-->', '')).Replace("`r`n", "`n").Trim()
    $actual = ([regex]::Replace($actual, '(?s)<!--.*?-->', '')).Replace("`r`n", "`n").Trim()
    if ($actual -cne $expected) { throw 'Generated manifest differs from the formal 920-class activation table/COM declarations.' }
}
function Get-SandboxProcesses {
    @(Get-CimInstance Win32_Process -Filter "Name = 'WindowsSandbox.exe' OR Name = 'WindowsSandboxClient.exe' OR Name = 'WindowsSandboxRemoteSession.exe'")
}

# Exact P0 ownership/CreationDate cleanup contract, compatible with its 7 mocks.
function Stop-OwnedSandbox($Session, $Before, [datetime]$LaunchUtc, [string]$Configuration, [string]$Output) {
    # A launcher may hand off to another client. Only NEW client identities tied
    # to this .wsb or to our launch PID are eligible; never taskkill /IM, services,
    # or clients that existed before launch. Recheck CreationDate against PID reuse.
    $current = @(Get-SandboxProcesses)
    $owned = @($current | Where-Object {
        $candidate = $_
        $preexisting = @($Before | Where-Object {
            $_.ProcessId -eq $candidate.ProcessId -and $_.CreationDate -eq $candidate.CreationDate
        }).Count -gt 0
        -not $preexisting -and $candidate.CreationDate.ToUniversalTime() -ge $LaunchUtc -and
            (($candidate.ProcessId -eq $Session.Id -and -not $Session.HasExited) -or
             ($null -ne $candidate.CommandLine -and $candidate.CommandLine.IndexOf($Configuration, [StringComparison]::OrdinalIgnoreCase) -ge 0))
    })
    # Discover only Sandbox client descendants, not unrelated Hyper-V/server processes.
    do {
        $added = @($current | Where-Object {
            $_.CreationDate.ToUniversalTime() -ge $LaunchUtc -and $_.ProcessId -notin @($owned | ForEach-Object { $_.ProcessId }) -and
                $_.ParentProcessId -in @($owned | ForEach-Object { $_.ProcessId }) -and $_.ProcessId -notin @($Before | ForEach-Object { $_.ProcessId })
        })
        $owned += $added
    } while ($added.Count -gt 0)
    $results = @()
    foreach ($candidate in $owned) {
        $live = @(Get-CimInstance Win32_Process -Filter "ProcessId = $($candidate.ProcessId)")
        if ($live.Count -eq 0 -or $live[0].CreationDate -ne $candidate.CreationDate) { continue }
        $preference = $ErrorActionPreference
        try {
            $ErrorActionPreference = 'Continue'
            $message = & (Join-Path $env:SystemRoot 'System32\taskkill.exe') /PID $candidate.ProcessId /T /F 2>&1
            $code = $LASTEXITCODE
        } finally { $ErrorActionPreference = $preference }
        $after = @(Get-CimInstance Win32_Process -Filter "ProcessId = $($candidate.ProcessId)")
        $gone = $after.Count -eq 0 -or $after[0].CreationDate -ne $candidate.CreationDate
        $results += [ordered]@{ Name = $candidate.Name; ProcessId = $candidate.ProcessId; CreationDate = $candidate.CreationDate; ExitCode = $code; Gone = $gone; Message = ($message | Out-String).Trim() }
        Write-Output "Sandbox taskkill PID=$($candidate.ProcessId) exit=$code gone=$gone"
    }
    $remaining = @(Get-SandboxProcesses | Where-Object {
        $candidate = $_
        @($owned | Where-Object { $_.ProcessId -eq $candidate.ProcessId -and $_.CreationDate -eq $candidate.CreationDate }).Count -gt 0
    })
    [ordered]@{ LaunchPid = $Session.Id; Configuration = $Configuration; Taskkill = $results; RemainingOwned = @($remaining | Select-Object Name, ProcessId, CreationDate); Passed = ($remaining.Count -eq 0) } |
        ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $Output 'cleanup.json') -Encoding UTF8
    if ($remaining.Count -gt 0) { throw 'Owned Sandbox client remained after taskkill. See cleanup.json; no elevation/retry against other sessions was attempted.' }
}

$watcher = $null; $subscription = $null; $session = $null
$sandboxBefore = @(); $launchUtc = [datetime]::MinValue; $exitCode = 0
$eventId = 'panechord-windows-' + [guid]::NewGuid().ToString('N')
try {
    if ($Run -and $PrepareOnly) { throw 'Choose -Run or -PrepareOnly. Default is prepare only.' }
    if ($Run -and $KeepOpen) { throw '-Run always taskkills its owned Sandbox in finally; -KeepOpen requires separately managed prepare-only mode.' }
    if ($AllowRuntimeCompatibilityProbe -or $RuntimeNuGetVersion -ne '2.4.0') { throw 'Legacy compatibility probe / Runtime 2.6.4 is no longer supported. Published setup 0.100.0 requires exact Runtime 2.4.0; omit -AllowRuntimeCompatibilityProbe.' }
    $sandbox = Join-Path $env:SystemRoot 'System32\WindowsSandbox.exe'
    if (-not (Test-Path -LiteralPath $sandbox -PathType Leaf)) { throw 'Windows Sandbox unavailable. Owner must enable it separately; no install/elevation attempted.' }
    if (-not [Environment]::Is64BitOperatingSystem -or -not [Environment]::Is64BitProcess) { throw 'Use x64 PowerShell.' }
    $manifest = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\Cargo.toml'))
    $packageRoot = Split-Path -Parent $manifest
    $manifestTemplate = Join-Path $packageRoot 'assets\app.manifest'
    $activationList = Join-Path $packageRoot 'assets\activation.txt'
    $formalRuntimeList = Join-Path $packageRoot 'assets\runtime.txt'
    $verifier = Join-Path $PSScriptRoot 'verify-package.ps1'; $guest = Join-Path $PSScriptRoot 'sandbox-guest.ps1'
    $startup = Join-Path $PSScriptRoot 'sandbox-startup.ps1'
    # Local owned copy; no source-tree dependency on the historical P0 harness.
    $cleanupTests = Join-Path $PSScriptRoot 'sandbox-cleanup-tests.ps1'
    foreach ($path in @($manifest, $manifestTemplate, $activationList, $formalRuntimeList, $verifier, $guest, $startup, $cleanupTests)) {
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "Required input missing: $path" }
    }
    $artifacts = [IO.Path]::GetFullPath($ArtifactDirectory)
    if (Test-Path -LiteralPath $artifacts) { throw "Artifact directory must be NEW: $artifacts" }
    $null = New-Item -ItemType Directory -Path $artifacts
    Assert-PlainTree $artifacts
    $payload = Join-Path $artifacts 'payload'; $output = Join-Path $artifacts 'output'
    $null = New-Item -ItemType Directory -Path $payload, $output
    # Metadata is the ACTUAL root workspace graph, not the old isolated fixture.
    # Activate optional setup even when staging explicitly supplied executables.
    $records = @(Invoke-CargoJson @('metadata', '--manifest-path', $manifest, '--locked', '--offline', '--filter-platform', 'x86_64-pc-windows-msvc', '--features', 'self-contained,native-smoke', '--format-version=1') (Join-Path $artifacts 'metadata.json'))
    if ($records.Count -ne 1) { throw 'Expected one metadata document.' }
    $metadata = $records[0]
    if (@($metadata.packages | Where-Object { $_.name -match '^winio(?:-|$)' }).Count -gt 0) { throw 'Unexpected Winio package in workspace metadata.' }
    $windowsPackages = @($metadata.packages | Where-Object { $_.name -eq 'windows' -or $_.name.StartsWith('windows-') })
    foreach ($package in $windowsPackages) {
        if ($package.source -cne 'registry+https://github.com/rust-lang/crates.io-index') { throw "Windows dependency must use the published crates.io registry, not path/git/patch: $($package.name) $($package.source)" }
    }
    $setup = @($metadata.packages | Where-Object { $_.name -eq 'windows-reactor-setup' })
    $formal = @($metadata.packages | Where-Object { $_.name -eq 'panechord-windows' })
    $reactor = @($metadata.packages | Where-Object { $_.name -eq 'windows-reactor' })
    if ($setup.Count -ne 1 -or $formal.Count -ne 1 -or $reactor.Count -ne 1) { throw 'Expected exactly one formal panechord-windows/reactor/setup package.' }
    if ([IO.Path]::GetFullPath($formal[0].manifest_path) -ine $manifest -or $formal[0].id -notin @($metadata.workspace_members)) { throw 'Formal package does not match the parent workspace manifest.' }
    if ($setup[0].version -ne '0.100.0' -or $reactor[0].version -ne '0.100.0') { throw 'Expected locked published reactor/setup 0.100.0.' }
    $testTargets = @($formal[0].targets | Where-Object { $_.test -and ($_.kind -contains 'lib' -or $_.kind -contains 'example' -or $_.kind -contains 'test') })
    foreach ($kind in @('lib', 'example', 'test')) {
        if (@($testTargets | Where-Object { $_.kind -contains $kind }).Count -eq 0) { throw "Missing formal test target kind: $kind" }
    }
    if (@($testTargets | Where-Object { $_.name -eq 'build_assets' -and $_.kind -contains 'test' }).Count -ne 1) { throw 'Missing formal build_assets integration test target.' }
    $setupRoot = Split-Path -Parent $setup[0].manifest_path
    $setupSource = Get-Content -LiteralPath (Join-Path $setupRoot 'src\lib.rs') -Raw -Encoding UTF8
    if ($setupSource -notmatch 'const RUNTIME_VER: &str = "2\.4\.0";' -or $setupSource -notmatch 'const WEBVIEW2_VER: &str = "1\.0\.4078\.44";') { throw 'Setup source package versions changed; review staging contract first.' }
    $criticalFiles = @('Microsoft.UI.Xaml.dll', 'Microsoft.UI.Xaml.Controls.dll', 'Microsoft.UI.Windowing.Core.dll',
        'Microsoft.UI.Windowing.dll', 'Microsoft.UI.Input.dll', 'Microsoft.UI.Composition.OSSupport.dll', 'CoreMessagingXP.dll',
        'Microsoft.UI.pri', 'Microsoft.UI.Xaml.Controls.pri', 'resources.pri', 'Microsoft.UI.Xaml.Resources.common.dll',
        'Microsoft.UI.Xaml.Resources.19h1.dll', 'Microsoft.WindowsAppRuntime.dll', 'Microsoft.WindowsAppRuntime.pri', 'Microsoft.Web.WebView2.Core.dll')
    $criticalDirectories = @('Microsoft.UI.Xaml', 'en-us')
    $runtimeList = Join-Path $setupRoot 'assets\runtime.txt'
    $runtimeNames = @(Get-Content -LiteralPath $runtimeList -Encoding UTF8 | ForEach-Object { $_.Trim() } | Where-Object { $_ })
    if ($runtimeNames.Count -eq 0 -or @($runtimeNames | Sort-Object -Unique).Count -ne $runtimeNames.Count) { throw 'Empty/duplicate runtime asset list.' }
    foreach ($name in $runtimeNames) {
        if ($name -notmatch '^[a-zA-Z0-9][a-zA-Z0-9._-]*$' -or $name -in @('.', '..') -or $name -match '\.exe$') { throw "Unsafe runtime asset: $name" }
    }
    $formalRuntimeNames = @(Get-Content -LiteralPath $formalRuntimeList -Encoding UTF8 | ForEach-Object { $_.Trim().ToLowerInvariant() } | Where-Object { $_ })
    $officialRuntimeNames = @($runtimeNames | ForEach-Object { $_.ToLowerInvariant() })
    if ($formalRuntimeNames.Count -ne $runtimeNames.Count -or
        (($formalRuntimeNames | Sort-Object) -join "`n") -cne (($officialRuntimeNames | Sort-Object) -join "`n")) { throw 'Formal runtime allowlist differs from pinned official runtime.txt.' }
    foreach ($name in $criticalFiles + $criticalDirectories) {
        if ($name -ne 'Microsoft.Web.WebView2.Core.dll' -and $runtimeNames -inotcontains $name) { throw "Critical entry absent from pinned runtime.txt: $name" }
    }
    if ($SkipBuild) {
        if (-not $ApplicationExecutable -or $TestExecutables.Count -eq 0) { throw '-SkipBuild requires explicit application and --no-run test executable paths.' }
        if (@($TestExecutables | Sort-Object -Unique).Count -ne $TestExecutables.Count -or $TestExecutables.Count -ne $testTargets.Count) { throw '-SkipBuild requires one unique executable for EVERY formal library/example/integration test target, including build_assets.' }
    } else {
        if ($ApplicationExecutable -or $TestExecutables.Count -gt 0) { throw 'Explicit executable paths require -SkipBuild.' }
        # Preflight both complete packages AND extractions. Formal build.rs is
        # cache-only, and this harness never invokes setup's download helpers.
        $cache = Join-Path $env:LOCALAPPDATA 'windows-reactor-setup\temp'
        $cachedRuntime = Join-Path $cache 'Microsoft.WindowsAppSDK.Runtime-2.4.0\.msix_extract'
        $cachedWebView = Join-Path $cache 'Microsoft.Web.WebView2-1.0.4078.44\win-x64\native_uap'
        foreach ($path in @(
            (Join-Path $cache 'Microsoft.WindowsAppSDK.Runtime.2.4.0.nupkg'),
            (Join-Path $cache 'Microsoft.Web.WebView2.1.0.4078.44.nupkg'),
            (Join-Path $cache 'Microsoft.WindowsAppSDK.Runtime-2.4.0\MSIX\win10-x64\Microsoft.WindowsAppRuntime.2.msix'),
            (Join-Path $cache 'Microsoft.Web.WebView2-1.0.4078.44\win-x64\native_uap\Microsoft.Web.WebView2.Core.dll')
        )) { if (-not (Test-Path -LiteralPath $path -PathType Leaf) -or (Get-Item -LiteralPath $path).Length -eq 0) { throw "Official exact-version setup cache incomplete: $path. No download attempted. Use a separately reviewed explicit -SkipBuild staging route." } }
        Assert-PlainTree (Split-Path -Parent $cachedRuntime)
        Assert-PlainTree (Join-Path $cache 'Microsoft.Web.WebView2-1.0.4078.44')
        Assert-CriticalRuntime $cachedRuntime $cachedWebView $criticalFiles $criticalDirectories
        $null = @(Get-PinnedProvenance $cachedRuntime $cachedWebView)
        $build = @(Invoke-CargoJson @('build', '--manifest-path', $manifest, '-p', 'panechord-windows', '--locked', '--offline', '--features', 'self-contained,native-smoke', '--example', 'native_smoke', '--message-format=json') (Join-Path $artifacts 'build.jsonl'))
        $apps = @($build | Where-Object { $_.reason -eq 'compiler-artifact' -and $_.package_id -eq $formal[0].id -and $_.target.name -eq 'native_smoke' -and $_.target.kind -contains 'example' -and -not $_.profile.test -and $_.executable })
        if ($apps.Count -ne 1) { throw 'Expected exactly one formal native_smoke example application artifact.' }
        $ApplicationExecutable = $apps[0].executable
        $scripts = @($build | Where-Object { $_.reason -eq 'build-script-executed' -and $_.package_id -eq $formal[0].id })
        if ($scripts.Count -ne 1) { throw 'Expected one formal build script output directory.' }
        $generatedManifest = Join-Path $scripts[0].out_dir 'app.manifest'
        Assert-ExpandedManifest $generatedManifest $manifestTemplate $activationList
        # Formal build.rs embeds the SAME expanded manifest for examples, library
        # harnesses and integration tests. --no-run never executes host tests.
        $tests = @(Invoke-CargoJson @('test', '--manifest-path', $manifest, '-p', 'panechord-windows', '--locked', '--offline', '--features', 'self-contained,native-smoke', '--lib', '--examples', '--tests', '--no-run', '--message-format=json') (Join-Path $artifacts 'test-build.jsonl'))
        $testArtifacts = @($tests | Where-Object { $_.reason -eq 'compiler-artifact' -and $_.package_id -eq $formal[0].id -and $_.profile.test -and $_.executable })
        foreach ($target in $testTargets) {
            $matches = @($testArtifacts | Where-Object { $_.target.name -eq $target.name -and (($_.target.kind -join ',') -ceq ($target.kind -join ',')) })
            if ($matches.Count -ne 1) { throw "Missing/duplicate formal test artifact: $($target.name) ($($target.kind -join ','))" }
        }
        $testScripts = @($tests | Where-Object { $_.reason -eq 'build-script-executed' -and $_.package_id -eq $formal[0].id })
        if ($testScripts.Count -ne 1) { throw 'Expected one formal test build script output directory.' }
        Assert-ExpandedManifest (Join-Path $testScripts[0].out_dir 'app.manifest') $manifestTemplate $activationList
        $TestExecutables = @($testArtifacts | ForEach-Object { $_.executable } | Sort-Object -Unique)
        if ($TestExecutables.Count -eq 0 -or $TestExecutables.Count -ne $testTargets.Count) { throw 'Missing/extra formal test artifacts; refusing empty/partial success.' }
    }
    $ApplicationExecutable = (Resolve-Path -LiteralPath $ApplicationExecutable).Path
    Assert-X64Executable $ApplicationExecutable
    if ([bool]$ProbeExecutable -ne ($ProbeCases.Count -gt 0) -or $ProbeCases.Count -gt 24 -or
        @($ProbeCases | Sort-Object -Unique).Count -ne $ProbeCases.Count) { throw 'Specify one explicit probe executable and 1..24 unique cases together.' }
    foreach ($case in $ProbeCases) { if ($case -notmatch '^[a-z][a-z0-9-]{0,47}$') { throw "Invalid probe case: $case" } }
    if ($ProbeExecutable) {
        $ProbeExecutable = (Resolve-Path -LiteralPath $ProbeExecutable).Path
        Assert-X64Executable $ProbeExecutable
        Copy-Item -LiteralPath $ProbeExecutable -Destination (Join-Path $payload 'native-family.exe')
    }
    $startupEntries = @(); $startupIndex = 0
    foreach ($executable in $StartupExecutables) {
        $resolved = Resolve-Path -LiteralPath $executable
        if ($resolved.Provider.Name -ne 'FileSystem' -or $resolved.Path -notmatch '^[a-zA-Z]:\\[^:]*$') { throw "Expected local executable path without alternate streams: $executable" }
        $path = $resolved.Path; Assert-X64Executable $path
        for ($parent = Split-Path -Parent $path; $parent; $parent = Split-Path -Parent $parent) {
            if (((Get-Item -LiteralPath $parent -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "Reparse point in startup path: $parent" }
        }
        $startupIndex++; $name = "startup-$startupIndex.exe"
        Copy-Item -LiteralPath $path -Destination (Join-Path $payload $name)
        $startupEntries += [ordered]@{ FileName = $name; Source = $path }
    }
    $additionalTestEntries = @(); $additionalTestIndex = 0
    foreach ($executable in $AdditionalTestExecutables) {
        $resolved = Resolve-Path -LiteralPath $executable
        if ($resolved.Provider.Name -ne 'FileSystem' -or $resolved.Path -notmatch '^[a-zA-Z]:\\[^:]*$') { throw "Expected local executable path without alternate streams: $executable" }
        $path = $resolved.Path; Assert-X64Executable $path
        for ($parent = Split-Path -Parent $path; $parent; $parent = Split-Path -Parent $parent) {
            if (((Get-Item -LiteralPath $parent -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "Reparse point in additional test path: $parent" }
        }
        $additionalTestIndex++; $name = "additional-test-$additionalTestIndex.exe"
        Copy-Item -LiteralPath $path -Destination (Join-Path $payload $name)
        $additionalTestEntries += [ordered]@{ FileName = $name; Source = $path }
    }
    if (-not $RuntimeDirectory) { $RuntimeDirectory = Split-Path -Parent $ApplicationExecutable }
    $RuntimeDirectory = (Resolve-Path -LiteralPath $RuntimeDirectory).Path
    Assert-PlainTree $RuntimeDirectory
    # Compatibility filename only: source is the NEW panechord-windows
    # native_smoke example, never a historical fixture binary.
    Copy-Item -LiteralPath $ApplicationExecutable -Destination (Join-Path $payload 'panechord-windows-next.exe')
    $testNames = @(); $index = 0
    foreach ($test in $TestExecutables) {
        $path = (Resolve-Path -LiteralPath $test).Path; Assert-X64Executable $path
        $index++; $name = "test-$index.exe"; Copy-Item -LiteralPath $path -Destination (Join-Path $payload $name); $testNames += $name
    }
    $stagedRuntime = @(); $missingOptionalRuntime = @()
    foreach ($name in $runtimeNames) {
        $source = Join-Path $RuntimeDirectory $name
        if (Test-Path -LiteralPath $source) {
            Assert-PlainTree $source
            if (@(Get-ChildItem -LiteralPath $source -Recurse -File -Force | Where-Object { $_.Extension -ieq '.exe' }).Count -gt 0) { throw "Executable in runtime resource: $source" }
            Copy-Item -LiteralPath $source -Destination (Join-Path $payload $name) -Recurse; $stagedRuntime += $name
        } else { $missingOptionalRuntime += $name }
    }
    if (-not $WebView2Directory) { $WebView2Directory = $RuntimeDirectory }
    $webview = Join-Path $WebView2Directory 'Microsoft.Web.WebView2.Core.dll'
    Assert-PlainTree $webview; Copy-Item -LiteralPath $webview -Destination $payload
    Assert-CriticalRuntime $payload $payload $criticalFiles $criticalDirectories
    $provenance = @(Get-PinnedProvenance $payload $payload)
    # Only the official VS2022 Professional x64 CRT; no runtime installation.
    $redistRoot = Join-Path $env:ProgramFiles 'Microsoft Visual Studio\2022\Professional\VC\Redist\MSVC'
    $crt = [ordered]@{ Available = $false; Version = $null; Files = @() }
    if (Test-Path -LiteralPath $redistRoot -PathType Container) {
        $versions = @(Get-ChildItem -LiteralPath $redistRoot -Directory | Where-Object { $_.Name -match '^\d+\.\d+\.\d+(\.\d+)?$' } | Sort-Object { [version]$_.Name } -Descending)
        if ($versions.Count -gt 0) {
            $crtDirectory = Join-Path $versions[0].FullName 'x64\Microsoft.VC143.CRT'
            if (Test-Path -LiteralPath $crtDirectory -PathType Container) {
                Assert-PlainTree $crtDirectory; $dlls = @(Get-ChildItem -LiteralPath $crtDirectory -Filter '*.dll' -File)
                foreach ($required in @('vcruntime140.dll', 'vcruntime140_1.dll', 'msvcp140.dll')) { if ($dlls.Name -inotcontains $required) { throw "Official CRT incomplete: $required" } }
                foreach ($dll in $dlls) { Copy-Item -LiteralPath $dll.FullName -Destination $payload }
                $crt.Available = $true; $crt.Version = $versions[0].Name; $crt.Files = @($dlls.Name)
            }
        }
    }
    if (-not $crt.Available) { Write-Warning 'Official CRT unavailable; no installation attempted.' }
    Copy-Item -LiteralPath $verifier, $guest, $startup, $cleanupTests -Destination $payload
    Copy-Item -LiteralPath $PSCommandPath -Destination (Join-Path $payload 'sandbox.ps1')
    $cases = @(
        [ordered]@{ Name = 'make-error'; Arguments = '--make-error'; ExpectedExitCode = 1; ExpectedText = 'intentional make error'; Fault = $true },
        [ordered]@{ Name = 'task-panic'; Arguments = '--task-panic'; ExpectedExitCode = 1; ExpectedText = 'local task panicked'; Fault = $true },
        [ordered]@{ Name = 'reject-wake'; Arguments = '--reject-wake'; ExpectedExitCode = 1; ExpectedText = 'PostMessageW rejected wake'; Fault = $true },
        [ordered]@{ Name = 'p1-smoke'; Arguments = '--p1-smoke'; ExpectedExitCode = 0; ExpectedText = 'P1 SMOKE PASS'; Fault = $false }
    )
    $cases | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $payload 'cases.json') -Encoding UTF8
    $config = [ordered]@{
        SchemaVersion = 1; KeepOpen = [bool]$KeepOpen; HostManagedCleanup = [bool]$Run; ChildTimeoutSeconds = 30
        Application = 'panechord-windows-next.exe'; Tests = $testNames; CasesFile = 'cases.json'
        ProbeExecutable = $(if ($ProbeExecutable) { 'native-family.exe' } else { $null }); ProbeCases = $ProbeCases
        StartupExecutables = $startupEntries
        AdditionalTestExecutables = $additionalTestEntries
        SetupVersion = '0.100.0'; ReactorVersion = '0.100.0'; SetupRuntimeNuGetVersion = '2.4.0'
        RuntimeNuGetVersion = $RuntimeNuGetVersion; RuntimeCompatibilityProbe = $false; WebView2NuGetVersion = '1.0.4078.44'
        WindowsPackages = @($windowsPackages | Select-Object name, version, source)
        CriticalFiles = $criticalFiles; CriticalDirectories = $criticalDirectories; Crt = $crt
        StagedRuntimeEntries = $stagedRuntime; MissingOptionalRuntimeEntries = $missingOptionalRuntime
        RuntimeProvenance = $provenance
        RuntimeInput = $RuntimeDirectory; SetupManifestSHA256 = (Get-FileHash -LiteralPath (Join-Path $setupRoot 'assets\app.manifest') -Algorithm SHA256).Hash
        FormalManifestTemplateSHA256 = (Get-FileHash -LiteralPath $manifestTemplate -Algorithm SHA256).Hash
        FormalActivationSHA256 = (Get-FileHash -LiteralPath $activationList -Algorithm SHA256).Hash
        Host = [ordered]@{ PreparedUtc = [DateTime]::UtcNow.ToString('o'); CargoTargetDirectory = $metadata.target_directory; BuildSkipped = [bool]$SkipBuild
            PackageId = $formal[0].id; ApplicationTarget = 'native_smoke'; TestTargets = @($testTargets | Select-Object name, kind)
            ApplicationInput = $ApplicationExecutable; TestInputs = $TestExecutables
            AdditionalTestSourcePaths = @($additionalTestEntries | ForEach-Object { $_.Source })
            StartupSourcePaths = @($startupEntries | ForEach-Object { $_.Source }) }
    }
    $config | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $payload 'configuration.json') -Encoding UTF8
    Assert-PlainTree $payload
    $inventory = @(Get-ChildItem -LiteralPath $payload -Recurse -File | ForEach-Object {
        [ordered]@{ Name = $_.FullName.Substring($payload.Length + 1); Length = $_.Length; SHA256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash }
    })
    $inventory | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $payload 'inventory.json') -Encoding UTF8
    $inputXml = [Security.SecurityElement]::Escape($payload); $outputXml = [Security.SecurityElement]::Escape($output)
    # p0 mapping names are compatibility-only for the unmodified 7-case mocks.
    $commandXml = [Security.SecurityElement]::Escape('powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File C:\p0-input\sandbox-startup.ps1')
    $wsb = Join-Path $artifacts 'windows.wsb'
    @"
<Configuration>
  <VGpu>Disable</VGpu>
  <Networking>Disable</Networking>
  <ClipboardRedirection>Disable</ClipboardRedirection>
  <MappedFolders>
    <MappedFolder><HostFolder>$inputXml</HostFolder><SandboxFolder>C:\p0-input</SandboxFolder><ReadOnly>true</ReadOnly></MappedFolder>
    <MappedFolder><HostFolder>$outputXml</HostFolder><SandboxFolder>C:\p0-output</SandboxFolder><ReadOnly>false</ReadOnly></MappedFolder>
  </MappedFolders>
  <LogonCommand><Command>$commandXml</Command></LogonCommand>
</Configuration>
"@ | Set-Content -LiteralPath $wsb -Encoding UTF8
    Write-Output "Configuration: $wsb"; Write-Output "Output: $output"
    if ($Run) {
        $marker = Join-Path $output 'complete.marker'
        $watcher = [IO.FileSystemWatcher]::new($output, 'complete.marker'); $watcher.NotifyFilter = [IO.NotifyFilters]::FileName
        $subscription = Register-ObjectEvent -InputObject $watcher -EventName Created -SourceIdentifier $eventId
        $watcher.EnableRaisingEvents = $true; $clock = [Diagnostics.Stopwatch]::StartNew()
        if (-not (Test-Path -LiteralPath $marker -PathType Leaf)) {
            $sandboxBefore = @(Get-SandboxProcesses); $launchUtc = [DateTime]::UtcNow
            $session = Start-Process -FilePath $sandbox -ArgumentList ('"' + $wsb + '"') -PassThru
            Write-Output "Sandbox launch PID: $($session.Id) (no elevation)"
        }
        while (-not (Test-Path -LiteralPath $marker -PathType Leaf)) {
            $remaining = $WatchdogSeconds - $clock.Elapsed.TotalSeconds
            if ($remaining -le 0) { break }
            $event = Wait-Event -SourceIdentifier $eventId -Timeout ([int][Math]::Ceiling($remaining))
            if ($null -eq $event) { break }; Remove-Event -EventIdentifier $event.EventIdentifier
        }
        if (-not (Test-Path -LiteralPath $marker -PathType Leaf)) { throw "Sandbox watchdog expired; finally taskkills only owned clients. Output: $output" }
        $reportPath = Join-Path $output 'report.json'; $report = Get-Content -LiteralPath $reportPath -Raw -Encoding UTF8 | ConvertFrom-Json
        if ($report.Passed -ne $true) { throw "Sandbox verification failed; see $reportPath" }
        Write-Output "SANDBOX_PASS report=$reportPath reactor=0.100.0 setup=0.100.0 runtime=2.4.0"
    } else { Write-Output 'Prepared only. No application/tests or Sandbox executed.' }
} catch { Write-Error -Message $_.Exception.Message -ErrorAction Continue; $exitCode = 1 }
finally {
    try {
        if ($watcher) { $watcher.EnableRaisingEvents = $false; $watcher.Dispose() }
        if ($subscription) { Unregister-Event -SourceIdentifier $eventId; Remove-Event -SourceIdentifier $eventId -ErrorAction SilentlyContinue }
    } catch { Write-Error -Message $_.Exception.Message -ErrorAction Continue; $exitCode = 1 }
    if ($null -ne $session) {
        try { Stop-OwnedSandbox $session $sandboxBefore $launchUtc $wsb $output }
        catch { Write-Error -Message $_.Exception.Message -ErrorAction Continue; $exitCode = 1 }
        finally { $session.Dispose() }
    }
}
if ($exitCode -ne 0) { exit $exitCode }
