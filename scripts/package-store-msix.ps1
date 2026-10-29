param(
    [Parameter(Mandatory = $true)]
    [string]$IdentityName,

    [Parameter(Mandatory = $true)]
    [string]$IdentityPublisher,

    [string]$PublisherDisplayName = "Mingle",
    [string]$AppVersion = "0.1.1-beta",
    [string]$TargetDirectory = "C:\Users\Public\minlaucher-target",
    [string]$MakeAppxPath
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot

if ([string]::IsNullOrWhiteSpace($IdentityName) -or $IdentityName -match '[^A-Za-z0-9.-]') {
    throw "IdentityName must be the exact package identity from Partner Center and contain only letters, digits, dots, and hyphens."
}
if ([string]::IsNullOrWhiteSpace($IdentityPublisher)) {
    throw "IdentityPublisher must be the exact Publisher value from Partner Center."
}
if ($AppVersion -notmatch '^(\d+)\.(\d+)\.(\d+)(?:-[0-9A-Za-z.-]+)?$') {
    throw "AppVersion must use X.Y.Z or X.Y.Z-beta form."
}
$packageVersion = "$($Matches[1]).$($Matches[2]).$($Matches[3]).0"

if (-not $MakeAppxPath) {
    $candidateRoots = @(
        (Join-Path ${env:ProgramFiles(x86)} "Windows Kits\10\bin"),
        "C:\Users\Public\minlauncher-sdktools\bin"
    )
    foreach ($candidateRoot in $candidateRoots) {
        if (-not (Test-Path -LiteralPath $candidateRoot)) { continue }
        $MakeAppxPath = Get-ChildItem -Path $candidateRoot -Filter MakeAppx.exe -Recurse -ErrorAction SilentlyContinue |
            Where-Object { $_.FullName -match '\\x64\\MakeAppx\.exe$' } |
            Sort-Object FullName -Descending |
            Select-Object -First 1 -ExpandProperty FullName
        if ($MakeAppxPath) { break }
    }
}
if (-not $MakeAppxPath -or -not (Test-Path -LiteralPath $MakeAppxPath)) {
    throw "MakeAppx.exe was not found. Install the Windows SDK or pass -MakeAppxPath explicitly."
}

$releaseDirectory = Join-Path $TargetDirectory "release"
$exePath = Join-Path $releaseDirectory "minlaucher.exe"
if (-not (Test-Path -LiteralPath $exePath)) {
    throw "Release executable not found: $exePath. Build with npm run tauri:build:store first."
}
$webViewLoader = Get-ChildItem -Path $releaseDirectory -Filter WebView2Loader.dll -Recurse -ErrorAction SilentlyContinue |
    Select-Object -First 1 -ExpandProperty FullName
if (-not $webViewLoader) {
    throw "WebView2Loader.dll was not found under $releaseDirectory."
}

$outputDirectory = Join-Path $repoRoot "artifacts\store"
$stageDirectory = Join-Path $outputDirectory ("stage-" + [guid]::NewGuid().ToString("N"))
$assetsDirectory = Join-Path $stageDirectory "Assets"
New-Item -ItemType Directory -Path $assetsDirectory -Force | Out-Null
Copy-Item -LiteralPath $exePath -Destination (Join-Path $stageDirectory "minlaucher.exe")
Copy-Item -LiteralPath $webViewLoader -Destination (Join-Path $stageDirectory "WebView2Loader.dll")

$sourceIcon = Join-Path $repoRoot "crates\ml-desktop\icons\icon-source.png"
Add-Type -AssemblyName System.Drawing
$sourceImage = [System.Drawing.Image]::FromFile($sourceIcon)
try {
    foreach ($size in @(44, 50, 150)) {
        $bitmap = [System.Drawing.Bitmap]::new($size, $size)
        try {
            $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
            try {
                $graphics.Clear([System.Drawing.Color]::Transparent)
                $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
                $graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
                $graphics.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
                $graphics.DrawImage($sourceImage, 0, 0, $size, $size)
            } finally {
                $graphics.Dispose()
            }
            $name = switch ($size) {
                44 { "Square44x44Logo.png" }
                50 { "StoreLogo.png" }
                150 { "Square150x150Logo.png" }
            }
            $bitmap.Save((Join-Path $assetsDirectory $name), [System.Drawing.Imaging.ImageFormat]::Png)
        } finally {
            $bitmap.Dispose()
        }
    }
} finally {
    $sourceImage.Dispose()
}

$templatePath = Join-Path $repoRoot "apps\windows-msix\Package.appxmanifest.template"
$manifest = Get-Content -LiteralPath $templatePath -Raw
function Escape-Xml([string]$value) {
    return [System.Security.SecurityElement]::Escape($value)
}
$manifest = $manifest.Replace("{{IDENTITY_NAME}}", (Escape-Xml $IdentityName))
$manifest = $manifest.Replace("{{IDENTITY_PUBLISHER}}", (Escape-Xml $IdentityPublisher))
$manifest = $manifest.Replace("{{PUBLISHER_DISPLAY_NAME}}", (Escape-Xml $PublisherDisplayName))
$manifest = $manifest.Replace("{{VERSION}}", $packageVersion)
$manifestPath = Join-Path $stageDirectory "AppxManifest.xml"
[System.IO.File]::WriteAllText($manifestPath, $manifest, [System.Text.UTF8Encoding]::new($false))

$null = New-Item -ItemType Directory -Path $outputDirectory -Force
$outputPath = Join-Path $outputDirectory "MinLauncher_$($packageVersion)_x64.msix"
& $MakeAppxPath pack /h SHA256 /o /d $stageDirectory /p $outputPath
if ($LASTEXITCODE -ne 0) {
    throw "MakeAppx failed with exit code $LASTEXITCODE."
}
Write-Host "Created Store package: $outputPath"
