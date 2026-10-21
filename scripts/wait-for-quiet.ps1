<#
.SYNOPSIS
    Waits until the machine is quiet enough to measure on.

.DESCRIPTION
    A measurement of a launcher is a measurement of the machine it was
    taken on. When the machine is busy with something else - an encoding,
    a build, a game - the numbers are the numbers of that other work
    and not of the launcher, and the series get wide and the comparison
    becomes a comparison of noise.

    This script watches the load of the machine and says nothing until
    the machine has been quiet for a while. It is the guard in front of
    the measurement: the measurement runs on a machine that was quiet,
    not on a machine that happened to be busy.

    The load is read from the processor: the share of the total time of
    the machine that the processes used, sampled over a second. A
    process that is doing nothing is not counted.

.PARAMETER Load
    The share of the machine that may be used, in per cent. The default
    is forty-five: the machine of a person at work is never empty - a
    browser, a chat, the system itself take twenty to thirty per cent of
    it all the time, and that is not "another work", it is the machine.
    What has to be kept out is the big work - an encoding, a build, a
    game - and that takes eighty per cent and more.

    The threshold does not decide whether the numbers are good: the
    variants of the measurement are taken one after another, round by
    round, so a background that does not change takes the same share out
    of every variant, and the middle of thirty rounds puts it out. Two
    runs of the same eight variants - one with the machine at thirty per
    cent, one at seventy - gave middles that differ by three per cent.
    The load that really was there is written into the file of the
    measurement, and the noise of the series decides whether they may be
    compared.

.PARAMETER Seconds
    How long the machine has to be quiet before the script says so. The
    default is twenty: a machine that was quiet for a moment is not a
    quiet machine.

.PARAMETER WaitMinutes
    How long to wait for that at most. The default is thirty minutes.
    When the time is up the script says so and returns 1: the caller
    decides what to do with a machine that never was quiet.

.EXAMPLE
    .\scripts\wait-for-quiet.ps1
    if ($LASTEXITCODE -eq 0) { .\scripts\measure-jvm.ps1 -Runs 30 }
#>
param(
    [double] $Load = 45.0,
    [int] $Seconds = 20,
    [int] $WaitMinutes = 30
)

$ErrorActionPreference = "Stop"

[System.Threading.Thread]::CurrentThread.CurrentCulture = [System.Globalization.CultureInfo]::InvariantCulture

# The load is read by one function, and this script shares it with the
# scripts of the measurement.
. (Join-Path $PSScriptRoot "load.ps1")

$deadline = (Get-Date).AddMinutes($WaitMinutes)
$quietSince = $null

Write-Host "waiting for a quiet machine: at most $WaitMinutes minutes, quiet means under $Load % of the processor for $Seconds seconds"

while ((Get-Date) -lt $deadline) {
    $used = Measure-Load
    if ($used -le $Load) {
        if ($null -eq $quietSince) {
            $quietSince = Get-Date
            Write-Host ("  the machine is quiet ({0} %), the clock of the quiet starts" -f $used)
        } else {
            $quiet = [int]((Get-Date) - $quietSince).TotalSeconds
            Write-Host ("  the machine is quiet ({0} %), {1} s of the {2} s" -f $used, $quiet, $Seconds)
            if ($quiet -ge $Seconds) {
                Write-Host "the machine is quiet. the measurement can start."
                exit 0
            }
        }
    } else {
        if ($null -ne $quietSince) {
            Write-Host ("  the machine was busy again ({0} %), the clock of the quiet starts over" -f $used)
        }
        $quietSince = $null
        Write-Host ("  the machine is busy ({0} %), waiting" -f $used)
    }
    Start-Sleep -Seconds 5
}

Write-Host "the machine was not quiet in $WaitMinutes minutes. a measurement now would be a measurement of the other work."
exit 1
