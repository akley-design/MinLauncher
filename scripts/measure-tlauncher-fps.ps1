[CmdletBinding()]
param(
    [ValidateRange(1, 100)] [int] $Runs = 30,
    [ValidateRange(60, 900)] [int] $MinimumRunSeconds = 120,
    [string] $TLauncherPath = (Join-Path $env:APPDATA '.minecraft\TLauncher.exe'),
    [string] $ReferenceGameDirectory = 'E:\C_Offload\MinLauncher\benchmarks\TLauncherReference',
    [string] $ProfileName = 'Forge 1.20.1',
    [string] $GameVersion = '1.20.1',
    [string] $AgentPath = (Join-Path $env:APPDATA 'MinLauncher\agent\minlaucher-agent.jar'),
    [string] $OutputPath = '',
    [switch] $ValidateOnly
)

$ErrorActionPreference = 'Stop'
$settingsPath = Join-Path $env:APPDATA '.tlauncher\tlauncher-2.0.properties'
$benchmarkDirectory = Join-Path $env:APPDATA 'MinLauncher\benchmarks'
$sourceGameDirectory = [System.IO.Path]::GetFullPath((Join-Path $env:APPDATA '.minecraft'))
$referenceGameDirectory = [System.IO.Path]::GetFullPath($ReferenceGameDirectory)
if (-not $OutputPath) {
    $OutputPath = Join-Path $benchmarkDirectory ("tlauncher-fps-{0}.json" -f (Get-Date -Format 'yyyyMMdd-HHmmss'))
}
$OutputPath = [System.IO.Path]::GetFullPath($OutputPath)
$reportTemplate = Join-Path $benchmarkDirectory 'tlauncher-agent-{pid}.json'

if ($referenceGameDirectory.Equals($sourceGameDirectory, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw 'The TLauncher reference path must be separate from the source .minecraft directory.'
}
if (-not (Test-Path -LiteralPath $TLauncherPath -PathType Leaf)) { throw "TLauncher not found: $TLauncherPath" }
if (-not (Test-Path -LiteralPath $AgentPath -PathType Leaf)) { throw "Build or install the MinLauncher frame agent first: $AgentPath" }
if (-not (Test-Path -LiteralPath $settingsPath -PathType Leaf)) { throw "TLauncher settings not found: $settingsPath" }
if (-not (Test-Path -LiteralPath (Join-Path $referenceGameDirectory "versions\$ProfileName\$ProfileName.json") -PathType Leaf)) {
    throw "The copied Forge profile was not found under $referenceGameDirectory. Run prepare-tlauncher-reference.ps1 first."
}
foreach ($directory in @('assets', 'libraries', 'runtime', 'versions', 'mods', 'saves')) {
    $path = Join-Path $referenceGameDirectory $directory
    if (-not (Test-Path -LiteralPath $path -PathType Container) -or (Get-Item -LiteralPath $path -Force).LinkType) {
        throw "The reference copy is missing '$directory' or still contains a link; refusing to measure."
    }
}
if ($env:COMPUTERNAME -eq '') { throw 'The benchmark must be bound to this PC.' }

$runningJava = @(Get-CimInstance Win32_Process -Filter "Name='javaw.exe' OR Name='java.exe'" -ErrorAction SilentlyContinue)
if ($runningJava | Where-Object { $_.CommandLine -match 'TLauncher|tlauncher|cpw\.mods\.bootstraplauncher\.BootstrapLauncher' }) {
    throw 'Close TLauncher and Minecraft before starting the benchmark collector.'
}

$existingSamples = @()
$existingFrametimes = @()
if (Test-Path -LiteralPath $OutputPath -PathType Leaf) {
    $prior = Get-Content -LiteralPath $OutputPath -Raw | ConvertFrom-Json
    if ($prior.scenario -ne 'in_game/tlauncher' -or $prior.meta.launcher -ne 'TLauncher' -or $prior.meta.host -ne $env:COMPUTERNAME) {
        throw "The existing output file is not a resumable TLauncher benchmark for this PC: $OutputPath"
    }
    $existingSamples = @($prior.samples.in_game_fps)
    $existingFrametimes = @($prior.samples.in_game_frametime)
    if ($existingSamples.Count -ne $existingFrametimes.Count -or $existingSamples.Count -ge $Runs) {
        throw 'The existing benchmark is complete or inconsistent; choose a new OutputPath or increase Runs.'
    }
} elseif ($ValidateOnly) {
    $stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
    $OutputPath = Join-Path $benchmarkDirectory "tlauncher-fps-$stamp.json"
}

if ($ValidateOnly) {
    Write-Host 'Validation passed. The Forge profile and the independent game directory are ready.'
    Write-Host "TLauncher setting will be restored after collection: $settingsPath"
    Write-Host "Samples will be saved here: $OutputPath"
    exit 0
}

New-Item -ItemType Directory -Path $benchmarkDirectory -Force | Out-Null
$priorSettings = [System.IO.File]::ReadAllBytes($settingsPath)
$settingsBackup = "$settingsPath.minlauncher-benchmark-backup"
if (Test-Path -LiteralPath $settingsBackup) {
    $existingBackup = [System.IO.File]::ReadAllBytes($settingsBackup)
    if ([Convert]::ToBase64String($existingBackup) -ne [Convert]::ToBase64String($priorSettings)) {
        throw "A different TLauncher settings backup already exists: $settingsBackup"
    }
} else {
    [System.IO.File]::WriteAllBytes($settingsBackup, $priorSettings)
}

function ConvertTo-JavaPropertyValue([string] $Value) {
    $builder = [System.Text.StringBuilder]::new()
    foreach ($character in $Value.ToCharArray()) {
        $code = [int]$character
        if ($character -eq '\') { [void]$builder.Append('\\') }
        elseif ($character -eq ':') { [void]$builder.Append('\:') }
        elseif ($code -gt 126) { [void]$builder.Append(('\u{0:x4}' -f $code)) }
        else { [void]$builder.Append($character) }
    }
    $builder.ToString()
}

$launcherProcess = $null
$priorJavaToolOptions = $env:JAVA_TOOL_OPTIONS
function Set-ReferenceGraphicsProfile {
    param([Parameter(Mandatory = $true)] [string] $GameDirectory)

    $optionsPath = Join-Path $GameDirectory 'options.txt'
    if (-not (Test-Path -LiteralPath $optionsPath -PathType Leaf)) {
        throw "The isolated TLauncher copy has no options.txt: $optionsPath"
    }
    $backupPath = "$optionsPath.minlaucher-benchmark-original"
    if (-not (Test-Path -LiteralPath $backupPath -PathType Leaf)) {
        Copy-Item -LiteralPath $optionsPath -Destination $backupPath
    }

    $options = [ordered]@{}
    foreach ($line in [System.IO.File]::ReadAllLines($optionsPath)) {
        $separator = $line.IndexOf(':')
        if ($separator -gt 0) {
            $options[$line.Substring(0, $separator)] = $line.Substring($separator + 1)
        }
    }
    foreach ($setting in @{
        renderDistance = '4'
        simulationDistance = '4'
        graphicsMode = '0'
        ao = 'false'
        renderClouds = '"false"'
        particles = '0'
        vsync = 'false'
        maxFps = '500'
        mipmapLevels = '0'
        entityShadows = 'false'
        biomeBlendRadius = '0'
        entityDistanceScaling = '0.5'
    }.GetEnumerator()) {
        $options[$setting.Key] = $setting.Value
    }

    $contents = @($options.GetEnumerator() | ForEach-Object { "{0}:{1}" -f $_.Key, $_.Value }) -join "`n"
    [System.IO.File]::WriteAllText($optionsPath, "$contents`n", [System.Text.UTF8Encoding]::new($false))
}

try {
    Set-ReferenceGraphicsProfile -GameDirectory $referenceGameDirectory
    $settingsText = [System.Text.Encoding]::UTF8.GetString($priorSettings).TrimStart([char]0xFEFF)
    if ($settingsText -notmatch '(?m)^minecraft\.gamedir=') {
        throw 'TLauncher has no minecraft.gamedir setting; no game was launched.'
    }
    $encodedReferencePath = ConvertTo-JavaPropertyValue $referenceGameDirectory
    $temporarySettings = [regex]::Replace($settingsText, '(?m)^minecraft\.gamedir=.*$', "minecraft.gamedir=$encodedReferencePath")
    [System.IO.File]::WriteAllText($settingsPath, $temporarySettings, [System.Text.Encoding]::ASCII)

    $fpsSamples = [System.Collections.Generic.List[double]]::new()
    $frameSamples = [System.Collections.Generic.List[double]]::new()
    foreach ($value in $existingSamples) { $fpsSamples.Add([double]$value) }
    foreach ($value in $existingFrametimes) { $frameSamples.Add([double]$value) }
    $meta = [ordered]@{
        launcher = 'TLauncher'
        game_version = $GameVersion
        os = 'windows'
        host = $env:COMPUTERNAME
        recorded_at = (Get-Date).ToUniversalTime().ToString('o')
        extra = [ordered]@{
            profile = $ProfileName
            graphics_profile = 'max_fps_matched'
            render_distance = '4'
            simulation_distance = '4'
            agent = 'MinLauncher frame counter'
            run_count = [string]$fpsSamples.Count
        }
    }
    $saveSamples = {
        param([string] $Destination)
        $meta.extra.run_count = [string]$fpsSamples.Count
        $document = [ordered]@{
            meta = $meta
            scenario = 'in_game/tlauncher'
            samples = [ordered]@{
                in_game_fps = @($fpsSamples.ToArray())
                in_game_frametime = @($frameSamples.ToArray())
            }
        }
        $temporary = "$Destination.tmp"
        [System.IO.File]::WriteAllText($temporary, ($document | ConvertTo-Json -Depth 8), [System.Text.UTF8Encoding]::new($false))
        Move-Item -LiteralPath $temporary -Destination $Destination -Force
    }

    $agentArguments = "-javaagent:$AgentPath -Dminlaucher.agent.output=$reportTemplate"
    $env:JAVA_TOOL_OPTIONS = if ($priorJavaToolOptions) { "$priorJavaToolOptions $agentArguments" } else { $agentArguments }
    Write-Host "Starting TLauncher with a separate copied game directory: $referenceGameDirectory"
    Write-Host 'The reference copy uses the same Max FPS graphics settings as MinLauncher (4 render/simulation chunks).'
    Write-Host "For every run, select '$ProfileName', load the same save, viewpoint, resolution and graphics settings, then play for at least $MinimumRunSeconds seconds."
    Write-Host "Collecting $Runs runs; completed samples are saved after every game exit."
    $launcherProcess = Start-Process -FilePath $TLauncherPath -WorkingDirectory (Split-Path -Parent $TLauncherPath) -PassThru

    $seenPids = [System.Collections.Generic.HashSet[int]]::new()
    for ($run = $fpsSamples.Count + 1; $run -le $Runs; $run++) {
        Write-Host "[$run/$Runs] Launch the selected Forge profile. Close Minecraft normally after the measurement period."
        $deadline = (Get-Date).AddMinutes(30)
        $game = $null
        while ((Get-Date) -lt $deadline) {
            $processes = Get-CimInstance Win32_Process -Filter "Name='javaw.exe' OR Name='java.exe'" -ErrorAction SilentlyContinue
            $game = $processes | Where-Object {
                -not $seenPids.Contains([int]$_.ProcessId) -and
                $_.CommandLine -match 'cpw\.mods\.bootstraplauncher\.BootstrapLauncher' -and
                $_.CommandLine -match [regex]::Escape($GameVersion)
            } | Select-Object -First 1
            if ($game) { break }
            Start-Sleep -Milliseconds 800
        }
        if (-not $game) { throw "Timed out waiting for Minecraft $GameVersion; no sample was fabricated." }

        $gamePid = [int]$game.ProcessId
        [void]$seenPids.Add($gamePid)
        $reportPath = $reportTemplate.Replace('{pid}', [string]$gamePid)
        Write-Host "[$run/$Runs] Game process $gamePid started. Hold the same in-game scene steady."
        while (Get-CimInstance Win32_Process -Filter "ProcessId=$gamePid" -ErrorAction SilentlyContinue) { Start-Sleep -Seconds 1 }

        $reportDeadline = (Get-Date).AddSeconds(30)
        while (-not (Test-Path -LiteralPath $reportPath -PathType Leaf) -and (Get-Date) -lt $reportDeadline) { Start-Sleep -Milliseconds 500 }
        if (-not (Test-Path -LiteralPath $reportPath -PathType Leaf)) { throw "No frame-agent report was written for run $run." }
        $report = Get-Content -LiteralPath $reportPath -Raw | ConvertFrom-Json
        if (-not $report.frames_counted -or [int64]$report.frames -lt 3000 -or [double]$report.uptime_nanos -lt ($MinimumRunSeconds * 1000000000.0)) {
            throw "Run $run was too short or the frame counter did not attach; its values were discarded."
        }
        $fps = [double]$report.average_fps
        $frameTime = [double]$report.one_percent_low_ms
        if ($frameTime -le 0) { $frameTime = [double]$report.p99_frametime_ms }
        if ($fps -le 0 -or $frameTime -le 0) { throw "Run $run returned invalid FPS/frame-time values." }
        $fpsSamples.Add($fps)
        $frameSamples.Add($frameTime)
        & $saveSamples $OutputPath
        Remove-Item -LiteralPath $reportPath -Force
        Write-Host ("[{0}/{1}] {2:N1} FPS; {3:N2} ms one-percent-low frametime." -f $run, $Runs, $fps, $frameTime)
    }
} finally {
    $env:JAVA_TOOL_OPTIONS = $priorJavaToolOptions
    $tlauncherProcesses = @(Get-CimInstance Win32_Process -Filter "Name='java.exe' OR Name='javaw.exe'" -ErrorAction SilentlyContinue | Where-Object { $_.CommandLine -match 'tlauncher' })
    if ($tlauncherProcesses.Count -gt 0) {
        Write-Host 'Close TLauncher normally, then press Enter so its original game directory setting can be restored.'
        [void](Read-Host)
        $tlauncherProcesses = @(Get-CimInstance Win32_Process -Filter "Name='java.exe' OR Name='javaw.exe'" -ErrorAction SilentlyContinue | Where-Object { $_.CommandLine -match 'tlauncher' })
    }
    if ($tlauncherProcesses.Count -eq 0) {
        [System.IO.File]::WriteAllBytes($settingsPath, $priorSettings)
    } else {
        Write-Warning "TLauncher is still open. Its original settings remain backed up at $settingsBackup; close TLauncher before restoring them."
    }
}

Write-Host "TLauncher FPS baseline is ready: $OutputPath"
Write-Host 'Import this file on MinLauncher → Производительность → Импортировать замеры TLauncher.'
