<#
.SYNOPSIS
    Prints the comparison of the profiles of the flags of the virtual machine.

.DESCRIPTION
    `scripts\measure-profiles.ps1` measures the profiles one after another
    and writes the samples of all of them into one file,
    `benchmarks\flags-ab.json`. This script reads that file and prints, for
    every metric, the middle of every profile and the difference to the
    first profile.

    The middle is the median, and not the mean: the mean of a series that
    has one slow run in it is the story of that run. The rules of the
    comparison are the rules of `docs\PROTOCOL.md`: the noise of a series
    is `(p75 - p25) / median`, and a series that is noisier than five per
    cent cannot be compared with another one.

    Nothing is decided here: the script only counts. The decision is
    written down in `docs\RESULTS.md`, together with the numbers it is
    based on.

.PARAMETER File
    The file of the comparison. The default is the one the measurement
    writes.

.EXAMPLE
    .\scripts\compare-profiles.ps1
#>
param(
    [string] $File = ""
)

$ErrorActionPreference = "Stop"

[System.Threading.Thread]::CurrentThread.CurrentCulture = [System.Globalization.CultureInfo]::InvariantCulture

if (-not $File) {
    $File = Join-Path $PSScriptRoot "..\benchmarks\flags-ab.json"
}
$File = [System.IO.Path]::GetFullPath($File)

if (-not (Test-Path -LiteralPath $File)) {
    Write-Error "there is no comparison at $File"
    exit 2
}

$document = Get-Content -LiteralPath $File -Raw | ConvertFrom-Json
$meta = $document.meta
$samples = $document.samples

$profiles = @()
if ($meta.profiles) { $profiles = @($meta.profiles) }
if ($profiles.Count -eq 0) {
    foreach ($property in $samples.PSObject.Properties) {
        $name = ($property.Name -split "\.")[0]
        if ($profiles -notcontains $name) { $profiles += $name }
    }
}

# The samples of the profiles are named "<profile>.<metric>", and the
# samples of the start of the virtual machine are named after the variant
# alone. Both are read here: the metric of a variant of the virtual
# machine is the start of the virtual machine.
$metrics = @()
$series = @{}
foreach ($property in $samples.PSObject.Properties) {
    $name = $property.Name
    if ($name -like "*.*") {
        $profile = ($name -split "\.", 2)[0]
        $metric = ($name -split "\.", 2)[1]
    } else {
        $profile = $name
        $metric = "start of the virtual machine"
    }
    if ($metrics -notcontains $metric) { $metrics += $metric }
    $key = "$metric|$profile"
    if (-not $series.ContainsKey($key)) { $series[$key] = @() }
    $series[$key] = @($series[$key]) + @($property.Value)
}

function Get-Middle {
    param($Values)

    $numbers = @($Values | ForEach-Object { [double]$_ }) | Sort-Object
    if ($numbers.Count -eq 0) { return $null }
    $middle = [int][math]::Floor($numbers.Count / 2)
    if ($numbers.Count % 2 -eq 1) { return $numbers[$middle] }
    return ($numbers[$middle - 1] + $numbers[$middle]) / 2
}

function Get-Percentile {
    param($Values, [double] $Fraction)

    $numbers = @($Values | ForEach-Object { [double]$_ }) | Sort-Object
    if ($numbers.Count -eq 0) { return $null }
    $index = [int][math]::Floor($Fraction * ($numbers.Count - 1))
    return $numbers[$index]
}

Write-Host "the comparison: $File"
Write-Host ("profiles: {0}" -f ($profiles -join " | "))
Write-Host ("rounds per profile: {0}" -f $meta.runs)
Write-Host ("failed rounds: {0}" -f $meta.failed_rounds)
if ($meta.failed_rounds_by_profile) {
    foreach ($property in $meta.failed_rounds_by_profile.PSObject.Properties) {
        Write-Host ("  failed in {0}: {1}" -f $property.Name, $property.Value)
    }
}
Write-Host ""

$metrics = @($series.Keys | ForEach-Object { ($_ -split "\|", 2)[0] } | Select-Object -Unique)

foreach ($metric in $metrics) {
    Write-Host "--- $metric"
    $base = $null
    $baseFastest = $null
    $baseSlowest = $null
    $baseNoise = 0.0
    foreach ($profile in $profiles) {
        $values = @()
        if ($series.ContainsKey("$metric|$profile")) { $values = @($series["$metric|$profile"]) }
        if ($values.Count -eq 0) {
            Write-Host ("   {0,-14} no samples" -f $profile)
            continue
        }
        $middle = Get-Middle -Values $values
        $p25 = Get-Percentile -Values $values -Fraction 0.25
        $p75 = Get-Percentile -Values $values -Fraction 0.75
        $fastest = ($values | Measure-Object -Minimum).Minimum
        $slowest = ($values | Measure-Object -Maximum).Maximum
        $noise = 0.0
        if ($middle -gt 0) { $noise = ($p75 - $p25) / $middle * 100.0 }
        if ($null -eq $base) {
            $base = $middle
            $baseFastest = $fastest
            $baseSlowest = $slowest
            $baseNoise = $noise
            Write-Host ("   {0,-22} {1,10:N2}   n={2}  noise {3:N1} %   [{4:N2} .. {5:N2}]   (the base)" -f `
                $profile, $middle, $values.Count, $noise, $fastest, $slowest)
        } else {
            $delta = $middle - $base
            $mark = if ($delta -lt 0) { "faster" } else { "slower" }

            # The series do not meet when the runs of one of them are all
            # better than the runs of the other one, and the gap between
            # them is wider than the noise the protocol allows: five per
            # cent of the middle. A gap of a hundredth of a millisecond is
            # a gap of the machine and not of the flags, and calling it a
            # separation would be a lie with a decimal point in it.
            $gap = 0.0
            if ($slowest -lt $baseFastest) { $gap = $baseFastest - $slowest }
            elseif ($fastest -gt $baseSlowest) { $gap = $fastest - $baseSlowest }
            $limit = $middle * 0.05
            if ($gap -gt $limit) {
                $apart = "the series do not meet"
            } else {
                $apart = "the series meet"
            }

            # The rule of the protocol: a series that is noisier than five
            # per cent cannot be compared with another one, and the verdict
            # is not given for it at all.
            if ($noise -le 5.0 -and $baseNoise -le 5.0) {
                $comparable = "comparable"
            } else {
                $comparable = "not compared: the series is too noisy"
            }

            Write-Host ("   {0,-22} {1,10:N2}   n={2}  noise {3:N1} %   [{4:N2} .. {5:N2}]   {6:+0.00;-0.00} {7}   {8}   {9}" -f `
                $profile, $middle, $values.Count, $noise, $fastest, $slowest, $delta, $mark, $apart, $comparable)
        }
    }
    Write-Host ""
}
