# 反复重启 HoverNote，记录每次启动是否真的建出了折角窗口和托盘。
# 用来抓「开机自启后进程活着但什么都不显示」这类偶发失败。
#   powershell -ExecutionPolicy Bypass -File tools/startup-check.ps1 -Runs 6

param(
  [int]$Runs = 6,
  [int]$WaitSeconds = 5,
  [string]$Exe = "$env:LOCALAPPDATA\Programs\HoverNote\HoverNote.exe",
  [string]$LogDir = 'C:\tmp'
)

$ErrorActionPreference = 'Stop'

Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public class HnProbe {
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr p);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetClassNameW(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  public delegate bool EnumProc(IntPtr h, IntPtr p);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }

  public static string Probe(uint want) {
    string orb = "none";
    int tauri = 0;
    bool tray = false;
    EnumWindows((h, p) => {
      uint pid; GetWindowThreadProcessId(h, out pid);
      if (pid != want) return true;
      var c = new StringBuilder(256); GetClassNameW(h, c, 256);
      string cls = c.ToString();
      if (cls == "tray_icon_app") tray = true;
      if (cls == "Tauri Window") {
        tauri++;
        RECT r; GetWindowRect(h, out r);
        int w = r.Right - r.Left, ht = r.Bottom - r.Top;
        if (IsWindowVisible(h) && w <= 40 && ht <= 40) orb = r.Left + "," + r.Top + " " + w + "x" + ht;
      }
      return true;
    }, IntPtr.Zero);
    return "tauriWins=" + tauri + "  orb=" + orb + "  tray=" + tray;
  }
}
'@

New-Item -ItemType Directory -Force -Path $LogDir | Out-Null
$fails = 0

# 必须等旧进程真的退干净再启新的。进程里有单实例互斥锁，只要旧的还没死透，
# 新起的那个会立刻自己退出——此时 Get-Process 拿到的仍是正在拆窗口的旧进程，
# 看起来就像"启动了但什么都没显示"，是假失败。
function Wait-Gone([int]$TimeoutMs = 15000) {
  Get-Process hovernote -ErrorAction SilentlyContinue |
    Stop-Process -Force -ErrorAction SilentlyContinue
  $sw = [Diagnostics.Stopwatch]::StartNew()
  while ($sw.ElapsedMilliseconds -lt $TimeoutMs) {
    if (-not (Get-Process hovernote -ErrorAction SilentlyContinue)) { return $true }
    Start-Sleep -Milliseconds 200
  }
  return $false
}

$WV_QUERY = 'SELECT ProcessId,ParentProcessId,CommandLine FROM Win32_Process WHERE Name=' +
            [char]39 + 'msedgewebview2.exe' + [char]39

function Get-OrphanWebView {
  @(Get-CimInstance -Query $WV_QUERY |
      Where-Object { $_.CommandLine -like '*com.shawn.hovernote*' })
}

# 强杀 HoverNote 会留下它自己的 WebView2 进程，那些进程还占着
# %LOCALAPPDATA%\com.shawn.hovernote\EBWebView 这个用户数据目录的锁；
# 下一次启动拿不到锁，窗口就一个也建不出来（setup 里表现为
# FailedToReceiveMessage）。这是强杀留下的残留，不是应用自身的启动问题——
# 要量应用自己的启动成功率，就得先把残留清掉。
function Clear-OrphanWebView([int]$Rounds = 4) {
  for ($r = 0; $r -lt $Rounds; $r++) {
    $orph = Get-OrphanWebView
    if (-not $orph) { return $true }
    # 先杀 browser 主进程（父进程不在这批里的那个），它会带走自己的子进程
    $ids = $orph | ForEach-Object { $_.ProcessId }
    $roots = $orph | Where-Object { $ids -notcontains $_.ParentProcessId }
    foreach ($t in @($roots) + @($orph)) {
      # taskkill 对已经消失的 pid 会往 stderr 写字并置退出码，$ErrorActionPreference
      # = 'Stop' 下会把整个脚本掀掉。这里的失败都是无关紧要的（要么已经死了，
      # 要么下一轮再杀），全部吞掉。
      try { & taskkill /PID $t.ProcessId /T /F 2>&1 | Out-Null } catch { }
    }
    Start-Sleep -Milliseconds 1200
  }
  return -not (Get-OrphanWebView)
}

for ($i = 1; $i -le $Runs; $i++) {
  if (-not (Wait-Gone)) {
    Write-Output "run $i : 旧进程没退干净，跳过"
    $fails++
    continue
  }
  $leftover = @(Get-OrphanWebView).Count
  $cleaned = Clear-OrphanWebView
  if (-not $cleaned) {
    Write-Output ("run {0} : 残留 WebView2 清不掉（{1} 个），本轮结果不可信" -f $i, @(Get-OrphanWebView).Count)
  }

  $log = Join-Path $LogDir "hn-run$i.log"
  Remove-Item $log -ErrorAction SilentlyContinue
  $env:HOVERNOTE_TRACE = $log

  Start-Process -FilePath $Exe -WorkingDirectory (Split-Path $Exe)
  Start-Sleep -Seconds $WaitSeconds

  $p = Get-Process hovernote -ErrorAction SilentlyContinue
  if (-not $p) {
    Write-Output "run $i : NO PROCESS"
    $fails++
    continue
  }

  $info = [HnProbe]::Probe([uint32]$p[0].Id)
  Write-Output ("run {0} : {1}  (启动前残留 webview2={2})" -f $i, $info, $leftover)
  if ($info -match 'orb=none') { $fails++ }

  if (Test-Path $log) {
    Get-Content $log | ForEach-Object { "        $_" }
  } else {
    Write-Output '        (no trace file)'
  }
}

Write-Output ''
Write-Output "失败 $fails / $Runs"
