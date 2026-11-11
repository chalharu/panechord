# Guest-only bootstrap: report even a PS5.1 parse failure before guest finally can run.
$ErrorActionPreference = 'Stop'
if ([Environment]::UserName -ine 'WDAGUtilityAccount') { throw 'Sandbox account required.' }
$output = 'C:\p0-output'
$marker = Join-Path $output 'complete.marker'
Start-Transcript -Path (Join-Path $output 'startup.log') -Force
try {
    $tokens = $null; $errors = $null
    $null = [System.Management.Automation.Language.Parser]::ParseFile('C:\p0-input\sandbox-guest.ps1', [ref]$tokens, [ref]$errors)
    if ($errors.Count -gt 0) { throw ($errors | Out-String) }
    & 'C:\p0-input\sandbox-guest.ps1'
} catch {
    if (-not (Test-Path -LiteralPath $marker)) {
        @{ Passed = $false; Diagnostics = @($_.Exception.ToString()) } | ConvertTo-Json -Depth 4 |
            Set-Content -LiteralPath (Join-Path $output 'report.json') -Encoding UTF8
        Set-Content -LiteralPath $marker -Value 'bootstrap failure' -Encoding UTF8
    }
    Write-Error $_ -ErrorAction Continue
} finally { Stop-Transcript }
