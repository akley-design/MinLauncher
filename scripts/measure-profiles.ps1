<#
.SYNOPSIS
    Measures the profiles of the flags of the virtual machine, one after another.

.DESCRIPTION
    The launcher can leave the flags of the version of the game alone, or it
    can change them by the measurement that was made on this machine. Both
    are measured here, with the protocol of `scripts\measure.ps1`, and the
    runs are made one after another: profile, profile, profile.

    The order matters. A machine that gets warmer or busier changes the
    numbers of every profile in the same way, and when the profiles are
    measured one after another, the difference between them is still the
    difference of the flags. When each profile were measured in its own
    long run, a change of the machine would be taken for a change of the
    flags.

    The profile is put into the settings of the launcher, and the launcher
    is started again for every run: the settings are read at the start, and
    that is the path a player would take.

    The numbers of the game itself - the frames and the memory - are read
    from the file that the launcher writes, and only the samples of the
    run that has just finished are taken: the file keeps the samples of
    every run, and the runs of the other profiles are in it too. At the
    end, the samples of every profile are put back into the file of that
    profile, so that no file holds the numbers of another profile.

.PARAMETER Runs
    How many times every profile is measured.

.PARAMETER Profiles
    The profiles to measure. Every profile is a name and the flags of the
    virtual machine that belong to it.

.PARAMETER Settings
    The settings of the launcher. The default is the file the launcher
    reads.

.PARAMETER Out
    The directory the files of the profiles are written into.

.PARAMETER Fresh
    Throws the files of the profiles away before the measurement starts.

    The samples of a file are the samples of one binary. When the launcher
    has been rebuilt, the samples of the old binary are the samples of
    another launcher, and they must not be mixed with the new ones.

.PARAMETER Report
    Writes the comparison of the profiles and does not start the game.

    The files of the profiles are the raw material of the comparison, and
    they are written by a measurement. When the measurement is over, the
    comparison is made out of them alone.

.PARAMETER Archive
    Where the archive of the classes is kept. The default is in the
    catalog of the launcher, next to its cache.

.EXAMPLE
    .\scripts\measure-profiles.ps1 -Runs 30 -Profiles default,measured

.EXAMPLE
    .\scripts\measure-profiles.ps1 -Report -Profiles default,measured
#>
param(
    [int] $Runs = 30,
    [string] $Profiles = "default,measured",
    [string] $Settings = (Join-Path $env:APPDATA "MinLauncher\settings.json"),
    [string] $Out = (Join-Path $PSScriptRoot "..\benchmarks"),
    [string] $GameVersion = "26.3",
    [int] $GameWaitSeconds = 20,
    [string] $Archive = "",
    [switch] $Fresh,
    [switch] $Report
)

$ErrorActionPreference = "Stop"

# The names of the profiles come as one string: a list that is written
# on the command line of `powershell -File` arrives as one argument, and
# a script that expects a list would see a name with commas in it. The
# shell that calls the script may also have split the list already, and
# then the names are separated by spaces.
$names = @($Profiles -split "[,\s]+" | ForEach-Object { $_.Trim() } | Where-Object { $_ })
Write-Host "the profiles are: $($names -join ' | ')"

# The numbers of the report are read by another program, so they are
# written the same way everywhere.
[System.Threading.Thread]::CurrentThread.CurrentCulture = [System.Globalization.CultureInfo]::InvariantCulture

$measure = Join-Path $PSScriptRoot "measure.ps1"

# The archive of the classes. The archive is bound to the classpath and
# to the flags of the machine, so it belongs to the version of the game
# and to this machine.
$archivePath = if ($Archive) {
    $Archive
} else {
    Join-Path $env:LOCALAPPDATA "MinLauncher\cds\$GameVersion.jsa"
}

# The flags of every profile. The name of the profile is the name of the
# scenario in the file of the measurements.
$flagsOf = @{
    "default"      = @()
    "measured"     = @()
    "measured_g1"  = @("-XX:+UseG1GC")
    "measured_cds" = @("-Xshare:on", "-XX:SharedArchiveFile=$archivePath")
}

# The profile of the flags of the launcher is not the same thing as the
# flags that the measurement puts into the settings: the launcher has to
# know which of its profiles is in use, so that the settings screen can
# show what the measurement said.
$profileOf = @{
    "default"      = "default"
    "measured"     = "measured"
    "measured_g1"  = "measured"
    "measured_cds" = "measured"
}

function Read-Json {
    param([string] $Path)

    $text = [System.IO.File]::ReadAllText($Path)
    if (-not $text.Trim()) { return $null }
    return ($text | ConvertFrom-Json)
}

function Write-Settings {
    <#
        Puts the flags of the profile into the settings of the launcher.

        The settings are read as they are and are written back whole: a
        field of the settings that the measurement does not know about
        must not be lost. The result is read back and checked, because a
        settings file that the launcher cannot read would quietly take
        the defaults - and the measurement would then be of nothing.
    #>
    param(
        [string] $Path,
        [string[]] $Flags,
        [string] $Profile
    )

    $settings = Read-Json -Path $Path
    if ($null -eq $settings) { throw "the settings at $Path cannot be read" }

    $jvm = $settings.jvm
    $jvm | Add-Member -MemberType NoteProperty -Name "flags_profile" -Value $Profile -Force
    $jvm | Add-Member -MemberType NoteProperty -Name "extra_args" -Value @($Flags) -Force

    $text = $settings | ConvertTo-Json -Depth 10
    [System.IO.File]::WriteAllText($Path, $text, (New-Object System.Text.UTF8Encoding($false)))

    $check = Read-Json -Path $Path
    if ($null -eq $check -or $null -eq $check.jvm.extra_args) {
        throw "the settings were written, but the launcher would not read the flags out of them"
    }
    if ($check.jvm.flags_profile -ne $Profile) {
        throw "the profile of the flags was not written"
    }
}

function Write-ArchiveDump {
    <#
        Runs the game once with the flag that asks the virtual machine to
        leave an archive of the classes behind, and waits for the archive
        to be written. Returns its size in bytes, or zero when there is
        no archive.

        The flags of this run are the flags of the runs that will use the
        archive, and in the same order: the archive is bound to the
        classpath and to the flags of the machine, and a machine that is
        handed an archive that does not match refuses to start.

        The game is closed by its window, so the machine leaves normally
        and the archive is written on the way out. A machine that is
        killed writes nothing, and the archive of a machine that was
        killed is a file of half a heap.
    #>
    param(
        [string] $ArchivePath,
        [string] $DumpPath
    )

    $directory = Split-Path -Parent $ArchivePath
    if ($directory -and -not (Test-Path -LiteralPath $directory)) {
        New-Item -ItemType Directory -Path $directory -Force | Out-Null
    }
    if (Test-Path -LiteralPath $ArchivePath) {
        Remove-Item -LiteralPath $ArchivePath -Force
    }

    # The same flags as the profile that uses the archive, except for
    # the flag that says which archive it is. `-Xshare:on` is in both:
    # a machine that is asked for an archive that does not match has to
    # say so and stop, rather than quietly do without the archive.
    Write-Settings -Path $Settings -Flags @("-Xshare:on", "-XX:ArchiveClassesAtExit=$ArchivePath") `
        -Profile "measured"

    & $measure -Runs 1 -ColdRuns 0 -GameWaitSeconds $GameWaitSeconds -AutoPlay `
        -Scenario "$GameVersion-cds-dump" -GameVersion $GameVersion -Out $DumpPath

    # The archive is written while the machine leaves, so the file is
    # watched until it stops growing: a file that is still growing is a
    # file that is not finished.
    $clock = [System.Diagnostics.Stopwatch]::StartNew()
    $length = 0
    $quiet = 0
    while ($clock.Elapsed.TotalSeconds -lt 180) {
        if (Test-Path -LiteralPath $ArchivePath) {
            $current = (Get-Item -LiteralPath $ArchivePath).Length
            if ($current -gt 0 -and $current -eq $length) {
                $quiet++
                if ($quiet -ge 2) { return $current }
            } else {
                $quiet = 0
            }
            $length = $current
        }
        Start-Sleep -Milliseconds 500
    }
    return 0
}

function Get-InGameFile {
    <#
        The file of the launcher that holds the samples of the game.

        There is one file for the game with the mod and one for the game
        without it, and the game that was measured last is the one that
        was written last.
    #>
    param([string] $BenchmarksDir)

    $file = Get-ChildItem -LiteralPath $BenchmarksDir -Filter "in_game_*.json" -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTimeUtc -Descending |
        Select-Object -First 1
    if ($null -eq $file) { return $null }
    return $file.FullName
}

function Get-SampleCount {
    <#
        How many samples the file of the game holds already. The runs of
        the other profiles are in it too, and only the samples that were
        added since this number belong to the run that has just finished.
    #>
    param([string] $Path)

    if (-not $Path -or -not (Test-Path -LiteralPath $Path)) { return 0 }

    $document = Read-Json -Path $Path
    if ($null -eq $document -or -not $document.samples) { return 0 }

    $total = 0
    foreach ($property in $document.samples.PSObject.Properties) {
        $count = @($property.Value).Count
        if ($count -gt $total) { $total = $count }
    }
    return $total
}

function Get-NewSamples {
    <#
        The samples of the game that the run that has just finished added.
    #>
    param([string] $Path, [int] $Taken)

    if (-not $Path -or -not (Test-Path -LiteralPath $Path)) { return @{} }

    $document = Read-Json -Path $Path
    if ($null -eq $document -or -not $document.samples) { return @{} }

    $result = @{}
    foreach ($property in $document.samples.PSObject.Properties) {
        $values = @($property.Value)
        if ($values.Count -le $Taken) { continue }
        $result[$property.Name] = @($values[$Taken..($values.Count - 1)])
    }
    return $result
}

function Write-SamplesFile {
    <#
        Writes the file of the measurements, in the shape of the script of
        the protocol: the metadata, the scenario and the samples.
    #>
    param(
        [string] $Path,
        $Meta,
        [string] $Scenario,
        [hashtable] $Samples
    )

    $lines = @()
    foreach ($metric in ($Samples.Keys | Sort-Object)) {
        $values = @()
        foreach ($value in $Samples[$metric]) {
            $values += ([double] $value).ToString("R", [System.Globalization.CultureInfo]::InvariantCulture)
        }
        $lines += ('    "{0}": [{1}]' -f $metric, ($values -join ", "))
    }

    $text = "{`n" +
        "  `"meta`": " + ($Meta | ConvertTo-Json -Depth 4) + ",`n" +
        "  `"scenario`": " + ($Scenario | ConvertTo-Json) + ",`n" +
        "  `"samples`": {`n" + ($lines -join ",`n") + "`n  }`n}"

    $directory = Split-Path -Parent $Path
    if ($directory -and -not (Test-Path -LiteralPath $directory)) {
        New-Item -ItemType Directory -Path $directory -Force | Out-Null
    }
    [System.IO.File]::WriteAllText($Path, $text, (New-Object System.Text.UTF8Encoding($false)))
}

if (-not (Test-Path -LiteralPath $measure)) {
    Write-Error "there is no measurement script at $measure"
    exit 2
}

$benchmarksDir = Join-Path $env:APPDATA "MinLauncher\benchmarks"
$backup = [System.IO.File]::ReadAllText($Settings)

# The samples of the game, profile by profile.
$inGame = @{}
foreach ($name in $names) { $inGame[$name] = @{} }

# The file of every profile, and the scenario it is measured under. The
# profile that leaves the game alone is the headline of the launcher, and
# its file keeps the name of the protocol; the other profiles are the
# same measurement with other flags, and they are told apart by the name
# of the scenario.
$fileOf = @{}
$scenarioOf = @{}
foreach ($name in $names) {
    # The file of the protocol - `ours.json` - belongs to the measurement
    # of the protocol and to nothing else. The profiles are measured
    # without a cold start, and a file that mixes the two would be read
    # later as a measurement of the protocol.
    $fileOf[$name] = "ours-$name.json"
    $scenarioOf[$name] = "$GameVersion-$name"
}

# The samples of a file are the samples of one binary. When the binary
# has changed, the old samples are of another launcher and must not be
# mixed with the new ones - and the file of the protocol keeps the
# samples of the runs, so that a measurement can be continued.
if ($Fresh -and -not $Report) {
    foreach ($name in $names) {
        $path = Join-Path $Out $fileOf[$name]
        if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Force }
    }
}

if (-not $Report) {
    # The rounds that did not happen, profile by profile. A round whose
    # game did not come is a round without numbers, and a number that is
    # missing is not a number that is zero: the count of them is written
    # into the comparison, so that nobody reads the middle of the rounds
    # that happened and thinks it is the middle of all of them.
    $failedRounds = @{}
    foreach ($name in $names) { $failedRounds[$name] = 0 }

    # The archive of the classes has to exist before the profiles that
    # use it are measured, and it is made by the game itself: the
    # virtual machine writes it when it leaves.
    if ($names -contains "measured_cds") {
        Write-Host ""
        Write-Host "the archive of the classes is made by one run of the game"
        $dumpPath = Join-Path $Out "ours-cds-dump.json"
        try {
            $length = Write-ArchiveDump -ArchivePath $archivePath -DumpPath $dumpPath
        } catch {
            $length = 0
            Write-Warning "  the archive was not made: $($_.Exception.Message)"
        }
        if ($length -gt 0) {
            Write-Host ("  the archive is {0} bytes: {1}" -f $length, $archivePath)
        } else {
            Write-Warning "  there is no archive, the profile measured_cds is left out"
            $names = @($names | Where-Object { $_ -ne "measured_cds" })
            $failedRounds.Remove("measured_cds")
        }
    }

    try {
        for ($round = 1; $round -le $Runs; $round++) {
            foreach ($name in $names) {
                if (-not $flagsOf.ContainsKey($name)) {
                    throw "there is no profile named $name"
                }

                Write-Settings -Path $Settings -Flags $flagsOf[$name] -Profile $profileOf[$name]

                $gameFile = Get-InGameFile -BenchmarksDir $benchmarksDir
                $taken = Get-SampleCount -Path $gameFile

                $path = Join-Path $Out $fileOf[$name]
                Write-Host ""
                Write-Host "round $round of $Runs, the profile $name"
                try {
                    & $measure -Runs 1 -ColdRuns 0 -GameWaitSeconds $GameWaitSeconds -AutoPlay `
                        -Scenario $scenarioOf[$name] -GameVersion $GameVersion -Out $path
                } catch {
                    # The round is counted and the measurement goes on: a
                    # measurement of thirty rounds is not lost because
                    # the third one did not happen.
                    $failedRounds[$name] = $failedRounds[$name] + 1
                    Write-Warning "  the round did not happen: $($_.Exception.Message)"
                    continue
                }

                # The samples of the game of this run, and only them.
                $samples = Get-NewSamples -Path $gameFile -Taken $taken
                foreach ($metric in $samples.Keys) {
                    $values = @()
                    if ($inGame[$name].ContainsKey($metric)) {
                        $values = @($inGame[$name][$metric])
                    }
                    $inGame[$name][$metric] = @($values + $samples[$metric])
                }
            }
        }
    } finally {
        [System.IO.File]::WriteAllText($Settings, $backup, (New-Object System.Text.UTF8Encoding($false)))
    }

    # The rounds that did not happen are counted by the script of the
    # protocol: it writes into its own file how many of its runs had no
    # game, and the file of the profile is the file of the protocol. The
    # count here is what the script of the protocol threw away - a round
    # that did not happen at all - and the two are added up.
    foreach ($name in $names) {
        $path = Join-Path $Out $fileOf[$name]
        if (-not (Test-Path -LiteralPath $path)) { continue }
        try {
            $document = Read-Json -Path $path
        } catch {
            continue
        }
        if ($null -eq $document -or -not $document.meta) { continue }
        if ($null -eq $document.meta.failed_runs) { continue }
        $failedRounds[$name] = $failedRounds[$name] + [int] $document.meta.failed_runs
    }
} else {
    Write-Host "only the report is written, the game is not started"
    $failedRounds = @{}
    foreach ($name in $names) { $failedRounds[$name] = 0 }
}

# The file of a profile must hold the numbers of that profile and of no
# other: the script of the protocol takes the whole file of the game,
# and in the runs one after another that file holds the runs of every
# profile.
foreach ($name in $names) {
    $path = Join-Path $Out $fileOf[$name]
    if (-not (Test-Path -LiteralPath $path)) { continue }
    if ($inGame[$name].Count -eq 0) { continue }

    $document = Read-Json -Path $path
    $samples = @{}
    foreach ($property in $document.samples.PSObject.Properties) {
        $samples[$property.Name] = @($property.Value)
    }
    foreach ($metric in $inGame[$name].Keys) {
        $samples[$metric] = @($inGame[$name][$metric])
    }
    Write-SamplesFile -Path $path -Meta $document.meta -Scenario $document.scenario -Samples $samples
    Write-Host "the samples of the game of the profile $name are in $path"
}

Write-Host "the measurements are in $Out"

# The comparison of the profiles, in one file: the samples of every
# profile under the name of the profile. It is the evidence of the
# decision about the flags, so it is written by the script that made the
# measurement and not by hand.
$combined = [ordered]@{
    meta     = [ordered]@{
        os          = "windows"
        game_version = $GameVersion
        profiles    = @($names)
        runs        = $Runs
        failed_rounds = ($failedRounds.Values | Measure-Object -Sum).Sum
        failed_rounds_by_profile = $failedRounds
        recorded_at = (Get-Date).ToUniversalTime().ToString("o", [System.Globalization.CultureInfo]::InvariantCulture)
    }
    scenario = "flags-ab"
    samples  = [ordered]@{}
}

foreach ($name in $names) {
    $path = Join-Path $Out $fileOf[$name]
    if (-not (Test-Path -LiteralPath $path)) { continue }
    $document = Read-Json -Path $path
    if ($null -eq $document -or -not $document.samples) { continue }

    foreach ($property in $document.samples.PSObject.Properties) {
        $values = @()
        foreach ($value in @($property.Value)) {
            $values += ([double] $value).ToString("R", [System.Globalization.CultureInfo]::InvariantCulture)
        }
        $combined.samples["$name.$($property.Name)"] = $values
    }
}

if ($combined.samples.Keys.Count -gt 0) {
    $lines = @()
    foreach ($key in $combined.samples.Keys) {
        $lines += ('    "{0}": [{1}]' -f $key, ($combined.samples[$key] -join ", "))
    }
    $text = "{`n" +
        "  `"meta`": " + ($combined.meta | ConvertTo-Json -Depth 4) + ",`n" +
        "  `"scenario`": " + ($combined.scenario | ConvertTo-Json) + ",`n" +
        "  `"samples`": {`n" + ($lines -join ",`n") + "`n  }`n}"
    $combinedPath = Join-Path $Out "flags-ab.json"
    [System.IO.File]::WriteAllText($combinedPath, $text, (New-Object System.Text.UTF8Encoding($false)))
    Write-Host "the comparison of the profiles is in $combinedPath"
}
