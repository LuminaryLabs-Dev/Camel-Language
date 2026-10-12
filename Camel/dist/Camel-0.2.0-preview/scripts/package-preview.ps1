param(
    [string]$Output = "Camel/dist/Camel-0.2.0-preview.zip"
)
$ErrorActionPreference = "Stop"
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$destination = if ([IO.Path]::IsPathRooted($Output)) {
    [IO.Path]::GetFullPath($Output)
} else {
    [IO.Path]::GetFullPath((Join-Path $repositoryRoot $Output))
}
if (Test-Path -LiteralPath $destination) {
    throw "Archive already exists: $destination. Choose a different -Output to preserve it."
}
$files = [Collections.Generic.List[object]]::new()
foreach ($relative in @("README.md", "LICENSE", ".gitignore", "memory.md", "runtime/Cargo.toml", "runtime/Cargo.lock", "runtime/README.md", ".github/workflows/runtime.yml", "scripts/package-preview.ps1")) {
    $files.Add(@{ Source = Join-Path $repositoryRoot $relative; Entry = $relative })
}
foreach ($relativeDirectory in @("runtime/src", "runtime/tests", "docs", "Camel/examples")) {
    $directory = Join-Path $repositoryRoot $relativeDirectory
    foreach ($file in Get-ChildItem -LiteralPath $directory -File -Recurse) {
        $entry = [IO.Path]::GetRelativePath($repositoryRoot, $file.FullName).Replace('\', '/')
        $files.Add(@{ Source = $file.FullName; Entry = $entry })
    }
}
$files.Add(@{ Source = Join-Path $repositoryRoot "Camel/docs/AUDIO_NOTE_SUMMARIZER.md"; Entry = "Camel/docs/AUDIO_NOTE_SUMMARIZER.md" })
foreach ($binary in @("camel.exe", "camel-audio-runtime.exe")) {
    $files.Add(@{ Source = Join-Path $repositoryRoot "runtime/target/release/$binary"; Entry = "bin/$binary" })
}
# Validate inputs before creating any archive. Only explicitly selected build artifacts enter it.
foreach ($file in $files) {
    if (-not (Test-Path -LiteralPath $file.Source -PathType Leaf)) {
        throw "Missing bundle input: $($file.Source). Build with cargo build --release --manifest-path runtime/Cargo.toml first."
    }
}
$parent = Split-Path -Parent $destination
New-Item -ItemType Directory -Path $parent -Force | Out-Null
$temporary = Join-Path $parent ([IO.Path]::GetRandomFileName() + ".zip")
Add-Type -AssemblyName System.IO.Compression.FileSystem
try {
    $archive = [IO.Compression.ZipFile]::Open($temporary, [IO.Compression.ZipArchiveMode]::Create)
    try {
        $manifest = @()
        foreach ($file in ($files | Sort-Object Entry)) {
            [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($archive, $file.Source, $file.Entry, [IO.Compression.CompressionLevel]::Optimal) | Out-Null
            $manifest += @{ path = $file.Entry; sha256 = (Get-FileHash -LiteralPath $file.Source -Algorithm SHA256).Hash.ToLowerInvariant() }
        }
        $entry = $archive.CreateEntry("BUNDLE_MANIFEST.json")
        $writer = [IO.StreamWriter]::new($entry.Open(), [Text.UTF8Encoding]::new($false))
        try { $writer.Write(($manifest | ConvertTo-Json -Depth 4)) } finally { $writer.Dispose() }
    } finally { $archive.Dispose() }
    Move-Item -LiteralPath $temporary -Destination $destination -ErrorAction Stop
} catch {
    if (Test-Path -LiteralPath $temporary -PathType Leaf) { Remove-Item -LiteralPath $temporary -Force }
    throw
}
Write-Output "Created $destination"
Write-Output (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash
