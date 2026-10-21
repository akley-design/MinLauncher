<#
.SYNOPSIS
    The load of the machine.

.DESCRIPTION
    One function, one meaning: how much of the machine was busy. The
    scripts of the measurement use it, and the script that waits for a
    quiet machine uses it too, so that the number that says "the machine
    was busy" and the number that says "wait for a quiet machine" are
    the same number.

    The function is meant to be called like this:

        . "$PSScriptRoot\load.ps1"
        $load = Measure-Load
#>

function Measure-Load {
    <#
        The share of the machine that the processes used in the last
        second, in per cent.

        The time of the processor of every process is read twice with a
        second in between. The difference is the time the machine spent
        on the processes, and the time of the processor is the time it
        could have spent: a machine with twelve processors has twelve
        seconds of processor time in every second.

        The window is measured, not assumed. The two readings of the
        processes are not a second apart: the enumeration itself takes
        time, and on a busy machine it takes a good deal of it. A number
        that divided by a second while the window was longer would say
        that the machine was busier than it was - and it would say so
        most of all when the machine was busy, which is exactly when
        the number is read. The clock runs around the whole window, so
        the share is a share and never more than a hundred per cent.

        The processes that started and ended inside the second are not
        counted, and that is a small hole: a short program that took a
        whole processor for a moment would not be seen. A measurement
        of a launcher lasts minutes, and a hole of a moment does not
        make the number of the machine a lie.
    #>
    $cores = (Get-CimInstance Win32_ComputerSystem -ErrorAction SilentlyContinue).NumberOfLogicalProcessors
    if (-not $cores -or $cores -lt 1) { return 100.0 }

    $clock = [System.Diagnostics.Stopwatch]::StartNew()

    $before = @{}
    foreach ($process in Get-Process -ErrorAction SilentlyContinue) {
        $before[$process.Id] = $process.CPU
    }

    Start-Sleep -Seconds 1

    $used = 0.0
    foreach ($process in Get-Process -ErrorAction SilentlyContinue) {
        if ($before.ContainsKey($process.Id)) {
            $spent = $process.CPU - $before[$process.Id]
            if ($spent -gt 0) { $used += $spent }
        }
    }

    $clock.Stop()
    $window = $clock.Elapsed.TotalSeconds
    if ($window -le 0) { return 0.0 }

    return [math]::Round(($used / ($cores * $window)) * 100.0, 1)
}
