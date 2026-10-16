param(
    [string]$BricsCad = 'D:\Bricsys\BricsCAD\bricscad.exe',
    [string]$OutputDirectory = 'target\bricscad-issues127-132',
    [int]$TimeoutSeconds = 90,
    [string]$Filter = '*',
    [string[]]$InputFiles,
    [switch]$SkipGenerate,
    [switch]$WriteNativeFixtures
)

$ErrorActionPreference = 'Stop'
$repository = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
if (-not $SkipGenerate) {
    & cargo run --example issue_regressions_bricscad -- $OutputDirectory
    if ($LASTEXITCODE -ne 0) { throw 'Failed to generate native-validation inputs.' }
}
$output = (Resolve-Path (Join-Path $repository $OutputDirectory)).Path
$lisp = (Join-Path $PSScriptRoot 'issue_regressions.lsp').Replace('\', '/')
$results = @()
$inputs = if ($InputFiles) { $InputFiles } else { Get-Content -LiteralPath (Join-Path $output 'inputs.txt') }
foreach ($name in $inputs) {
    if ($name -notlike $Filter) { continue }
    $inputPath = Join-Path $output $name
    if (-not (Test-Path -LiteralPath $inputPath)) { throw "Missing native-validation input: $inputPath" }
    $reportPath = Join-Path $output ($name + '.report.txt')
    if (Test-Path -LiteralPath $reportPath) { Remove-Item -LiteralPath $reportPath }
    $scriptPath = Join-Path $output ($name + '.scr')
    $nativePath = if ($WriteNativeFixtures) { '"' + (Join-Path $output ($name + '.native.dwg')).Replace('\', '/') + '"' } else { 'nil' }
    $modern = if ($name -like 'modern*') { 'T' } else { 'nil' }
    $script = '(setvar "FILEDIA" 0)' + "`r`n" + '_.OPEN' + "`r`n" + '"' + $inputPath.Replace('\', '/') + '"' + "`r`n" + '(load "' + $lisp + '")' + "`r`n" + '(ar:run "' + $reportPath.Replace('\', '/') + '" ' + $modern + ' ' + $nativePath + ')' + "`r`n"
    Set-Content -LiteralPath $scriptPath -Value $script -Encoding ascii
    Write-Output "Native validation: $name"
    $process = Start-Process -FilePath $BricsCad -ArgumentList ('/nologo /b "' + $scriptPath + '"') -WorkingDirectory (Split-Path $BricsCad) -WindowStyle Hidden -PassThru
    $createdProcessId = $process.Id
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    while (-not $process.HasExited -and [DateTime]::UtcNow -lt $deadline) {
        if ($process.WaitForExit(1000)) { break }
        $process.Refresh()
    }
    $completed = Test-Path -LiteralPath $reportPath
    $report = if ($completed) { Get-Content -LiteralPath $reportPath -Raw } else { '' }
    if (-not $process.HasExited) {
        Stop-Process -Id $createdProcessId -Force
    }
    $auditPath = Join-Path $output ([IO.Path]::GetFileNameWithoutExtension($name) + '.adt')
    $audit = if (Test-Path -LiteralPath $auditPath) { Get-Content -LiteralPath $auditPath -Raw } else { '' }
    $status = if ($report -match 'COMPLETE\s+T' -and $report -notmatch '"FAIL"|ERROR\s' -and $audit -match 'Errors\s*:\s*0') { 'PASS' } else { 'FAIL' }
    $results += [pscustomobject]@{ Input = $name; ProcessId = $createdProcessId; Status = $status; Report = $reportPath }
    Write-Output "$status $name (created PID $createdProcessId)"
}
$results | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $output 'native-results.json') -Encoding utf8
$results | Format-Table -AutoSize
if ($results.Status -contains 'FAIL') { exit 1 }
