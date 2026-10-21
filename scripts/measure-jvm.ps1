<#
.SYNOPSIS
    Measures the start of the virtual machine with the flags of the game.

.DESCRIPTION
    The flags of the virtual machine are the business of the launcher, and a
    flag is only allowed in the product when a measurement on this machine
    showed that it wins. This script is that measurement: it starts the
    virtual machine of Mojang with the flags of the version of the game and
    with the variants of those flags, and writes down how long the start
    took.

    The measurement is of the virtual machine alone, and not of the game. The
    flags act in the start of the virtual machine - the heap is committed
    before the game begins - and the game itself loads its world, its
    textures and its shaders, and that noise is bigger than the effect of a
    flag.

    The variants are measured one after another, round by round. A machine
    that gets warmer or busier changes both of them the same way, and the
    difference between them is still the difference of the flag.

.PARAMETER Java
    The virtual machine to measure. The default is the one of Mojang that
    the launcher installed.

.PARAMETER Flags
    The flags of the version of the game. The default is what the version
    26.3 prescribes.

.PARAMETER Runs
    How many times every variant is started. The protocol asks for thirty.

.PARAMETER Out
    The file the samples are written into.

.EXAMPLE
    .\scripts\measure-jvm.ps1 -Runs 30 -Out benchmarks\jvm-flags.json
#>
param(
    [string] $Java = "",
    [string[]] $Flags = @(),
    [int] $Runs = 30,
    [string] $Out = "benchmarks\jvm-flags.json",
    [string] $Scenario = "jvm-flags-26.3"
)

$ErrorActionPreference = "Stop"

# The numbers of the report are read by another program, so they are
# written the same way everywhere.
[System.Threading.Thread]::CurrentThread.CurrentCulture = [System.Globalization.CultureInfo]::InvariantCulture

# The load of the machine is read by one function, and this script shares
# it with the script that waits for a quiet machine.
. (Join-Path $PSScriptRoot "load.ps1")

if (-not $Java) {
    $Java = Join-Path $env:LOCALAPPDATA "MinLauncher\runtimes\java-runtime-epsilon\bin\java.exe"
}

if (-not (Test-Path -LiteralPath $Java)) {
    Write-Error "there is no virtual machine at $Java"
    exit 2
}

if (-not $Flags -or $Flags.Count -eq 0) {
    # The flags of the version 26.3, as its document prescribes them.
    $Flags = @(
        "-Xms2G",
        "-Xmx4G",
        "-XX:+UseCompactObjectHeaders",
        "-XX:+AlwaysPreTouch",
        "-XX:+UseStringDeduplication",
        "-XX:+UseZGC"
    )
}

# The variants of the measurement. The first is what the version of the
# game prescribes, the others are the same with the flags that the
# measurement has to judge changed. The flags of the version are never
# removed: the last of a pair `+Flag` / `-Flag` is the one the virtual
# machine obeys, so a flag is switched off by adding `-Flag` after it.
$variants = [ordered]@{
    "as_is"           = @()
    "no_pretouch"     = @("-XX:-AlwaysPreTouch")
    "g1"              = @("-XX:+UseG1GC")
    "g1_no_pretouch"  = @("-XX:+UseG1GC", "-XX:-AlwaysPreTouch")
    # The other collectors the launcher offers, measured the same way the
    # profile would start the machine with them: the pretouch of the
    # version is off, so that the collectors are compared with each
    # other and not with a second of pretouch.
    #
    # Shenandoah is not among them: the Java of Mojang that the launcher
    # installs does not have it, and says so at the start
    # ("Unrecognized VM option 'UseShenandoah'"). A collector that the
    # machine refuses cannot be measured, and cannot be offered either.
    "serial_no_pretouch"     = @("-XX:+UseSerialGC", "-XX:-AlwaysPreTouch")
    "parallel_no_pretouch"   = @("-XX:+UseParallelGC", "-XX:-AlwaysPreTouch")
    # The collector that the Java of Mojang does not have. The variant is
    # here for the same reason as the others - every flag the launcher
    # knows about has a row of its own in the file of the measurement -
    # but the row is empty: the machine refuses to start with it, and a
    # refusal is not a number. The count of the refusals is in the
    # metadata of the file.
    "shenandoah" = @("-XX:+UseShenandoah")
    # The two flags of the version that the launcher does not touch. They
    # are measured so that every flag of the version has a number of its
    # own: the launcher keeps them, and the price of keeping them has to
    # be known.
    "no_dedup" = @("-XX:-UseStringDeduplication")
    "no_coh"   = @("-XX:-UseCompactObjectHeaders")
}

function Quote-Argument {
    <#
        Puts the argument in the form the command line of the system
        understands. The flags of the virtual machine have no spaces, but
        the rule is here so that a flag that has one is still right.
    #>
    param([string] $Value)

    if ($null -eq $Value) { return '""' }
    if ($Value -notmatch '[\s"]') { return $Value }
    return '"' + ($Value -replace '"', '\"') + '"'
}

# The collectors of the virtual machine. The virtual machine starts only
# one of them, and it refuses to start when two are named at once: the
# answer is "Multiple garbage collectors selected", and the machine is
# gone in a fraction of a second. A measurement of that is a measurement
# of a failure, and it looks like a very fast start.
$collectors = @(
    "-XX:+UseG1GC",
    "-XX:+UseZGC",
    "-XX:+UseShenandoah",
    "-XX:+UseParallelGC",
    "-XX:+UseSerialGC",
    "-XX:+UseEpsilonGC"
)

function Resolve-Flags {
    <#
        Puts the flags of the version and the flags of the variant
        together, the way the launcher puts them together.

        The virtual machine is told only one collector. When the variant
        names one, the collector of the version is taken away - exactly
        as the launcher takes it away, so that the measurement is of the
        thing the launcher would really start.

        The other flags are left alone: the last of a pair `+Flag` and
        `-Flag` is the one the virtual machine obeys, and that is how a
        flag is switched off without deleting the argument of the
        version.
    #>
    param(
        [string[]] $Base,
        [string[]] $Variant
    )

    $all = @($Base) + @($Variant)
    $named = @($Variant | Where-Object { $collectors -contains $_ })
    if ($named.Count -eq 0) { return $all }

    $all = @($all | Where-Object { $collectors -notcontains $_ })
    return @($all + $named)
}

function Measure-Start {
    <#
        Starts the virtual machine once and returns how long it took, in
        milliseconds, and what it said.

        The whole life of the process is measured: from the moment before
        the process is created to the moment it is gone. The creation of
        the process is a part of every measurement, and it is the same
        part for every variant - the difference between the variants is
        what is wanted, and the difference is not touched by a constant.

        The output of the virtual machine is read and kept: the machine
        that refuses to start says so, and a refusal that is measured as
        a very fast start is a lie.
    #>
    param(
        [string] $Java,
        [string[]] $Flags
    )

    $info = New-Object System.Diagnostics.ProcessStartInfo
    $info.FileName = $Java
    $info.Arguments = (($Flags | ForEach-Object { Quote-Argument $_ }) -join " ")
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true

    $clock = [System.Diagnostics.Stopwatch]::StartNew()
    $process = [System.Diagnostics.Process]::Start($info)
    # The streams are read before the wait: a process that fills the pipe
    # and waits for someone to read it would never be gone.
    $null = $process.StandardOutput.ReadToEnd()
    $error = $process.StandardError.ReadToEnd()
    $process.WaitForExit()
    $clock.Stop()
    $process.Dispose()

    return @($clock.Elapsed.TotalMilliseconds, $error)
}

function Get-Median {
    param([double[]] $Values)

    if (-not $Values -or $Values.Count -eq 0) { return 0 }
    $sorted = @($Values | Sort-Object)
    $middle = [int][Math]::Floor($sorted.Count / 2)
    if ($sorted.Count % 2 -eq 1) { return $sorted[$middle] }
    return ($sorted[$middle - 1] + $sorted[$middle]) / 2
}

Write-Host "measuring the start of the virtual machine"
Write-Host "  the virtual machine is $Java"
Write-Host "  the flags of the version are: $($Flags -join ' ')"
Write-Host "  $Runs runs of every variant"

# The load of the machine before the measurement starts. A number of a
# busy machine is a number of the other work, and the file of the
# measurement has to say that the machine was busy.
$loadBefore = Measure-Load
Write-Host ("  the load of the machine is {0} %" -f $loadBefore)

# The flags of every variant, the way the launcher would start the
# machine with them.
$resolved = [ordered]@{}
foreach ($name in $variants.Keys) {
    $resolved[$name] = Resolve-Flags -Base $Flags -Variant $variants[$name]
    Write-Host "  the variant $name is: $($resolved[$name] -join ' ')"
}

# A machine that refuses to start is not a fast start, and a run that
# was refused is not a number. The runs that were refused are counted,
# variant by variant, and the count is written into the file: a reader
# must be able to see that the numbers are the numbers of the runs that
# happened - and which variant never started at all.
$refused = @{}
foreach ($name in $variants.Keys) { $refused[$name] = 0 }

foreach ($name in $variants.Keys) {
    Write-Host "  the variant $name is warmed up"
    $null = Measure-Start -Java $Java -Flags $resolved[$name]
}

$samples = [ordered]@{}
foreach ($name in $variants.Keys) { $samples[$name] = @() }

# The load of the machine while the measurement is taken. It is read once
# per round, and the highest of the readings is written into the file: a
# number of a machine that was busy at some point of the measurement is a
# number of a machine that was busy, and the reader of the file has to be
# able to see that.
$loadDuring = $loadBefore
$loads = @()

for ($run = 1; $run -le $Runs; $run++) {
    foreach ($name in $variants.Keys) {
        $answer = Measure-Start -Java $Java -Flags $resolved[$name]
        $millis = $answer[0]
        $said = [string] $answer[1]
        if ($said -match "Error occurred during initialization" -or
            $said -match "Multiple garbage collectors" -or
            $said -match "Unrecognized VM option") {
            $refused[$name] = $refused[$name] + 1
            Write-Warning ("  {0} run {1} of {2}: the virtual machine refused to start: {3}" -f `
                $name, $run, $Runs, ($said -split "`n" | Select-Object -First 2 | Out-String).Trim())
            continue
        }
        $samples[$name] += $millis
        Write-Host ("  {0} run {1} of {2}: {3:N1} ms" -f $name, $run, $Runs, $millis)
    }

    $load = Measure-Load
    $loads += $load
    $loadDuring = [math]::Max($loadDuring, $load)
    Write-Host ("  the load of the machine is {0} %" -f $loadDuring)
}

$refusedTotal = 0
foreach ($name in $variants.Keys) { $refusedTotal += $refused[$name] }
if ($refusedTotal -gt 0) {
    Write-Warning "$refusedTotal runs were refused by the virtual machine and are not in the numbers"
}

Write-Host ""
Write-Host "the result:"
foreach ($name in $variants.Keys) {
    $values = $samples[$name]
    if ($values.Count -eq 0) {
        if ($refused[$name] -gt 0) {
            Write-Host ("  ${name}: the virtual machine refused to start in all {0} runs" -f $refused[$name])
        } else {
            Write-Host "  ${name}: no run at all"
        }
        continue
    }
    Write-Host ("  {0}: the middle is {1:N1} ms, the fastest {2:N1} ms, the slowest {3:N1} ms" -f `
        $name, (Get-Median $values), ($values | Measure-Object -Minimum).Minimum, ($values | Measure-Object -Maximum).Maximum)
}

$meta = [ordered]@{
    os          = "windows"
    java        = $Java
    flags       = $Flags
    runs        = $Runs
    refused     = $refusedTotal
    recorded_at = (Get-Date).ToUniversalTime().ToString("o", [System.Globalization.CultureInfo]::InvariantCulture)
}

# The refusals, variant by variant: a reader has to be able to see which
# variant never started at all, and not only that something did not start.
$meta["refused_by_variant"] = $refused

# The load of the machine while the measurement was taken. A number of a
# busy machine is a number of the other work, and the reader of the file
# has to be able to see that. The load is read before the measurement,
# once per round of it, and after it, and the highest of the readings is
# written down: the machine was busy at some point of the measurement,
# and that is the number that says so.
$meta["load_percent"] = [math]::Max($loadDuring, (Measure-Load))

# The middle of the readings as well. The highest of them is the worst
# moment of the measurement, and one moment of somebody else's work would
# then say that the whole measurement was taken on a busy machine. The
# middle says how the machine was for most of the time, and both numbers
# together say it honestly.
$meta["load_median_percent"] = Get-Median $loads

try {
    $meta["cpu"] = (Get-CimInstance Win32_Processor -ErrorAction Stop |
        Select-Object -First 1 -ExpandProperty Name)
} catch {
    Write-Warning "the processor is not known : $($_.Exception.Message)"
}

# The samples are written as they are read: a list in brackets, so that
# the file is a JSON file and a program other than this script can read
# the numbers of it. The numbers are written without a thousands
# separator: a comma in the middle of a number is a comma in the middle
# of a list, and the list of the samples would be read as a list of many
# more numbers than it is.
$sampleJson = "{`n" + (($samples.Keys | ForEach-Object {
    "    `"$_`": [" + (($samples[$_] | ForEach-Object {
        ([double] $_).ToString("R", [System.Globalization.CultureInfo]::InvariantCulture)
    }) -join ", ") + "]"
}) -join ",`n") + "`n  }"

$text = "{`n" +
    "  `"meta`": " + ($meta | ConvertTo-Json -Depth 4) + ",`n" +
    "  `"scenario`": " + ($Scenario | ConvertTo-Json) + ",`n" +
    "  `"samples`": $sampleJson`n" +
    "}`n"

$directory = Split-Path -Parent $Out
if ($directory -and -not (Test-Path -LiteralPath $directory)) {
    New-Item -ItemType Directory -Path $directory -Force | Out-Null
}
[System.IO.File]::WriteAllText($Out, $text, (New-Object System.Text.UTF8Encoding($false)))

Write-Host "the measurements are in $Out"
