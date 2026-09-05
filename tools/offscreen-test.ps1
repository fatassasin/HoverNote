# 验证折角被甩到屏幕外还能自己回到角上。
#
#   powershell -ExecutionPolicy Bypass -File tools/offscreen-test.ps1
#
# 分辨率变小时窗口就是这个下场——旧坐标整个落在新桌面之外。snap-test.ps1 只挪
# 300px，仍在屏幕内，探不到这一档：一旦窗口中心飘出所有显示器，认显示器的那次
# MonitorFromPoint 就得靠 MONITOR_DEFAULTTONEAREST 兜底，走的是另一条分支。

$ErrorActionPreference = 'Stop'

Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public class Strand {
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr p);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int cx, int cy, uint flags);
  [DllImport("user32.dll")] public static extern IntPtr MonitorFromPoint(POINT pt, uint flags);
  [DllImport("user32.dll")] public static extern bool GetMonitorInfoW(IntPtr mon, ref MONITORINFO mi);
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
  public static void Move(IntPtr h, int x, int y) { SetWindowPos(h, IntPtr.Zero, x, y, 0, 0, 0x0004 | 0x0010 | 0x0001); }
  public static RECT Work(int x, int y) {
    var pt = new POINT(); pt.X = x; pt.Y = y;
    var mi = new MONITORINFO(); mi.cbSize = Marshal.SizeOf(typeof(MONITORINFO));
    GetMonitorInfoW(MonitorFromPoint(pt, 2), ref mi);
    return mi.Work;
  }
}
'@

[void][Strand]::SetProcessDPIAware()

$proc = Get-Process -Name 'hovernote' -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $proc) { throw 'HoverNote 没在跑' }

$orb = $null
foreach ($h in [Strand]::ByPid([uint32]$proc.Id)) {
  $r = [Strand]::Rect($h)
  $w = $r.Right - $r.Left; $ht = $r.Bottom - $r.Top
  if ($w -gt 0 -and $w -eq $ht -and $w -le 120) { $orb = $h; break }
}
if (-not $orb) { throw '没找到折角窗口' }

$wa0 = [Strand]::Work(10, 10)
Write-Host ("主屏工作区 = ({0},{1})-({2},{3})" -f $wa0.Left, $wa0.Top, $wa0.Right, $wa0.Bottom)

function Show([string]$tag) {
  $r = [Strand]::Rect($orb)
  $wa = [Strand]::Work((($r.Left + $r.Right) / 2), (($r.Top + $r.Bottom) / 2))
  $flush = ($r.Right -eq $wa.Right) -and ($r.Bottom -eq $wa.Bottom)
  "{0,-8} rect=({1},{2})-({3},{4}) {5}x{6}  贴合右下={7}" -f `
    $tag, $r.Left, $r.Top, $r.Right, $r.Bottom, ($r.Right - $r.Left), ($r.Bottom - $r.Top), $flush
}

function Wait-Snap([int]$ms = 4000) {
  $sw = [Diagnostics.Stopwatch]::StartNew()
  while ($sw.ElapsedMilliseconds -lt $ms) {
    $r = [Strand]::Rect($orb)
    $wa = [Strand]::Work((($r.Left + $r.Right) / 2), (($r.Top + $r.Bottom) / 2))
    if ($r.Right -eq $wa.Right -and $r.Bottom -eq $wa.Bottom) { return [int]$sw.ElapsedMilliseconds }
    Start-Sleep -Milliseconds 40
  }
  return -1
}

foreach ($case in @(
    @{ n = '右下角外 (分辨率变小)'; x = $wa0.Right + 400; y = $wa0.Bottom + 400 },
    @{ n = '正右方远处';           x = $wa0.Right + 2000; y = 300 },
    @{ n = '左上角外 (负坐标)';    x = -600; y = -600 }
  )) {
  Write-Host ''
  Write-Host ('--- ' + $case.n + ' ---')
  [Strand]::Move($orb, $case.x, $case.y)
  Start-Sleep -Milliseconds 60
  Show '甩出后'
  $t = Wait-Snap
  if ($t -ge 0) { Write-Host "校回用时 ${t}ms" } else { Write-Host '!! 4 秒内没校回来' }
  Show '结果'
}
