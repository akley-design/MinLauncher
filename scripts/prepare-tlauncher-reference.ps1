[CmdletBinding()]
param(
    [string] $SourceGameDirectory = (Join-Path $env:APPDATA '.minecraft'),
    [string] $ReferenceGameDirectory = 'E:\C_Offload\MinLauncher\benchmarks\TLauncherReference'
)

$ErrorActionPreference = 'Stop'
$source = [System.IO.Path]::GetFullPath($SourceGameDirectory)
$target = [System.IO.Path]::GetFullPath($ReferenceGameDirectory)
if ($target.Equals($source, [System.StringComparison]::OrdinalIgnoreCase) -or
    $target.StartsWith($source.TrimEnd('\') + '\', [System.StringComparison]::OrdinalIgnoreCase)) {
    throw 'The reference directory must be separate from the source .minecraft directory.'
}
if (-not (Test-Path -LiteralPath $source -PathType Container)) {
    throw "Minecraft source directory not found: $source"
}
if (Test-Path -LiteralPath $target) {
    throw "Reference directory already exists; choose another path to avoid overwriting it: $target"
}

$required = @('assets', 'libraries', 'runtime', 'versions', 'mods')
foreach ($name in $required) {
    if (-not (Test-Path -LiteralPath (Join-Path $source $name) -PathType Container)) {
        throw "Required Minecraft directory is missing: $name"
    }
}

$parent = Split-Path -Parent $target
New-Item -ItemType Directory -Path $parent -Force | Out-Null
$stage = "$target.staging-$([guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Path $stage | Out-Null
try {
    # These are read-only inputs to an installed game profile. Copying them
    # keeps the game and launcher from writing through junctions into source.
    foreach ($name in $required) {
        $from = Join-Path $source $name
        $to = Join-Path $stage $name
        & robocopy $from $to /E /COPY:DAT /DCOPY:DAT /R:1 /W:1 /MT:8 /NFL /NDL /NJH /NJS /NP
        if ($LASTEXITCODE -ge 8) { throw "Could not copy Minecraft directory '$name' (robocopy exit $LASTEXITCODE)." }
    }

    # Copy mutable profile data, including worlds, instead of sharing it.
    foreach ($name in @('config', 'defaultconfigs', 'resourcepacks', 'shaderpacks', 'saves')) {
        $from = Join-Path $source $name
        if (-not (Test-Path -LiteralPath $from -PathType Container)) { continue }
        $to = Join-Path $stage $name
        & robocopy $from $to /E /COPY:DAT /DCOPY:DAT /R:1 /W:1 /MT:8 /NFL /NDL /NJH /NJS /NP
        if ($LASTEXITCODE -ge 8) { throw "Could not copy profile data '$name' (robocopy exit $LASTEXITCODE)." }
    }
    foreach ($name in @('options.txt', 'optionsof.txt', 'servers.dat', 'servers.dat_old', 'TlauncherProfiles.json')) {
        $from = Join-Path $source $name
        if (Test-Path -LiteralPath $from -PathType Leaf) { Copy-Item -LiteralPath $from -Destination (Join-Path $stage $name) }
    }

    Move-Item -LiteralPath $stage -Destination $target
} catch {
    if (Test-Path -LiteralPath $stage) { Remove-Item -LiteralPath $stage -Recurse -Force }
    throw
}

Write-Host "Isolated TLauncher game directory is ready: $target"
Write-Host 'Set TLauncher → Settings → Minecraft → Game directory to this path before collecting a baseline.'
Write-Host 'This copy includes the Forge profile files, mods, configs, options, and saves.'
$global:LASTEXITCODE = 0
