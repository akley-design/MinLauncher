<#
.SYNOPSIS
    Measures the launcher and writes the samples into a file of measurements.

.DESCRIPTION
    The script is the outside observer of `docs/PROTOCOL.md`: it starts
    the launcher, waits for its window, waits for the player to click
    "Play", waits for the window of the game, and writes down what it
    saw. It never touches the launcher it measures: it reads the
    processes, the windows and the clock, and nothing else.

    The click is captured passively with a low-level mouse hook that
    lives on its own thread. The hook only reads the timestamps of the
    clicks; it does not move the mouse, does not click and does not
    write anything into the launcher.

.PARAMETER Launcher
    The executable to measure. The default is the release build of the
    launcher.

.PARAMETER Runs
    How many times the launcher is started in the warm phase. The
    protocol asks for thirty.

.PARAMETER ColdRuns
    How many times the launcher is started with an empty cache. The
    cache is the regenerable data of the launcher (the libraries, the
    assets and the runtimes); the accounts, the settings and the
    installed versions are not touched.

.PARAMETER GameWaitSeconds
    How long to wait before the memory of the game is read. The
    protocol asks for sixty seconds: the game needs that long to load
    and to settle.

.PARAMETER TimeoutSeconds
    How long to wait for the window of the launcher, for the process of
    the game and for the window of the game.

.PARAMETER Out
    The file the samples are written into. The samples that are already
    in the file are kept, so the measurement can be continued.

.PARAMETER Scenario
    The scenario the measurements belong to. It is usually the version
    of the game.

.PARAMETER GameVersion
    The version of the game that was measured. It goes into the
    metadata of the file.

.PARAMETER NoGame
    Do not wait for the game: only the launcher is measured.

.EXAMPLE
    .\scripts\measure.ps1 -Runs 3 -Out benchmarks\ours.json

.EXAMPLE
    .\scripts\measure.ps1 -Runs 30 -ColdRuns 3 -Out benchmarks\ours.json
#>
[CmdletBinding()]
param(
    [string] $Launcher = (Join-Path $PSScriptRoot "..\target\release\minlaucher.exe"),
    [int]    $Runs = 30,
    [int]    $ColdRuns = 1,
    [int]    $GameWaitSeconds = 60,
    [int]    $TimeoutSeconds = 180,
    [string] $Out = (Join-Path $PSScriptRoot "..\benchmarks\ours.json"),
    [string] $Scenario = "default",
    [string] $GameVersion = "",
    [switch] $NoGame
)

$ErrorActionPreference = "Stop"

# The numbers of the report are read by another program, so they are
# written the same way everywhere.
[System.Threading.Thread]::CurrentThread.CurrentCulture = [System.Globalization.CultureInfo]::InvariantCulture

$Launcher = [System.IO.Path]::GetFullPath($Launcher)
$Out = [System.IO.Path]::GetFullPath($Out)

if (-not (Test-Path -LiteralPath $Launcher)) {
    Write-Error "there is no launcher at $Launcher"
    exit 2
}

# --------------------------------------------------------------------------
# The mouse hook
#
# A low-level mouse hook is called on the thread that installed it, in
# the message loop of that thread. A hook installed on the thread of
# the script would never fire, because the script does not pump
# messages: the hook therefore lives on a thread of its own with a
# message loop of its own.
# --------------------------------------------------------------------------

$HookSource = @'
using System;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Threading;

public static class ClickWatch
{
    private delegate IntPtr HookProc(int nCode, IntPtr wParam, IntPtr lParam);

    [DllImport("user32.dll", CharSet = CharSet.Auto, SetLastError = true)]
    private static extern IntPtr SetWindowsHookEx(int idHook, HookProc lpfn, IntPtr hMod, uint dwThreadId);

    [DllImport("user32.dll", CharSet = CharSet.Auto, SetLastError = true)]
    private static extern bool UnhookWindowsHookEx(IntPtr hhk);

    [DllImport("user32.dll", CharSet = CharSet.Auto, SetLastError = true)]
    private static extern IntPtr CallNextHookEx(IntPtr hhk, int nCode, IntPtr wParam, IntPtr lParam);

    [DllImport("user32.dll", CharSet = CharSet.Auto)]
    private static extern int GetMessage(out MSG lpMsg, IntPtr hWnd, uint wMsgFilterMin, uint wMsgFilterMax);

    [DllImport("user32.dll", CharSet = CharSet.Auto)]
    private static extern bool TranslateMessage(ref MSG lpMsg);

    [DllImport("user32.dll", CharSet = CharSet.Auto)]
    private static extern IntPtr DispatchMessage(ref MSG lpMsg);

    [DllImport("kernel32.dll", CharSet = CharSet.Auto, SetLastError = true)]
    private static extern IntPtr GetModuleHandle(string lpModuleName);

    [DllImport("winmm.dll")]
    private static extern uint timeBeginPeriod(uint uPeriod);

    [DllImport("winmm.dll")]
    private static extern uint timeEndPeriod(uint uPeriod);

    [StructLayout(LayoutKind.Sequential)]
    private struct MSG
    {
        public IntPtr hwnd;
        public uint message;
        public IntPtr wParam;
        public IntPtr lParam;
        public uint time;
        public int pt_x;
        public int pt_y;
    }

    private const int WH_MOUSE_LL = 14;
    private const uint WM_LBUTTONDOWN = 0x0201;

    private static HookProc _proc;
    private static IntPtr _hook = IntPtr.Zero;
    private static uint _period;
    private static long _lastClick;
    private static long _clicks;

    /// The timestamp of the last left click, in Stopwatch ticks.
    public static long LastClick { get { return Interlocked.Read(ref _lastClick); } }

    /// How many left clicks were seen.
    public static long Clicks { get { return Interlocked.Read(ref _clicks); } }

    /// The timestamp of the last left click, in milliseconds.
    public static double LastClickMs()
    {
        return Interlocked.Read(ref _lastClick) * 1000.0 / Stopwatch.Frequency;
    }

    public static void Install()
    {
        _period = timeBeginPeriod(1);
        _proc = HookCallback;
        var thread = new Thread(Run);
        thread.IsBackground = true;
        thread.Start();
    }

    private static void Run()
    {
        _hook = SetWindowsHookEx(WH_MOUSE_LL, _proc, GetModuleHandle(null), 0);
        MSG msg;
        while (GetMessage(out msg, IntPtr.Zero, 0, 0) > 0)
        {
            TranslateMessage(ref msg);
            DispatchMessage(ref msg);
        }
    }

    private static IntPtr HookCallback(int nCode, IntPtr wParam, IntPtr lParam)
    {
        if (nCode >= 0 && (uint)wParam == WM_LBUTTONDOWN)
        {
            Interlocked.Exchange(ref _lastClick, Stopwatch.GetTimestamp());
            Interlocked.Increment(ref _clicks);
        }
        return CallNextHookEx(_hook, nCode, wParam, lParam);
    }

    public static void Remove()
    {
        if (_hook != IntPtr.Zero)
        {
            UnhookWindowsHookEx(_hook);
            _hook = IntPtr.Zero;
        }
        if (_period != 0)
        {
            timeEndPeriod(_period);
            _period = 0;
        }
    }
}
'@

Add-Type -TypeDefinition $HookSource -Language CSharp | Out-Null

# --------------------------------------------------------------------------
# The helpers
# --------------------------------------------------------------------------

function Get-TimestampMs {
    <#
        The clock of the measurement: the performance counter of the
        machine, in milliseconds. It is not the clock of the wall and
        it does not jump when the clock of the system is corrected.
    #>
    return [double]([System.Diagnostics.Stopwatch]::GetTimestamp() * 1000.0 /
        [System.Diagnostics.Stopwatch]::Frequency)
}

function Get-JavaProcesses {
    <#
        The processes of the virtual machine. The game runs in one of
        them, and the launcher may start a few of its own.
    #>
    return @(Get-Process -Name java, javaw -ErrorAction SilentlyContinue)
}

function Wait-ForWindow {
    <#
        Waits until the process has a visible window, and returns the
        moment it appeared.

        The window is looked for through the title of the window: a
        window without a title is not a window a player can use.
    #>
    param(
        [System.Diagnostics.Process] $Process,
        [int] $TimeoutSeconds,
        [string] $What
    )

    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        if ($Process.HasExited) {
            throw "the $What exited before its window appeared"
        }
        $Process.Refresh()
        if ($Process.MainWindowHandle -ne [IntPtr]::Zero) {
            return (Get-TimestampMs)
        }
        Start-Sleep -Milliseconds 20
    }

    throw "the window of the $What did not appear within $TimeoutSeconds seconds"
}

function Wait-ForGameProcess {
    <#
        Waits for a virtual machine process that was not there before.
        The game is found by the process, not by the title of a window:
        the title of the game is the name of the version, and it is not
        known beforehand.
    #>
    param(
        [int[]] $KnownIds,
        [int] $TimeoutSeconds
    )

    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        foreach ($process in Get-JavaProcesses) {
            if ($KnownIds -notcontains $process.Id) {
                # The launcher asks the Java of the system for its version
                # with `java -version`. That process lives a moment and is
                # not the game; the game lives for minutes. A process that
                # is gone by the time we look again was not the game.
                Start-Sleep -Milliseconds 500
                if ($process.HasExited) { continue }
                return $process
            }
        }
        Start-Sleep -Milliseconds 50
    }

    return $null
}

function Close-Game {
    <#
        The game is closed by its window, not killed: the agent writes
        its report from a shutdown hook, and a killed process does not
        run its hooks. Without this the measurements of the game would
        be lost.
    #>
    param([System.Diagnostics.Process] $Process)

    if ($null -eq $Process) { return }
    try {
        if (-not $Process.HasExited) {
            [void] $Process.CloseMainWindow()
            if (-not $Process.WaitForExit(30000)) {
                Write-Warning "  the game did not close; it is killed"
                Stop-Process -Id $Process.Id -Force -ErrorAction SilentlyContinue
            }
        }
    } catch {
        # The game is already gone: that is what was wanted.
    }
}

function Start-Launcher {
    <#
        Starts the launcher and waits for its window. Returns the
        process and the length of the start in milliseconds.
    #>
    param(
        [string] $Path,
        [int] $TimeoutSeconds
    )

    $started = Get-TimestampMs
    $process = Start-Process -FilePath $Path -WorkingDirectory (Split-Path -Parent $Path) -PassThru
    $appeared = Wait-ForWindow -Process $process -TimeoutSeconds $TimeoutSeconds -What "launcher"

    return [pscustomobject]@{
        Process     = $process
        StartedAt   = $started
        Milliseconds = ($appeared - $started)
    }
}

function Stop-ProcessQuietly {
    param([System.Diagnostics.Process] $Process)
    if ($null -eq $Process) { return }
    try {
        if (-not $Process.HasExited) {
            Stop-Process -Id $Process.Id -Force -ErrorAction SilentlyContinue
        }
    } catch {
        # The process is already gone: that is what was wanted.
    }
}

function Clear-LauncherCache {
    <#
        Empties the regenerable data of the launcher. The accounts,
        the settings and the installed versions live in another
        directory and are not touched.
    #>
    $cache = Join-Path $env:LOCALAPPDATA "MinLauncher"
    if (-not (Test-Path -LiteralPath $cache)) { return }

    if ((Split-Path -Leaf $cache) -ne "MinLauncher") {
        throw "refusing to clear $cache : it is not the cache of the launcher"
    }

    Write-Host "  emptying the cache at $cache"
    Get-ChildItem -LiteralPath $cache -Force -ErrorAction SilentlyContinue |
        Remove-Item -Recurse -Force -ErrorAction SilentlyContinue
}

function Add-Sample {
    param(
        [hashtable] $Samples,
        [string] $Metric,
        [double] $Value
    )

    if (-not $Samples.ContainsKey($Metric)) {
        $Samples[$Metric] = New-Object System.Collections.ArrayList
    }
    [void] $Samples[$Metric].Add($Value)
}

function Read-Samples {
    <#
        The samples that are already in the file. The measurement can
        be continued: the samples of the earlier runs stay.
    #>
    param([string] $Path)

    $samples = @{}
    if (-not (Test-Path -LiteralPath $Path)) { return $samples }

    try {
        $document = Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
    } catch {
        Write-Warning "cannot read $Path : $($_.Exception.Message)"
        return $samples
    }

    if (-not $document.samples) { return $samples }
    foreach ($property in $document.samples.PSObject.Properties) {
        $samples[$property.Name] = New-Object System.Collections.ArrayList
        foreach ($value in @($property.Value)) {
            [void] $samples[$property.Name].Add([double] $value)
        }
    }

    return $samples
}

function Merge-LauncherMeasurements {
    <#
        The launcher measures what only it can see: the length of an
        installation and the behaviour of the game. It writes those
        numbers into its own directory, in the same format, and they
        are added to the file here.

        The choice between the files of the launcher is made here and
        not in the report, so that the file of the measurement says
        exactly what was measured:

        * the installation of the version that was measured;
        * the run of the game with the mod — the game the launcher
          promises. The run without the mod is only a fallback.

        When several presets were measured, the last run wins: it is
        the state the game is in now.
    #>
    param(
        [hashtable] $Samples,
        [string] $BenchmarksDir,
        [string] $Scenario
    )

    if (-not (Test-Path -LiteralPath $BenchmarksDir)) { return }

    $files = @()
    foreach ($file in Get-ChildItem -LiteralPath $BenchmarksDir -Filter "*.json" -ErrorAction SilentlyContinue) {
        try {
            $document = Get-Content -LiteralPath $file.FullName -Raw | ConvertFrom-Json
        } catch {
            Write-Warning "cannot read $($file.Name) : $($_.Exception.Message)"
            continue
        }
        if (-not $document.samples) { continue }

        $files += [pscustomobject]@{
            Written  = $file.LastWriteTimeUtc
            Scenario = $document.scenario
            Samples  = $document.samples
        }
    }

    $metrics = @()
    foreach ($file in $files) {
        foreach ($property in $file.Samples.PSObject.Properties) {
            $metrics += $property.Name
        }
    }

    foreach ($metric in ($metrics | Sort-Object -Unique)) {
        $candidates = @()
        foreach ($file in $files) {
            $property = $file.Samples.PSObject.Properties[$metric]
            if ($null -eq $property) { continue }
            $candidates += [pscustomobject]@{
                Scenario = $file.Scenario
                Written  = $file.Written
                Values   = @($property.Value)
            }
        }

        $best = $null
        if ($metric -like "in_game_*") {
            # The run with the mod wins over the run without it.
            $withMod = @($candidates | Where-Object {
                $_.Scenario -like "in_game/*" -and $_.Scenario -ne "in_game/off"
            })
            if ($withMod.Count -gt 0) {
                $best = $withMod | Sort-Object Written -Descending | Select-Object -First 1
            } else {
                $best = $candidates |
                    Where-Object { $_.Scenario -like "in_game/*" } |
                    Sort-Object Written -Descending |
                    Select-Object -First 1
            }
        } else {
            # The installation belongs to the version that was measured.
            $best = $candidates |
                Where-Object { $_.Scenario -eq $Scenario } |
                Select-Object -First 1
        }

        if ($null -eq $best) {
            Write-Warning "the metric $metric was measured in another scenario; it was not merged"
            continue
        }
        if ($Samples.ContainsKey($metric)) {
            Write-Warning "the metric $metric is measured already; it was not merged"
            continue
        }

        $list = New-Object System.Collections.ArrayList
        foreach ($value in $best.Values) { [void] $list.Add([double] $value) }
        $Samples[$metric] = $list
        Write-Host "  merged $metric from $($best.Scenario)"
    }
}

function ConvertTo-SamplesJson {
    <#
        The samples as JSON. The numbers are written by hand: the
        converter of PowerShell writes them the way the machine is
        configured, and a comma instead of a point would make the file
        unreadable.
    #>
    param([hashtable] $Samples)

    $lines = @()
    foreach ($metric in ($Samples.Keys | Sort-Object)) {
        $values = @()
        foreach ($value in $Samples[$metric]) {
            $values += ([double] $value).ToString("R", [System.Globalization.CultureInfo]::InvariantCulture)
        }
        $lines += ('    "{0}": [{1}]' -f $metric, ($values -join ", "))
    }

    return "{`n" + ($lines -join ",`n") + "`n  }"
}

function Write-Document {
    param(
        [hashtable] $Meta,
        [hashtable] $Samples,
        [string] $Scenario,
        [string] $Path
    )

    $directory = Split-Path -Parent $Path
    if (-not (Test-Path -LiteralPath $directory)) {
        New-Item -ItemType Directory -Path $directory -Force | Out-Null
    }

    $metaJson = $Meta | ConvertTo-Json -Depth 4
    $text = "{`n" +
        "  `"meta`": $metaJson,`n" +
        "  `"scenario`": " + ($Scenario | ConvertTo-Json) + ",`n" +
        "  `"samples`": " + (ConvertTo-SamplesJson -Samples $Samples) + "`n" +
        "}`n"

    [System.IO.File]::WriteAllText($Path, $text, (New-Object System.Text.UTF8Encoding($false)))
}

# --------------------------------------------------------------------------
# The measurement
# --------------------------------------------------------------------------

$samples = Read-Samples -Path $Out

$name = [System.IO.Path]::GetFileNameWithoutExtension($Launcher)
$version = ""
try {
    $info = [System.Diagnostics.FileVersionInfo]::GetVersionInfo($Launcher)
    if ($info.ProductVersion) { $version = $info.ProductVersion }
} catch {
    # The version of the file is a nicety, not a necessity.
}

$meta = [ordered]@{
    launcher    = $(if ($version) { "$name $version" } else { $name })
    os          = "windows"
    recorded_at = (Get-Date).ToUniversalTime().ToString("o", [System.Globalization.CultureInfo]::InvariantCulture)
}

try {
    $meta["cpu"] = (Get-CimInstance Win32_Processor -ErrorAction Stop |
        Select-Object -First 1 -ExpandProperty Name)
} catch {
    Write-Warning "the processor is not known : $($_.Exception.Message)"
}

if ($GameVersion) { $meta["game_version"] = $GameVersion }

$benchmarksDir = Join-Path $env:APPDATA "MinLauncher\benchmarks"

Write-Host "measuring $Launcher"
Write-Host "  the file of the measurements is $Out"

[ClickWatch]::Install()
try {
    # --- the cold phase ------------------------------------------------
    for ($run = 1; $run -le $ColdRuns; $run++) {
        Write-Host "cold run $run of $ColdRuns"
        Clear-LauncherCache

        $launcher = Start-Launcher -Path $Launcher -TimeoutSeconds $TimeoutSeconds
        Add-Sample -Samples $samples -Metric "launcher_cold_start" -Value $launcher.Milliseconds
        Write-Host ("  the launcher was ready in {0:N1} ms" -f $launcher.Milliseconds)
        Stop-ProcessQuietly -Process $launcher.Process
    }

    # --- the warm-up ---------------------------------------------------
    # The first run after the cold phase is thrown away: it is the run
    # that fills the cache, and a cold sample in a warm series is a
    # lie.
    Write-Host "the warm-up"
    $warmup = Start-Launcher -Path $Launcher -TimeoutSeconds $TimeoutSeconds
    Stop-ProcessQuietly -Process $warmup.Process
    Start-Sleep -Seconds 2

    # --- the warm phase ------------------------------------------------
    for ($run = 1; $run -le $Runs; $run++) {
        Write-Host "run $run of $Runs"

        $known = @(Get-JavaProcesses | ForEach-Object { $_.Id })

        $launcher = Start-Launcher -Path $Launcher -TimeoutSeconds $TimeoutSeconds
        Add-Sample -Samples $samples -Metric "launcher_warm_start" -Value $launcher.Milliseconds
        Write-Host ("  the launcher was ready in {0:N1} ms" -f $launcher.Milliseconds)

        $launcher.Process.Refresh()
        Add-Sample -Samples $samples -Metric "launcher_rss" -Value ($launcher.Process.WorkingSet64 / 1MB)

        if (-not $NoGame) {
            Write-Host "  click Play in the launcher to start the game"
            $game = Wait-ForGameProcess -KnownIds $known -TimeoutSeconds $TimeoutSeconds

            if ($null -eq $game) {
                Write-Warning "  no game appeared within $TimeoutSeconds seconds; the run is not measured"
                Stop-ProcessQuietly -Process $launcher.Process
                continue
            }

            # The rule of the protocol: the last click before the game
            # appeared. A click that is older than the start of the
            # launcher belongs to another run and is not taken.
            $click = [ClickWatch]::LastClickMs
            if ($click -le $launcher.StartedAt) {
                Write-Warning "  no click was seen; the run is not measured"
                Stop-ProcessQuietly -Process $game
                Stop-ProcessQuietly -Process $launcher.Process
                continue
            }

            $appeared = Wait-ForWindow -Process $game -TimeoutSeconds $TimeoutSeconds -What "game"
            Add-Sample -Samples $samples -Metric "click_to_window" -Value ($appeared - $click)
            Write-Host ("  the game was ready in {0:N1} ms" -f ($appeared - $click))

            Write-Host "  waiting $GameWaitSeconds seconds for the game to settle"
            Start-Sleep -Seconds $GameWaitSeconds

            $game.Refresh()
            Add-Sample -Samples $samples -Metric "game_rss" -Value ($game.WorkingSet64 / 1MB)
            Write-Host ("  the game used {0:N1} MiB" -f ($game.WorkingSet64 / 1MB))

            # The game is closed, not killed: the agent writes its
            # report from a shutdown hook, and a killed process does not
            # run its hooks.
            Close-Game -Process $game

            # The launcher reads the report of the agent and writes the
            # measurements of the game as soon as the game is gone.
            Start-Sleep -Seconds 10
        }

        Stop-ProcessQuietly -Process $launcher.Process
    }
} finally {
    [ClickWatch]::Remove()
}

# The numbers the launcher took itself.
Merge-LauncherMeasurements -Samples $samples -BenchmarksDir $benchmarksDir -Scenario $Scenario

Write-Document -Meta $meta -Samples $samples -Scenario $Scenario -Path $Out

Write-Host "the measurements are in $Out"
foreach ($metric in ($samples.Keys | Sort-Object)) {
    Write-Host ("  {0}: {1} samples" -f $metric, $samples[$metric].Count)
}
