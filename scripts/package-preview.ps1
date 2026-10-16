param(
    [string]$Output = "",
    [switch]$IncludeStudio,
    [string]$Target = ""
)
$ErrorActionPreference = "Stop"
$repositoryRoot = Split-Path -Parent $PSScriptRoot
$hostInfo = & rustc -vV
if ($LASTEXITCODE -ne 0) { throw "Rust is required to identify the build target." }
$hostTarget = (($hostInfo | Where-Object { $_ -match '^host: ' }) -replace '^host: ', '').Trim()
if (-not $Target) { $Target = $hostTarget }
if ($Target -notmatch '^(x86_64|aarch64)-(pc-windows-msvc|unknown-linux-gnu|apple-darwin)$') {
    throw "Unsupported distribution target: $Target. Use a Windows MSVC, Linux GNU, or macOS x86_64/aarch64 target."
}
$architecture = $Matches[1]
$platform = if ($Target.Contains('windows')) { 'windows' } elseif ($Target.Contains('darwin')) { 'macos' } else { 'linux' }
$extension = if ($platform -eq 'windows') { '.exe' } else { '' }
$setupName = if ($platform -eq 'windows') { 'Camel-Setup.exe' } else { 'camel-setup' }
$version = '0.2.0-preview'
if (-not $Output) {
    $studioSuffix = if ($IncludeStudio) { '-studio' } else { '' }
    $Output = "Camel/dist/Camel-$version-$platform-$architecture$studioSuffix-setup.zip"
}
$destination = if ([IO.Path]::IsPathRooted($Output)) { [IO.Path]::GetFullPath($Output) } else { [IO.Path]::GetFullPath((Join-Path $repositoryRoot $Output)) }
if (Test-Path -LiteralPath $destination) { throw "Archive already exists: $destination. Choose a different -Output to preserve it." }

function Get-BinaryPath([string]$crate, [string]$binary) {
    $specific = Join-Path $repositoryRoot "$crate/target/$Target/release/$binary$extension"
    if (Test-Path -LiteralPath $specific -PathType Leaf) { return $specific }
    $native = Join-Path $repositoryRoot "$crate/target/release/$binary$extension"
    if ($Target -eq $hostTarget -and (Test-Path -LiteralPath $native -PathType Leaf)) { return $native }
    throw "Missing $binary for $Target. Build $crate/Cargo.toml in release mode for that target first."
}

$files = [Collections.Generic.List[object]]::new()
foreach ($relative in @('README.md', 'LICENSE', '.gitignore', 'runtime/Cargo.toml', 'runtime/Cargo.lock', 'runtime/README.md', 'packages/README.md', 'packages/camel-library-manager/Cargo.toml', 'packages/camel-library-manager/Cargo.lock', 'packages/camel-library-manager/README.md', 'packages/camel-library-manager/LICENSE', 'installer/Cargo.toml', 'installer/Cargo.lock', 'installer/README.md', '.github/workflows/distribution.yml', 'scripts/package-preview.ps1', 'scripts/install.sh')) {
    $files.Add(@{ Source = Join-Path $repositoryRoot $relative; Entry = $relative })
}
foreach ($relativeDirectory in @('runtime/src', 'runtime/tests', 'packages/camel-library-manager/src', 'installer/src', 'libraries', 'docs', 'conformance', 'Camel/examples')) {
    foreach ($file in Get-ChildItem -LiteralPath (Join-Path $repositoryRoot $relativeDirectory) -File -Recurse) {
        $entry = [IO.Path]::GetRelativePath($repositoryRoot, $file.FullName).Replace('\', '/')
        $files.Add(@{ Source = $file.FullName; Entry = $entry })
    }
}
$files.Add(@{ Source = Join-Path $repositoryRoot 'Camel/docs/AUDIO_NOTE_SUMMARIZER.md'; Entry = 'Camel/docs/AUDIO_NOTE_SUMMARIZER.md' })
foreach ($binary in @('camel', 'camel-audio-runtime')) {
    $files.Add(@{ Source = Get-BinaryPath 'runtime' $binary; Entry = "bin/$binary$extension" })
}
$files.Add(@{ Source = Get-BinaryPath 'installer' 'camel-setup'; Entry = $setupName })
if ($platform -ne 'windows') {
    $launcher = if ($platform -eq 'macos') { 'Install-Camel.command' } else { 'install.sh' }
    $files.Add(@{ Source = Join-Path $repositoryRoot 'scripts/install.sh'; Entry = $launcher })
}
if ($IncludeStudio) {
    foreach ($relative in @('studio/Cargo.toml', 'studio/Cargo.lock', 'studio/README.md')) { $files.Add(@{ Source = Join-Path $repositoryRoot $relative; Entry = $relative }) }
    foreach ($relativeDirectory in @('studio/src', 'studio/tests')) {
        foreach ($file in Get-ChildItem -LiteralPath (Join-Path $repositoryRoot $relativeDirectory) -File -Recurse) {
            $entry = [IO.Path]::GetRelativePath($repositoryRoot, $file.FullName).Replace('\', '/')
            $files.Add(@{ Source = $file.FullName; Entry = $entry })
        }
    }
    $files.Add(@{ Source = Get-BinaryPath 'studio' 'camel-studio'; Entry = "bin/camel-studio$extension" })
}
foreach ($file in $files) {
    if (-not (Test-Path -LiteralPath $file.Source -PathType Leaf)) { throw "Missing distribution input: $($file.Source)" }
}
$parent = Split-Path -Parent $destination
New-Item -ItemType Directory -Path $parent -Force | Out-Null
$temporary = Join-Path $parent ([IO.Path]::GetRandomFileName() + '.zip')
Add-Type -AssemblyName System.IO.Compression.FileSystem
try {
    $archive = [IO.Compression.ZipFile]::Open($temporary, [IO.Compression.ZipArchiveMode]::Create)
    try {
        $manifest = @()
        $payload = @()
        foreach ($file in ($files | Sort-Object Entry)) {
            $entry = [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($archive, $file.Source, $file.Entry, [IO.Compression.CompressionLevel]::Optimal)
            if ($platform -ne 'windows' -and ($file.Entry.StartsWith('bin/') -or $file.Entry -eq $setupName -or $file.Entry -eq 'install.sh' -or $file.Entry -eq 'Install-Camel.command')) {
                $entry.ExternalAttributes = 33261 -shl 16
            }
            $record = @{ path = $file.Entry; sha256 = (Get-FileHash -LiteralPath $file.Source -Algorithm SHA256).Hash.ToLowerInvariant() }
            $manifest += $record
            if ($file.Entry.StartsWith('bin/') -or $file.Entry.StartsWith('docs/') -or $file.Entry.StartsWith('libraries/') -or $file.Entry.StartsWith('Camel/examples/') -or $file.Entry -in @('README.md', 'LICENSE', 'installer/README.md', 'packages/README.md', 'packages/camel-library-manager/README.md')) { $payload += $record }
        }
        $installerManifest = @{ formatVersion = 1; version = $version; os = $platform; arch = $architecture; files = $payload }
        foreach ($generated in @(@{ Name = 'BUNDLE_MANIFEST.json'; Value = $manifest }, @{ Name = 'INSTALLER_BUNDLE.json'; Value = $installerManifest })) {
            $entry = $archive.CreateEntry($generated.Name)
            $writer = [IO.StreamWriter]::new($entry.Open(), [Text.UTF8Encoding]::new($false))
            try { $writer.Write((ConvertTo-Json -InputObject $generated.Value -Depth 6)) } finally { $writer.Dispose() }
        }
    } finally { $archive.Dispose() }
    Move-Item -LiteralPath $temporary -Destination $destination -ErrorAction Stop
} catch {
    if (Test-Path -LiteralPath $temporary -PathType Leaf) { Remove-Item -LiteralPath $temporary -Force }
    throw
}
Write-Output "Created $destination"
Write-Output (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash
