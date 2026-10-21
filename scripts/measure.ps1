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

.PARAMETER AutoPlay
    Click the button "Play" by itself, with a real mouse event.

    The measurement needs a click on "Play", and the click has to be a
    real one: the hook of the mouse reads the input of the mouse, and a
    message sent to the window is not that. The script therefore moves
    the cursor to the point of the button and presses it with
    `SendInput` - the same event a hand on the mouse puts into the queue
    of the system, and the same event the hook reads.

    Without this switch the click belongs to the operator: the script
    starts the launcher, waits, and the operator presses the button.

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
    [switch] $NoGame,
    [switch] $AutoPlay
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

/// The window of the process.
///
/// The protocol says that the window has to become *visible*: the
/// moment the handle exists is not that moment. A window of Tauri is
/// created long before the page of the interface is drawn, and the
/// handle of a window that was never shown is worth nothing.
///
/// `IsWindowVisible` is the rule the protocol names, and it is the
/// rule an outside party can reproduce on the other side of the
/// comparison: the window of the process is visible.
public static class WindowWatch
{
    [DllImport("user32.dll")]
    private static extern bool IsWindowVisible(IntPtr hwnd);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern int GetClassName(IntPtr hwnd, System.Text.StringBuilder buffer, int count);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern int GetWindowText(IntPtr hwnd, System.Text.StringBuilder buffer, int count);

    [DllImport("user32.dll")]
    private static extern bool GetWindowRect(IntPtr hwnd, out RECT rect);

    [StructLayout(LayoutKind.Sequential)]
    private struct RECT
    {
        public int left;
        public int top;
        public int right;
        public int bottom;
    }

    /// Whether the window of the process is on the screen.
    public static bool IsVisible(IntPtr hwnd)
    {
        if (hwnd == IntPtr.Zero) { return false; }
        return IsWindowVisible(hwnd);
    }

    /// Whether the window is the window of the game and not the
    /// console of a virtual machine started without `javaw`.
    ///
    /// The game of Minecraft is a window with a picture in it. The
    /// console of a `java.exe` is a window too, and it is on the screen
    /// from the first moment - measuring it would measure the wrong
    /// window.
    public static bool IsGameWindow(IntPtr hwnd)
    {
        if (!IsVisible(hwnd)) { return false; }
        var buffer = new System.Text.StringBuilder(256);
        GetClassName(hwnd, buffer, buffer.Capacity);
        return buffer.ToString() != "ConsoleWindowClass";
    }

    /// The name of the window and the class of it, for the log.
    ///
    /// The measurement has to show which window it took, so that a
    /// number can be checked against the game that was really running.
    public static string Describe(IntPtr hwnd)
    {
        var title = new System.Text.StringBuilder(256);
        GetWindowText(hwnd, title, title.Capacity);
        var klass = new System.Text.StringBuilder(256);
        GetClassName(hwnd, klass, klass.Capacity);
        RECT rect;
        var size = "?";
        if (GetWindowRect(hwnd, out rect))
        {
            size = (rect.right - rect.left) + "x" + (rect.bottom - rect.top);
        }
        return "'" + title.ToString() + "' [" + klass.ToString() + "] " + size;
    }
}

/// The click of the operator.
///
/// The measurement needs a click on "Play", and the click has to be a
/// real one: the hook above sees the input of the mouse, and a
/// message sent to the window is not input of the mouse. The click is
/// therefore made by `SendInput`, which puts a true mouse event into
/// the queue of the system - the same event a hand on the mouse would
/// put there, and the same event the hook reads.
///
/// The click is not made in the window of the launcher directly: the
/// cursor is moved to the point of the button first, so that the
/// launcher sees the click where a player would make it.
public static class MouseClick
{
    [StructLayout(LayoutKind.Sequential)]
    private struct INPUT
    {
        public uint type;
        public INPUTUNION u;
    }

    [StructLayout(LayoutKind.Explicit)]
    private struct INPUTUNION
    {
        [FieldOffset(0)] public MOUSEINPUT mi;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct MOUSEINPUT
    {
        public int dx;
        public int dy;
        public uint mouseData;
        public uint dwFlags;
        public uint time;
        public IntPtr dwExtraInfo;
    }

    [DllImport("user32.dll", SetLastError = true)]
    private static extern uint SendInput(uint nInputs, INPUT[] pInputs, int cbSize);

    [DllImport("user32.dll")]
    private static extern bool SetCursorPos(int x, int y);

    [DllImport("user32.dll")]
    private static extern bool SetForegroundWindow(IntPtr hwnd);

    [DllImport("user32.dll")]
    private static extern bool ShowWindow(IntPtr hwnd, int nCmdShow);

    [DllImport("user32.dll")]
    private static extern int GetSystemMetrics(int nIndex);

    [DllImport("user32.dll")]
    private static extern bool BringWindowToTop(IntPtr hwnd);

    [DllImport("user32.dll")]
    private static extern IntPtr GetForegroundWindow();

    [DllImport("user32.dll")]
    private static extern IntPtr SetFocus(IntPtr hwnd);

    [DllImport("user32.dll")]
    private static extern bool AttachThreadInput(uint attach, uint attachTo, bool doAttach);

    [DllImport("user32.dll")]
    private static extern uint GetWindowThreadProcessId(IntPtr hwnd, IntPtr processId);

    [DllImport("kernel32.dll")]
    private static extern uint GetCurrentThreadId();

    private const uint INPUT_MOUSE = 0;
    private const uint MOUSEEVENTF_MOVE = 0x0001;
    private const uint MOUSEEVENTF_ABSOLUTE = 0x8000;
    private const uint MOUSEEVENTF_VIRTUALDESK = 0x4000;
    private const uint MOUSEEVENTF_LEFTDOWN = 0x0002;
    private const uint MOUSEEVENTF_LEFTUP = 0x0004;

    private const int SM_XVIRTUALSCREEN = 76;
    private const int SM_YVIRTUALSCREEN = 77;
    private const int SM_CXVIRTUALSCREEN = 78;
    private const int SM_CYVIRTUALSCREEN = 79;

    /// Brings the window in front of the others.
    ///
    /// `SetForegroundWindow` alone is not enough: Windows lets a
    /// process take the front only when it is the foreground process
    /// already, and the console of the measurement is. A click that
    /// lands on a window that is behind another one only brings that
    /// window to the front and is lost - and the button is never
    /// pressed.
    ///
    /// The way around it is the way a person does it: the threads of
    /// the script and of the window are tied together for a moment,
    /// and the window is brought to the front from inside that tie.
    /// The tie is taken away right away.
    ///
    /// Returns whether the window is in front afterwards.
    public static bool Focus(IntPtr hwnd)
    {
        if (hwnd == IntPtr.Zero) { return false; }
        ShowWindow(hwnd, 9);       // SW_RESTORE

        uint self = GetCurrentThreadId();
        uint foreground = GetWindowThreadProcessId(GetForegroundWindow(), IntPtr.Zero);
        uint target = GetWindowThreadProcessId(hwnd, IntPtr.Zero);

        bool tiedToForeground = foreground != 0 && foreground != self;
        bool tiedToTarget = target != 0 && target != self;
        if (tiedToForeground) { AttachThreadInput(self, foreground, true); }
        if (tiedToTarget) { AttachThreadInput(self, target, true); }

        BringWindowToTop(hwnd);
        SetForegroundWindow(hwnd);
        SetFocus(hwnd);

        if (tiedToTarget) { AttachThreadInput(self, target, false); }
        if (tiedToForeground) { AttachThreadInput(self, foreground, false); }

        return GetForegroundWindow() == hwnd;
    }

    /// Clicks the point of the screen. Returns false when the system
    /// refused the input.
    public static bool Click(int x, int y)
    {
        if (!SetCursorPos(x, y)) { return false; }

        // The cursor has to be there before the button goes down, or
        // the button is pressed at the place the cursor came from.
        System.Threading.Thread.Sleep(60);

        int left = GetSystemMetrics(SM_XVIRTUALSCREEN);
        int top = GetSystemMetrics(SM_YVIRTUALSCREEN);
        int width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        int height = GetSystemMetrics(SM_CYVIRTUALSCREEN);
        if (width <= 0 || height <= 0) { return false; }

        int ax = (int)(((x - left) * 65535.0) / width);
        int ay = (int)(((y - top) * 65535.0) / height);

        var inputs = new INPUT[3];
        inputs[0].type = INPUT_MOUSE;
        inputs[0].u.mi.dx = ax;
        inputs[0].u.mi.dy = ay;
        inputs[0].u.mi.dwFlags = MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK;
        inputs[1].type = INPUT_MOUSE;
        inputs[1].u.mi.dwFlags = MOUSEEVENTF_LEFTDOWN;
        inputs[2].type = INPUT_MOUSE;
        inputs[2].u.mi.dwFlags = MOUSEEVENTF_LEFTUP;

        uint sent = SendInput((uint)inputs.Length, inputs, Marshal.SizeOf(typeof(INPUT)));
        return sent == inputs.Length;
    }
}
'@

Add-Type -TypeDefinition $HookSource -Language CSharp | Out-Null

# The tree of the accessibility of Windows is what the interface of the
# launcher is read through.
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

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

function Stop-LeftOverGames {
    <#
        Stops the game of the launcher that is still running.

        Only the game of the launcher is stopped, and not the java of
        anything else on the machine: the game is found by the path of
        its runtime, and that path lies in the directory of the
        launcher.

        A game that lives on into the next run would be measured
        there. Its window is a window of the game, and it would be
        taken for the game of the click of that run - and the number
        would be the length of nothing.
    #>
    foreach ($process in (Get-Process -Name java, javaw -ErrorAction SilentlyContinue)) {
        try {
            $path = $process.Path
        } catch {
            continue
        }
        if ($null -eq $path) { continue }
        if ($path -like "*MinLauncher*runtimes*") {
            Write-Host "  stopping the game that is still running"
            Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
        }
    }
}

function Stop-StaleProcesses {
    <#
        Stops what a previous run left running.

        The launcher is single-instance: the second copy of it quits at
        once and hands the window to the first. A launcher that is
        already running would therefore make every measurement of the
        script a measurement of a process that exits in a second - and
        the run would be counted as a launcher that never showed a
        window.

        The game of the launcher is stopped too: it holds two
        gigabytes, and a game that is running while the next one starts
        would be measured as the memory of the game.
    #>
    Get-Process -Name "minlaucher" -ErrorAction SilentlyContinue |
        Stop-Process -Force -ErrorAction SilentlyContinue

    Stop-LeftOverGames
}

function Set-AutomaticRestart {
    <#
        Switches the auto-restart of the launcher off for the time of
        the measurement, and keeps the settings so that they can be
        given back exactly as they were.

        The game of this machine falls over when it is closed: it asks
        the server of the sessions for an answer, the server is not
        reachable, and the game goes down with an error. The launcher
        sees a game that fell over and starts it again - five seconds
        later, with a window of its own. The run after that one would
        find that window and take it for the game of its own click,
        and the number would be the length of nothing.

        The rules of the restart are read once, when the game is
        started, so the switch has to be off before the launcher
        starts.

        Only the one word is changed and the document is not written
        again: everything else in it stays as it was. The file is
        written without a byte order mark, because the reader of the
        settings is Rust and a mark at the beginning of the file is
        not JSON.
    #>
    $path = Join-Path $env:APPDATA "MinLauncher\settings.json"
    if (-not (Test-Path -LiteralPath $path)) {
        Write-Warning "there are no settings; the automatic restart cannot be switched off"
        return [pscustomobject]@{ Existed = $false; Text = "" }
    }

    $text = [System.IO.File]::ReadAllText($path)
    $regex = [regex]::new('("restart"\s*:\s*\{[^}]*?"enabled"\s*:\s*)(true|false)')
    if (-not $regex.Match($text).Success) {
        throw "the rules of the restart are not in the settings; they cannot be switched off"
    }

    $changed = $regex.Replace($text, '${1}false', 1)
    [System.IO.File]::WriteAllText($path, $changed, (New-Object System.Text.UTF8Encoding($false)))

    $check = $regex.Match([System.IO.File]::ReadAllText($path))
    if (-not $check.Success -or $check.Groups[2].Value -ne "false") {
        throw "the automatic restart could not be switched off"
    }

    Write-Host "  the automatic restart of the launcher is switched off for the measurement"
    return [pscustomobject]@{ Existed = $true; Text = $text }
}

function Restore-AutomaticRestart {
    <#
        Gives the settings of the launcher back as they were before
        the measurement.
    #>
    param($Backup)

    if ($null -eq $Backup) { return }

    $path = Join-Path $env:APPDATA "MinLauncher\settings.json"
    if (-not $Backup.Existed) {
        if (Test-Path -LiteralPath $path) {
            Remove-Item -LiteralPath $path -Force
        }
        return
    }

    [System.IO.File]::WriteAllText($path, $Backup.Text, (New-Object System.Text.UTF8Encoding($false)))
}

# The name of the play button, written without a single Cyrillic
# letter in the source.
#
# PowerShell 5.1 reads a script without a byte order mark through the
# code page of ANSI, and a name written in the source arrives broken.
# A broken name matches nothing, and the search would go on until the
# timeout without an error. The name is therefore built from the code
# points of the letters.
$script:PlayName = [string]::Concat(
    [char]0x0418,   # U+0418
    [char]0x0433,   # U+0433
    [char]0x0440,   # U+0440
    [char]0x0430,   # U+0430
    [char]0x0442,   # U+0442
    [char]0x044C)   # U+044C

function Get-LauncherPage {
    <#
        The document of the page of the launcher.

        The interface is a web view, and it is read through the tree of
        the accessibility of Windows - the same tree a screen reader
        reads. The tree of a web view is built only when a client asks
        for it, and the call that asks for it is `FindAll` over the
        whole document.
    #>
    $root = [System.Windows.Automation.AutomationElement]::RootElement
    $condition = New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::NameProperty, "MinLauncher")
    $window = $root.FindFirst([System.Windows.Automation.TreeScope]::Children, $condition)
    if ($null -eq $window) { return $null }

    $document = $window.FindFirst(
        [System.Windows.Automation.TreeScope]::Descendants,
        (New-Object System.Windows.Automation.PropertyCondition(
            [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
            [System.Windows.Automation.ControlType]::Document)))
    if ($null -eq $document) { return $null }

    return [pscustomobject]@{
        Window   = $window
        Document = $document
    }
}

function Get-Elements {
    param($Root)
    if ($null -eq $Root) { return @() }
    return $Root.FindAll(
        [System.Windows.Automation.TreeScope]::Descendants,
        [System.Windows.Automation.Condition]::TrueCondition)
}

function Find-Element {
    <#
        The element with exactly this name and this kind.
    #>
    param($Root, [string] $Name, [string] $Type = "")
    foreach ($element in (Get-Elements $Root)) {
        $current = $element.Current
        if ($current.Name -ne $Name) { continue }
        if ($Type -ne "" -and $current.ControlType.ProgrammaticName -notlike "*$Type*") { continue }
        return $element
    }
    return $null
}

function Find-ChooseButton {
    <#
        The button that chooses the account of the list.

        The name of the button is not the text on it: the item of an
        account joins the letter of the avatar, the name and the kind
        into one name, and the button repeats the name of the account.
        The button is therefore the one whose name is a part of the name
        of the item.
    #>
    param($Root)
    foreach ($element in (Get-Elements $Root)) {
        $current = $element.Current
        if ($current.ControlType.ProgrammaticName -notlike "*ListItem*") { continue }
        foreach ($child in (Get-Elements $element)) {
            if ($child.Current.ControlType.ProgrammaticName -notlike "*Button*") { continue }
            if (-not $child.Current.Name) { continue }
            if ($current.Name.Contains($child.Current.Name)) { return $child }
        }
    }
    return $null
}

function Invoke-Element {
    param($Element)
    $pattern = $null
    if ($Element.TryGetCurrentPattern(
            [System.Windows.Automation.InvokePattern]::Pattern,
            [ref]$pattern)) {
        $pattern.Invoke()
        return $true
    }
    return $false
}

function Click-Play {
    <#
        Makes the click of the operator on the button "Play".

        The click has to be a real one - the hook of the mouse reads
        the input of the mouse, and a message sent to the window is not
        that. The cursor is therefore moved to the point of the button
        and the button is pressed with `SendInput`, which puts a true
        mouse event into the queue of the system.

        The navigation to the screen of the game is not measured, so it
        is done through the tree of the accessibility: that is a click
        of the instrument, not a click of the player, and it happens
        before the button that is measured.
    #>
    param([int] $TimeoutSeconds)

    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    $clicked = $false

    while ((Get-Date) -lt $deadline) {
        $page = Get-LauncherPage
        if ($null -eq $page) {
            Start-Sleep -Milliseconds 500
            continue
        }

        $button = Find-Element $page.Document $script:PlayName "Button"
        if ($null -eq $button) {
            # The launcher may have opened on another screen. The link
            # "Play" of the menu brings it back to the screen of the game.
            $link = Find-Element $page.Document $script:PlayName "Hyperlink"
            if ($null -ne $link) {
                $null = Invoke-Element $link
                Start-Sleep -Seconds 2
            }
            continue
        }

        if (-not $button.Current.IsEnabled) {
            # The button is greyed out until an account is chosen.
            $choose = Find-ChooseButton $page.Document
            if ($null -ne $choose) {
                $null = Invoke-Element $choose
                Start-Sleep -Seconds 2
            }
            continue
        }

        # The window has to be in front: the click goes to the window
        # that is under the cursor, and the console of the measurement
        # may be there. A click on a window that is behind another one
        # only brings that window to the front and is lost.
        if (-not [MouseClick]::Focus($page.Window.Current.NativeWindowHandle)) {
            Write-Warning "  the window of the launcher would not come to the front"
            Start-Sleep -Milliseconds 500
            continue
        }

        $rect = $button.Current.BoundingRectangle
        if ($rect.IsEmpty) {
            Start-Sleep -Milliseconds 500
            continue
        }

        $x = [int]($rect.Left + $rect.Width / 2)
        $y = [int]($rect.Top + $rect.Height / 2)
        Write-Host ("  clicking Play at {0},{1}" -f $x, $y)
        if (-not [MouseClick]::Click($x, $y)) {
            throw "the click could not be made"
        }
        $clicked = $true
        break
    }

    if (-not $clicked) {
        throw "the button Play was not found within $TimeoutSeconds seconds"
    }
}

function Wait-ForWindow {
    <#
        Waits until the window of the process is visible, and returns the
        moment it became visible.

        The window is looked for through the handle of the main window:
        a window without a title is not a window a player can use. The
        handle alone is not enough - the protocol says the window has to
        become *visible*, and the handle of a window that was created
        but never shown exists long before that. The window is checked
        with `IsWindowVisible`, the same rule the other side of the
        comparison can apply.
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
        $handle = $Process.MainWindowHandle
        if ($handle -ne [IntPtr]::Zero -and [WindowWatch]::IsVisible($handle)) {
            return (Get-TimestampMs)
        }
        Start-Sleep -Milliseconds 20
    }

    throw "the window of the $What did not appear within $TimeoutSeconds seconds"
}

function Wait-ForGame {
    <#
        Waits for the game and returns it together with the moment its
        window became visible.

        The game is found by its **window**, and not by the fact that a
        new process of the virtual machine appeared. The launcher asks
        the Java of the system for its version with `java -version`, and
        that process lives long enough to be caught by the wrong guess.

        The title of the window is not used either: the title of the
        game is the name of the version, and it is not known
        beforehand.

        The moment is taken when the window is seen. The loop asks for
        the windows of the processes that appeared after the click, and
        a console window is not the window of the game.
    #>
    param(
        [int[]] $KnownIds,
        [int] $TimeoutSeconds
    )

    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    $trace = $env:MINLAUNCHER_MEASURE_DEBUG -eq "1"
    $seen = @{}
    while ((Get-Date) -lt $deadline) {
        foreach ($process in (Get-JavaProcesses)) {
            if ($KnownIds -contains $process.Id) { continue }
            try {
                $process.Refresh()
                if ($process.HasExited) { continue }
                $handle = $process.MainWindowHandle
            } catch {
                # The process is gone between the list and the check.
                continue
            }
            if ($trace -and $handle -ne [IntPtr]::Zero) {
                $key = "$($process.Id):$($handle.ToInt64())"
                if (-not $seen.ContainsKey($key)) {
                    $seen[$key] = $true
                    Write-Host ("    [trace] pid {0} window {1} {2}" -f `
                        $process.Id, $handle.ToInt64(), [WindowWatch]::Describe($handle))
                }
            }
            if ([WindowWatch]::IsGameWindow($handle)) {
                # The window that was taken is written down, so that the
                # number can be checked against the game that was really
                # running.
                Write-Host ("  the game was found: {0} (pid {1}), the window is {2}" -f `
                    $process.ProcessName, $process.Id, [WindowWatch]::Describe($handle))
                return [pscustomobject]@{
                    Process    = $process
                    AppearedAt = (Get-TimestampMs)
                }
            }
        }
        Start-Sleep -Milliseconds 10
    }

    throw "the window of the game did not appear within $TimeoutSeconds seconds"
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

function Wait-ForPage {
    <#
        Waits until the interface of the launcher is drawn, and returns
        the moment it was.

        This is an observation and not a metric of the protocol. The
        window of the launcher becomes visible long before the page of
        the interface is on it, and the two moments are told apart in
        the report: the metric M1 and M2 is the moment the window
        became visible - the rule the other side of the comparison can
        apply to a launcher of any kind - and this one says when a
        player could have pressed the button.

        The page is read through the tree of the accessibility of
        Windows. The tree of a web view is built only when somebody
        asks for it, and the call that asks for it is `FindAll` over
        the whole document.
    #>
    param([int] $TimeoutSeconds)

    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        $page = Get-LauncherPage
        if ($null -ne $page) {
            # The count of an empty tree is 0 and not nothing: the list
            # is made a list, so that the count is a number.
            $elements = @(Get-Elements $page.Document)
            if ($elements.Count -gt 0) {
                return (Get-TimestampMs)
            }
        }
        Start-Sleep -Milliseconds 20
    }

    return $null
}

function Start-Launcher {
    <#
        Starts the launcher and waits for its window. Returns the
        process, the length of the start in milliseconds and the moment
        the interface of the launcher was drawn.
    #>
    param(
        [string] $Path,
        [int] $TimeoutSeconds
    )

    $started = Get-TimestampMs
    $process = Start-Process -FilePath $Path -WorkingDirectory (Split-Path -Parent $Path) -PassThru
    $appeared = Wait-ForWindow -Process $process -TimeoutSeconds $TimeoutSeconds -What "launcher"
    $page = Wait-ForPage -TimeoutSeconds $TimeoutSeconds

    $pageMilliseconds = $null
    if ($null -ne $page) {
        $pageMilliseconds = ($page - $started)
    } else {
        Write-Warning "  the interface of the launcher was not drawn; the observation is not made"
    }

    return [pscustomobject]@{
        Process         = $process
        StartedAt       = $started
        Milliseconds    = ($appeared - $started)
        PageMilliseconds = $pageMilliseconds
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

        The cache is *moved* and not deleted: the warm phase needs it,
        and the warm phase is what measures M3 and M8. A deleted cache
        would have to be downloaded again - 670 MB - and the launcher
        would spend the whole run downloading instead of starting the
        game. The moved copy is put back by `Restore-LauncherCache`.
    #>
    $cache = Join-Path $env:LOCALAPPDATA "MinLauncher"
    if (-not (Test-Path -LiteralPath $cache)) { return $null }

    if ((Split-Path -Leaf $cache) -ne "MinLauncher") {
        throw "refusing to clear $cache : it is not the cache of the launcher"
    }

    $backup = "$cache.cold"
    if (Test-Path -LiteralPath $backup) {
        Remove-Item -LiteralPath $backup -Recurse -Force
    }

    Write-Host "  moving the cache at $cache out of the way"
    Move-Item -LiteralPath $cache -Destination $backup
    return $backup
}

function Restore-LauncherCache {
    <#
        Puts the cache back where it was before the cold phase.

        The launcher that ran with the empty cache has filled a new one
        - it has fetched the manifest of the versions, at least. That
        new one is thrown away: it is the cache of a cold run, and the
        warm phase has to see the same cache the machine had before.
    #>
    param([string] $Backup)

    if ([string]::IsNullOrEmpty($Backup)) { return }
    if (-not (Test-Path -LiteralPath $Backup)) {
        throw "the cache was not moved : $Backup is not there"
    }

    $cache = Join-Path $env:LOCALAPPDATA "MinLauncher"
    if (Test-Path -LiteralPath $cache) {
        Remove-Item -LiteralPath $cache -Recurse -Force
    }

    Write-Host "  putting the cache back"
    Move-Item -LiteralPath $Backup -Destination $cache
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
        * the run of the game with the mod - the game the launcher
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

function Save-Samples {
    <#
        Writes the samples into the file, and does not let a failure of
        the writing hide the failure that caused it.

        The file is written after every run: a measurement of thirty
        runs is an hour of work, and it must not be lost because the
        twenty-ninth run failed.
    #>
    param(
        [hashtable] $Meta,
        [hashtable] $Samples,
        [string] $Scenario,
        [string] $Path
    )

    try {
        Write-Document -Meta $Meta -Samples $Samples -Scenario $Scenario -Path $Path
    } catch {
        Write-Warning "the samples could not be written into $Path : $($_.Exception.Message)"
    }
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

# The machine has to be in a known state: a launcher that is already
# running would take the window away from the one that is measured.
Stop-StaleProcesses

# The auto-restart of the launcher is switched off for the time of the
# measurement: a game that falls over would be started again by the
# launcher, and the second game of that run would be measured by the
# next one.
$restartBackup = Set-AutomaticRestart

[ClickWatch]::Install()
try {
    # --- the cold phase ------------------------------------------------
    #
    # The started instance is called `$instance` and not `$launcher`,
    # because the name of the parameter is `$Launcher` and the names of
    # the variables of PowerShell do not differ in their case. A
    # `[string]` parameter turns anything assigned to it into a string,
    # and a string has neither a window nor a time of start.
    for ($run = 1; $run -le $ColdRuns; $run++) {
        Write-Host "cold run $run of $ColdRuns"
        $coldCache = Clear-LauncherCache

        try {
            $instance = Start-Launcher -Path $Launcher -TimeoutSeconds $TimeoutSeconds
            Add-Sample -Samples $samples -Metric "launcher_cold_start" -Value $instance.Milliseconds
            Write-Host ("  the launcher was ready in {0:N1} ms" -f $instance.Milliseconds)
            if ($null -ne $instance.PageMilliseconds) {
                Add-Sample -Samples $samples -Metric "launcher_cold_page_ready" -Value $instance.PageMilliseconds
                Write-Host ("  the interface was drawn in {0:N1} ms" -f $instance.PageMilliseconds)
            }
            Stop-ProcessQuietly -Process $instance.Process
            Save-Samples -Meta $meta -Samples $samples -Scenario $Scenario -Path $Out
        } finally {
            # The cache comes back even when the run failed: without it
            # the warm phase below cannot start the game at all.
            Restore-LauncherCache -Backup $coldCache
        }
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

        $instance = Start-Launcher -Path $Launcher -TimeoutSeconds $TimeoutSeconds
        Add-Sample -Samples $samples -Metric "launcher_warm_start" -Value $instance.Milliseconds
        Write-Host ("  the launcher was ready in {0:N1} ms" -f $instance.Milliseconds)
        if ($null -ne $instance.PageMilliseconds) {
            Add-Sample -Samples $samples -Metric "launcher_warm_page_ready" -Value $instance.PageMilliseconds
            Write-Host ("  the interface was drawn in {0:N1} ms" -f $instance.PageMilliseconds)
        }

        $instance.Process.Refresh()
        Add-Sample -Samples $samples -Metric "launcher_rss" -Value ($instance.Process.WorkingSet64 / 1MB)

        if (-not $NoGame) {
            # The processes that are already running are listed now,
            # right before the click: the game of this run is the one
            # that appears after it. A game that was left over from the
            # run before would otherwise be taken for it.
            $known = @(Get-JavaProcesses | ForEach-Object { $_.Id })

            if ($AutoPlay) {
                Write-Host "  clicking Play in the launcher to start the game"
                Click-Play -TimeoutSeconds $TimeoutSeconds
            } else {
                Write-Host "  click Play in the launcher to start the game"
            }
            $game = Wait-ForGame -KnownIds $known -TimeoutSeconds $TimeoutSeconds

            # The rule of the protocol: the last click before the game
            # appeared. A click that is older than the start of the
            # launcher belongs to another run and is not taken.
            $click = [ClickWatch]::LastClickMs()
            if ($click -le $instance.StartedAt) {
                Write-Warning "  no click was seen; the run is not measured"
                Stop-ProcessQuietly -Process $game.Process
                Stop-ProcessQuietly -Process $instance.Process
                Stop-LeftOverGames
                continue
            }

            $appeared = $game.AppearedAt
            Add-Sample -Samples $samples -Metric "click_to_window" -Value ($appeared - $click)
            Write-Host ("  the game was ready in {0:N1} ms" -f ($appeared - $click))

            Write-Host "  waiting $GameWaitSeconds seconds for the game to settle"
            Start-Sleep -Seconds $GameWaitSeconds

            $gameProcess = $game.Process
            $gameProcess.Refresh()
            Add-Sample -Samples $samples -Metric "game_rss" -Value ($gameProcess.WorkingSet64 / 1MB)
            Write-Host ("  the game used {0:N1} MiB" -f ($gameProcess.WorkingSet64 / 1MB))

            # The game is closed, not killed: the agent writes its
            # report from a shutdown hook, and a killed process does not
            # run its hooks.
            Close-Game -Process $gameProcess

            # The launcher reads the report of the agent and writes the
            # measurements of the game as soon as the game is gone.
            Start-Sleep -Seconds 10
        }

        Stop-ProcessQuietly -Process $instance.Process

        # A game that is still running is stopped: the launcher is gone
        # by now, and a game of the run before must not be measured by
        # the run that comes next.
        Stop-LeftOverGames

        # The samples of the run that is done are on the disk: the
        # measurement of thirty runs is an hour of work, and it must
        # not be lost because of a later run.
        Save-Samples -Meta $meta -Samples $samples -Scenario $Scenario -Path $Out
    }
} finally {
    [ClickWatch]::Remove()

    # The numbers the launcher took itself.
    Merge-LauncherMeasurements -Samples $samples -BenchmarksDir $benchmarksDir -Scenario $Scenario

    Save-Samples -Meta $meta -Samples $samples -Scenario $Scenario -Path $Out

    Restore-AutomaticRestart -Backup $restartBackup
}

Write-Host "the measurements are in $Out"
foreach ($metric in ($samples.Keys | Sort-Object)) {
    Write-Host ("  {0}: {1} samples" -f $metric, $samples[$metric].Count)
}
