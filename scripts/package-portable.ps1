[CmdletBinding()]
param(
    [string] $Version = "local",
    [string] $OutputDirectory = "dist"
)

$ErrorActionPreference = "Stop"

$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$outputRoot = if ([System.IO.Path]::IsPathRooted($OutputDirectory)) {
    $OutputDirectory
} else {
    Join-Path $repositoryRoot $OutputDirectory
}
$safeVersion = $Version -replace "[^A-Za-z0-9._-]", "-"
$packageName = "aether-engine-windows-x64-$safeVersion"
$stage = Join-Path $outputRoot $packageName
$binary = Join-Path $repositoryRoot "target/release/aether-launcher.exe"

foreach ($requiredPath in @(
    $binary,
    (Join-Path $repositoryRoot "assets/hdr/newport_loft.hdr"),
    (Join-Path $repositoryRoot "scenes/13_clouds.ron")
)) {
    if (-not (Test-Path -LiteralPath $requiredPath -PathType Leaf)) {
        throw "Required release input is missing: $requiredPath"
    }
}

if (Test-Path -LiteralPath $stage) {
    throw "Package staging directory already exists: $stage. Choose a fresh output directory."
}

New-Item -ItemType Directory -Path $stage -Force | Out-Null
Copy-Item -LiteralPath $binary -Destination (Join-Path $stage "aether-launcher.exe")
Copy-Item -LiteralPath (Join-Path $repositoryRoot "assets") -Destination $stage -Recurse
Copy-Item -LiteralPath (Join-Path $repositoryRoot "scenes") -Destination $stage -Recurse

@"
Aether Engine portable build ($safeVersion)

Run aether-launcher.exe to open the scene browser.
Keep the assets and scenes folders beside the executable.
"@ | Set-Content -LiteralPath (Join-Path $stage "README.txt") -Encoding UTF8

New-Item -ItemType Directory -Path $outputRoot -Force | Out-Null
$archivePath = Join-Path $outputRoot "$packageName.zip"
$checksumPath = "$archivePath.sha256"
if (Test-Path -LiteralPath $archivePath) {
    throw "Release archive already exists: $archivePath"
}

$archiveArguments = @{
    Path = Join-Path $stage "*"
    DestinationPath = $archivePath
    CompressionLevel = "Optimal"
}
Compress-Archive @archiveArguments

Add-Type -AssemblyName System.IO.Compression.FileSystem
$archive = [System.IO.Compression.ZipFile]::OpenRead($archivePath)
try {
    $entries = @($archive.Entries | ForEach-Object { $_.FullName })
    foreach ($requiredEntry in @(
        "aether-launcher.exe",
        "assets/hdr/newport_loft.hdr",
        "scenes/13_clouds.ron",
        "README.txt"
    )) {
        if ($entries -notcontains $requiredEntry) {
            throw "Release archive is missing $requiredEntry"
        }
    }
} finally {
    $archive.Dispose()
}

$hash = (Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash.ToLowerInvariant()
"$hash  $(Split-Path -Leaf $archivePath)" |
    Set-Content -LiteralPath $checksumPath -Encoding ASCII

Write-Output "Created $archivePath"
Write-Output "SHA256 $hash"