param(
    [string]$Camel = "",
    [Parameter(Mandatory = $true)][string]$Model1,
    [Parameter(Mandatory = $true)][string]$Model2,
    [Parameter(Mandatory = $true)][string]$Model3,
    [Parameter(Mandatory = $true)][string]$Model4,
    [Parameter(Mandatory = $true)][string]$Model5,
    [ValidateRange(512, 100000)]
    [int]$PromptChars = 8192,
    [ValidateRange(1, 3600)]
    [int]$TimeoutSeconds = 600,
    [ValidateRange(1, 10000000)]
    [int]$TokenBudget = 32768,
    [switch]$ValidateOnly,
    [switch]$KeepArtifacts
)

$ErrorActionPreference = "Stop"
$Models = @($Model1, $Model2, $Model3, $Model4, $Model5)

if (-not $Camel) {
    $localCli = Join-Path $PSScriptRoot "../runtime/target/release/camel.exe"
    if (Test-Path -LiteralPath $localCli -PathType Leaf) {
        $Camel = $localCli
    } else {
        $command = Get-Command camel -ErrorAction SilentlyContinue
        if (-not $command) { throw "Pass -Camel <path> or put camel.exe on PATH." }
        $Camel = $command.Source
    }
}
$Camel = (Resolve-Path -LiteralPath $Camel).Path
if (-not (Test-Path -LiteralPath $Camel -PathType Leaf)) { throw "Camel executable not found: $Camel" }
if (@($Models | ForEach-Object { $_.Trim() } | Where-Object { $_ } | Select-Object -Unique).Count -ne 5) {
    throw "Supply five non-empty, distinct model IDs with -Models."
}

$temporaryRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$work = Join-Path $temporaryRoot ("camel-stress-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $work | Out-Null

function Invoke-CamelExpectedFailure {
    param([string]$Name, [string[]]$Arguments, [string]$ExpectedText)
    $output = & $Camel @Arguments 2>&1
    $code = $LASTEXITCODE
    $text = $output -join [Environment]::NewLine
    if ($code -eq 0) { throw "$Name unexpectedly succeeded." }
    if ($text -notmatch [regex]::Escape($ExpectedText)) {
        throw "$Name failed for the wrong reason (exit $code):`n$text"
    }
    Write-Host "PASS $Name (expected exit $code)"
}

try {
    $stepSource = Join-Path $work "step-budget.camel"
    [IO.File]::WriteAllText($stepSource, @'
workflow int Main() {
    int value = 0;
    while (true) { value = value + 1; }
    return value;
}
'@, [Text.UTF8Encoding]::new($false))
    Invoke-CamelExpectedFailure "step budget" @("run", $stepSource, "--steps", "128") "execution step budget exhausted"

    $toolSource = Join-Path $work "tool-budget.camel"
    [IO.File]::WriteAllText($toolSource, @'
tool int Measure(string value) { handler = "text.length"; }
workflow int Main() {
    Measure("first");
    Measure("second");
    return 0;
}
'@, [Text.UTF8Encoding]::new($false))
    Invoke-CamelExpectedFailure "run-wide tool-call budget" @("run", $toolSource, "--tool-calls", "1") "tool-call budget exhausted"

    $deadlineSource = Join-Path $work "deadline.camel"
    [IO.File]::WriteAllText($deadlineSource, "workflow int Main() { while (true) { } return 0; }", [Text.UTF8Encoding]::new($false))
    Invoke-CamelExpectedFailure "total run deadline" @("run", $deadlineSource, "--steps", "100000000", "--timeout-seconds", "1") "execution deadline exceeded"

    $modelsSource = Join-Path $work "five-model-stress.camel"
    $template = @'
version "0.2";
record Review { string summary; string[] risks; int score; }
agent Reviewer1 { model = __MODEL_1__; system = "Return a concise structured stress-test review. Use score 0..100 and list concrete risks."; response = Review; maxTurns = 1; retries = 0; }
agent Reviewer2 { model = __MODEL_2__; system = "Return a concise structured stress-test review. Use score 0..100 and list concrete risks."; response = Review; maxTurns = 1; retries = 0; }
agent Reviewer3 { model = __MODEL_3__; system = "Return a concise structured stress-test review. Use score 0..100 and list concrete risks."; response = Review; maxTurns = 1; retries = 0; }
agent Reviewer4 { model = __MODEL_4__; system = "Return a concise structured stress-test review. Use score 0..100 and list concrete risks."; response = Review; maxTurns = 1; retries = 0; }
agent Reviewer5 { model = __MODEL_5__; system = "Return a concise structured stress-test review. Use score 0..100 and list concrete risks."; response = Review; maxTurns = 1; retries = 0; }
workflow Review[] Main(string prompt) {
    Task<AgentResult> work1 = spawn Reviewer1.run(prompt);
    Task<AgentResult> work2 = spawn Reviewer2.run(prompt);
    Task<AgentResult> work3 = spawn Reviewer3.run(prompt);
    Task<AgentResult> work4 = spawn Reviewer4.run(prompt);
    Task<AgentResult> work5 = spawn Reviewer5.run(prompt);
    AgentResult result1 = await work1;
    AgentResult result2 = await work2;
    AgentResult result3 = await work3;
    AgentResult result4 = await work4;
    AgentResult result5 = await work5;
    return [result1.data, result2.data, result3.data, result4.data, result5.data];
}
'@
    for ($index = 0; $index -lt 5; $index++) {
        $literal = ConvertTo-Json -InputObject $Models[$index].Trim() -Compress
        $template = $template.Replace("__MODEL_$($index + 1)__", $literal)
    }
    [IO.File]::WriteAllText($modelsSource, $template, [Text.UTF8Encoding]::new($false))

    $checkOutput = & $Camel check $modelsSource 2>&1
    if ($LASTEXITCODE -ne 0) { throw "Five-model stress program did not compile:`n$($checkOutput -join [Environment]::NewLine)" }
    Write-Host "PASS five-model stress program compiles"
    if ($ValidateOnly) {
        Write-Host "Validation only; no model requests sent."
        return
    }

    $seed = "Review this release candidate. Identify correctness risks, resource exhaustion risks, and missing safety controls. Be specific and return only the declared Review record. Supporting context: the system must handle concurrent work, bounded execution, model failures, typed outputs, and safe cancellation. `n"
    $prompt = $seed
    while ($prompt.Length -lt $PromptChars) { $prompt += $seed }
    $prompt = $prompt.Substring(0, $PromptChars)
    Write-Host "Running five distinct models concurrently; prompt length $($prompt.Length), token budget $TokenBudget, deadline ${TimeoutSeconds}s."
    $endpoint = if ($env:CAMEL_LM_ENDPOINT) { $env:CAMEL_LM_ENDPOINT } else { "http://localhost:1234" }
    Write-Host "Endpoint: $endpoint"

    $arguments = @(
        "run", $modelsSource, "--json", "--steps", "100000", "--tool-calls", "256",
        "--tokens", "$TokenBudget", "--timeout-seconds", "$TimeoutSeconds", "--", $prompt
    )
    $output = & $Camel @arguments 2>&1
    $code = $LASTEXITCODE
    $text = $output -join [Environment]::NewLine
    if ($code -ne 0) { throw "Five-model stress run failed (exit $code):`n$text" }
    try { $execution = $text | ConvertFrom-Json } catch { throw "Camel did not return valid JSON:`n$text" }
    $reviews = @($execution.value)
    if ($reviews.Count -ne 5) { throw "Expected 5 model reviews, received $($reviews.Count)." }
    for ($index = 0; $index -lt 5; $index++) {
        $review = $reviews[$index]
        if ([string]::IsNullOrWhiteSpace($review.summary)) { throw "Model $($index + 1) returned an empty summary." }
        if ($null -eq $review.risks -or $review.score -lt 0 -or $review.score -gt 100) {
            throw "Model $($index + 1) returned an invalid typed review."
        }
        Write-Host "PASS model $($index + 1): $($Models[$index]) (score $($review.score), $(@($review.risks).Count) risks)"
    }
    Write-Host "PASS five-model parallel run; logs captured: $(@($execution.logs).Count)"
    Write-Host "Note: this exercises current runtime limits and provider-reported token accounting. It does not test the still-open checkpoint, replay, human-approval, evidence-provenance, or library-lock release gates."
} finally {
    if (-not $KeepArtifacts) {
        $resolvedWork = [IO.Path]::GetFullPath($work)
        if ($resolvedWork.StartsWith($temporaryRoot, [StringComparison]::OrdinalIgnoreCase) -and $resolvedWork -match 'camel-stress-[0-9a-f]{32}$') {
            Remove-Item -LiteralPath $resolvedWork -Recurse -Force
        }
    } else {
        Write-Host "Stress artifacts kept at $work"
    }
}
