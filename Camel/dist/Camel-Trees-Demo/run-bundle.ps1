$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($env:CAMEL_LM_MODEL)) {
    $env:CAMEL_LM_MODEL = Read-Host "Enter the LM Studio model identifier"
}

Push-Location $PSScriptRoot
try {
    & .\camel.exe .\encyclopedia.camel --unbounded
    exit $LASTEXITCODE
}
finally {
    Pop-Location
}
