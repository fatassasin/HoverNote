# 验证工作区变化时折角会跟着走。
#
# 这是「开机自启时不贴角」最可能的成因：登录那一刻任务栏还没建出来，rcWork 就是
# 整块屏幕，折角被摆到屏幕最底下；任务栏一出现，工作区缩小，它就悬在半空。
# 这里用 SPI_SETWORKAREA 直接把工作区改小来复现，测完立刻还原。
#
#   powershell -ExecutionPolicy Bypass -File tools/workarea-test.ps1

$ErrorActionPreference = 'Stop'

Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public class WA {
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr p);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern IntPtr MonitorFromPoint(POINT pt, uint flags);
  [DllImport("user32.dll")] public static extern bool GetMonitorInfoW(IntPtr mon, ref MONITORINFO mi);
  [DllImport("user32.dll")] public static extern bool SystemParametersInfoW(uint act, uint p, ref RECT r, uint win);
  public delegate bool EnumProc(IntPtr h, IntPtr p);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
  [StructLayout(LayoutKind.Sequential)] public struct MONITORINFO { public int cbSize; public RECT Monitor, Work; public uint Flags; }

  public static List<IntPtr> ByPid(uint want) {
    var list = new List<IntPtr>();
    EnumWindows((h, p) => { uint pid; GetWindowThreadProcessId(h, out pid); if (pid == want) list.Add(h); return true; }, IntPtr.Zero);
    return list;
  }
  public static RECT Rect(IntPtr h) { RECT r; GetWindowRect(h, out r); return r; }
  public static RECT Work(int x, int y) {
    var pt = new POINT(); pt.X = x; pt.Y = y;
    var mi = new MONITORINFO(); mi.cbSize = Marshal.SizeOf(typeof(MONITORINFO));
    GetMonitorInfoW(MonitorFromPoint(pt, 2), ref mi);
    return mi.Work;
  }
  // SPI_SETWORKAREA = 0x002F，不广播设置变更（0），改完自己还原
  public static bool SetWork(RECT r) { return SystemParametersInfoW(0x002F, 0, ref r, 0); }
}
'@

[void][WA]::SetProcessDPIAware()

$proc = Get-Process -Name 'hovernote' -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $proc) { throw 'HoverNote 没在跑' }

$orb = $null
foreach ($h in [WA]::ByPid([uint32]$proc.Id)) {
  $r = [WA]::Rect($h)
  $w = $r.Right - $r.Left
  if ($w -gt 0 -and $w -eq ($r.Bottom - $r.Top) -and $w -le 120) { $orb = $h; break }
}
if (-not $orb) { throw '没找到折角窗口' }

function Show([string]$tag) {
  $r = [WA]::Rect($orb)
  $wa = [WA]::Work((($r.Left + $r.Right) / 2), (($r.Top + $r.Bottom) / 2))
  $flush = ($r.Right -eq $wa.Right) -and ($r.Bottom -eq $wa.Bottom)
  "{0,-10} 折角右下=({1},{2})  工作区右下=({3},{4})  贴合={5}" -f $tag, $r.Right, $r.Bottom, $wa.Right, $wa.Bottom, $flush
}

$r0 = [WA]::Rect($orb)
$orig = [WA]::Work((($r0.Left + $r0.Right) / 2), (($r0.Top + $r0.Bottom) / 2))
Show '初始'

try {
  $shrunk = New-Object WA+RECT
  $shrunk.Left = $orig.Left; $shrunk.Top = $orig.Top
  $shrunk.Right = $orig.Right; $shrunk.Bottom = $orig.Bottom - 200
  [void][WA]::SetWork($shrunk)
  Write-Host "把工作区底边上移 200px -> $($shrunk.Bottom)"
  Start-Sleep -Milliseconds 900
  Show '缩小后'
}
finally {
  [void][WA]::SetWork($orig)
  Write-Host "工作区已还原 -> $($orig.Bottom)"
  Start-Sleep -Milliseconds 900
  Show '还原后'
}
