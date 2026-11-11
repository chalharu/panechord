# Guest-only, dependency-free mocks. NEVER launch or terminate a real process.
[CmdletBinding()]
param()
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if ([Environment]::UserName -ine 'WDAGUtilityAccount') { throw 'Run cleanup unit tests only in Windows Sandbox.' }

# Load ONLY the function AST from the hash-verified staged host script. Do not
# dot-source its top-level build/launch logic, even in the guest.
$tokens = $null; $errors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot 'sandbox.ps1'), [ref]$tokens, [ref]$errors)
if ($errors.Count -ne 0) { throw 'Host script syntax errors.' }
$definition = $ast.FindAll({ param($node) $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Stop-OwnedSandbox' }, $true)
if ($definition.Count -ne 1) { throw 'Expected exactly one cleanup function.' }
Invoke-Expression $definition[0].Extent.Text

$script:Processes = @(); $script:Killed = @(); $script:Reuse = $false; $script:Reject = $false; $script:ExitRace = $false
function Get-SandboxProcesses { $script:Processes }
function Get-CimInstance {
    param($ClassName, $Filter)
    if ($ClassName -ne 'Win32_Process' -or $Filter -notmatch '^ProcessId = (\d+)$') { throw 'Unexpected CIM mock call.' }
    $id = [uint32]$Matches[1]
    if ($script:Reuse) { $script:Reuse = $false; $script:Processes[0].CreationDate = $script:Processes[0].CreationDate.AddMinutes(1) }
    $script:Processes | Where-Object { $_.ProcessId -eq $id }
}
function Join-Path {
    param($Path, $ChildPath)
    if ($ChildPath -eq 'System32\taskkill.exe') { return 'Invoke-TestTaskkill' }
    Microsoft.PowerShell.Management\Join-Path $Path $ChildPath
}
function Invoke-TestTaskkill {
    param($PidSwitch, [uint32]$TargetId, $TreeSwitch, $ForceSwitch)
    if ($PidSwitch -ne '/PID' -or $TreeSwitch -ne '/T' -or $ForceSwitch -ne '/F') { throw 'Unexpected taskkill mock arguments.' }
    $script:Killed += $TargetId
    $global:LASTEXITCODE = 0
    if ($script:Reject) { $global:LASTEXITCODE = 1; return 'mock rejection' }
    $script:Processes = @($script:Processes | Where-Object { $_.ProcessId -ne $TargetId -and $_.ParentProcessId -ne $TargetId })
    if ($script:ExitRace) { $global:LASTEXITCODE = 128 }
    'mock termination'
}
function Process([uint32]$Id, [uint32]$Parent, [datetime]$Created, [string]$Command = '') {
    [pscustomobject]@{ Name = 'WindowsSandbox.exe'; ProcessId = $Id; ParentProcessId = $Parent; CreationDate = $Created; CommandLine = $Command }
}
function Assert($Condition, [string]$Message) { if (-not $Condition) { throw $Message } }

$now = [datetime]::UtcNow
$configuration = 'C:\unique output & spaces\p0.wsb'
$session = [pscustomobject]@{ Id = 100; HasExited = $false }
$count = 0
foreach ($case in @('owned-and-existing', 'handoff', 'empty', 'preexisting-config', 'pid-reuse', 'exit-race', 'reject')) {
    $script:Killed = @(); $script:Reuse = $false; $script:Reject = $false; $script:ExitRace = $false
    $before = @(); $session.HasExited = $false
    switch ($case) {
        'owned-and-existing' {
            $before = @((Process 1 9 $now.AddMinutes(-1)))
            $script:Processes = @($before[0], (Process 100 9 $now.AddSeconds(1)), (Process 101 100 $now.AddSeconds(2)), (Process 200 9 $now.AddSeconds(1)))
        }
        'handoff' {
            $session.HasExited = $true
            $script:Processes = @((Process 110 9 $now.AddSeconds(1) ('sandbox "' + $configuration + '"')), (Process 111 110 $now.AddSeconds(2)))
        }
        'empty' { $session.HasExited = $true; $script:Processes = @() }
        'preexisting-config' {
            $before = @((Process 1 9 $now.AddMinutes(-1) $configuration)); $script:Processes = $before; $session.HasExited = $true
        }
        'pid-reuse' { $script:Processes = @((Process 100 9 $now.AddSeconds(1))); $script:Reuse = $true }
        'exit-race' { $script:Processes = @((Process 100 9 $now.AddSeconds(1))); $script:ExitRace = $true }
        'reject' { $script:Processes = @((Process 100 9 $now.AddSeconds(1))); $script:Reject = $true }
    }
    # Clone CIM-like snapshots: native CIM queries return separate objects, not
    # references to our mock mutable inventory (important for PID reuse).
    $snapshot = $script:Processes
    function Get-SandboxProcesses {
        $script:Processes | ForEach-Object { $_ | Select-Object Name, ProcessId, ParentProcessId, CreationDate, CommandLine }
    }
    $directory = Microsoft.PowerShell.Management\Join-Path 'C:\p0-output' ('cleanup-test-' + $case)
    $null = New-Item -ItemType Directory -Path $directory
    $threw = $false
    try { Stop-OwnedSandbox $session $before $now $configuration $directory }
    catch { $threw = $true; if ($case -ne 'reject') { throw } }
    $result = Get-Content -LiteralPath (Microsoft.PowerShell.Management\Join-Path $directory 'cleanup.json') -Raw -Encoding UTF8 | ConvertFrom-Json
    Assert ($result.Passed -eq ($case -ne 'reject')) "$case cleanup outcome"
    Assert ($threw -eq ($case -eq 'reject')) "$case exception outcome"
    switch ($case) {
        'owned-and-existing' {
            Assert ($script:Killed.Count -eq 1 -and $script:Killed[0] -eq 100) 'Only owned root should be taskkilled.'
            Assert ($script:Processes.Count -eq 2 -and $script:Processes[0].ProcessId -eq 1 -and $script:Processes[1].ProcessId -eq 200) 'Existing/unrelated client must survive.'
        }
        'handoff' { Assert ($script:Killed.Count -eq 1 -and $script:Killed[0] -eq 110) 'Config-linked handoff root must be killed.' }
        { $_ -in @('empty', 'preexisting-config', 'pid-reuse') } { Assert ($script:Killed.Count -eq 0) "$case must not taskkill anything" }
        'exit-race' { Assert ($result.Taskkill[0].ExitCode -eq 128 -and $result.Taskkill[0].Gone) 'Already-exited race is resolved by identity check.' }
        'reject' { Assert ($result.RemainingOwned.Count -eq 1) 'Rejected taskkill must fail closed.' }
    }
    $count++
}
Write-Output "SANDBOX_CLEANUP_TESTS_PASS cases=$count (mock processes only)"
