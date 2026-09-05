# 列出某个进程的全部顶层窗口（类名、可见性、矩形）。
# probe.ps1 只按标题 "HoverNote" 匹配，窗口还没设好标题时会什么都看不到；
# 排查启动问题要用这个按 PID 枚举的版本。
#   powershell -ExecutionPolicy Bypass -File tools/wins.ps1

param([string]$ProcName = 'hovernote')

$ErrorActionPreference = 'Stop'

Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public class HnWins {
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr p);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowTextW(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetClassNameW(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  public delegate bool EnumProc(IntPtr h, IntPtr p);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }

  public static string ByPid(uint want) {
    var sb = new StringBuilder();
    EnumWindows((h, p) => {
      uint pid; GetWindowThreadProcessId(h, out pid);
      if (pid != want) return true;
      var t = new StringBuilder(256); GetWindowTextW(h, t, 256);
      var c = new StringBuilder(256); GetClassNameW(h, c, 256);
      RECT r; GetWindowRect(h, out r);
      sb.AppendLine(string.Format("  vis={0,-6} {1},{2} {3}x{4}  \"{5}\"  {6}",
        IsWindowVisible(h), r.Left, r.Top, r.Right - r.Left, r.Bottom - r.Top, t, c));
      return true;
    }, IntPtr.Zero);
    return sb.ToString();
  }
}
'@

[void][HnWins]::SetProcessDPIAware()

$procs = @(Get-Process $ProcName -ErrorAction SilentlyContinue)
if (-not $procs) {
  Write-Output "没有正在运行的 $ProcName"
  exit 0
}

foreach ($p in $procs) {
  Write-Output ("pid {0}  MB={1}  threads={2}" -f $p.Id, [math]::Round($p.WorkingSet64 / 1MB, 1), $p.Threads.Count)
  $w = [HnWins]::ByPid([uint32]$p.Id)
  if ([string]::IsNullOrWhiteSpace($w)) { Write-Output '  (没有顶层窗口)' } else { Write-Output $w.TrimEnd() }
}
