# 验证折角会自己校回角上：把它强行挪走 / 改大，然后看它多久回到贴死的位置。
#
#   powershell -ExecutionPolicy Bypass -File tools/snap-test.ps1
#
# 折角是 HoverNote 进程里那个正方形的小窗口，按尺寸认，不按标题——两个窗口
# 标题都是 HoverNote。

$ErrorActionPreference = 'Stop'

Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public class Snap {
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
  // SWP_NOZORDER | SWP_NOACTIVATE
  public static void Move(IntPtr h, int x, int y) { SetWindowPos(h, IntPtr.Zero, x, y, 0, 0, 0x0004 | 0x0010 | 0x0001); }
  public static void Resize(IntPtr h, int w, int hgt) { SetWindowPos(h, IntPtr.Zero, 0, 0, w, hgt, 0x0004 | 0x0010 | 0x0002); }
  public static RECT Work(int x, int y) {
    var pt = new POINT(); pt.X = x; pt.Y = y;
    var mi = new MONITORINFO(); mi.cbSize = Marshal.SizeOf(typeof(MONITORINFO));
    GetMonitorInfoW(MonitorFromPoint(pt, 2), ref mi);
    return mi.Work;
  }
}
'@

[void][Snap]::SetProcessDPIAware()

$proc = Get-Process -Name 'hovernote' -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $proc) { throw 'HoverNote 没在跑' }

$orb = $null
foreach ($h in [Snap]::ByPid([uint32]$proc.Id)) {
  $r = [Snap]::Rect($h)
  $w = $r.Right - $r.Left; $ht = $r.Bottom - $r.Top
  if ($w -gt 0 -and $w -eq $ht -and $w -le 120) { $orb = $h; break }
}
if (-not $orb) { throw '没找到折角窗口（可能还在冷启动，WebView2 没把窗口建出来）' }

function Show([string]$tag) {
  $r = [Snap]::Rect($orb)
  $wa = [Snap]::Work((($r.Left + $r.Right) / 2), (($r.Top + $r.Bottom) / 2))
  $flush = ($r.Right -eq $wa.Right) -and ($r.Bottom -eq $wa.Bottom)
  "{0,-10} rect=({1},{2})-({3},{4}) {5}x{6}  工作区右下=({7},{8})  贴合={9}" -f `
    $tag, $r.Left, $r.Top, $r.Right, $r.Bottom, ($r.Right - $r.Left), ($r.Bottom - $r.Top), $wa.Right, $wa.Bottom, $flush
}

function Wait-Snap([int]$ms = 3000) {
  $sw = [Diagnostics.Stopwatch]::StartNew()
  while ($sw.ElapsedMilliseconds -lt $ms) {
    $r = [Snap]::Rect($orb)
    $wa = [Snap]::Work((($r.Left + $r.Right) / 2), (($r.Top + $r.Bottom) / 2))
    if ($r.Right -eq $wa.Right -and $r.Bottom -eq $wa.Bottom) { return [int]$sw.ElapsedMilliseconds }
    Start-Sleep -Milliseconds 40
  }
  return -1
}

Show '初始'

Write-Host ''
Write-Host '--- 挪走 300px ---'
$r = [Snap]::Rect($orb)
[Snap]::Move($orb, ($r.Left - 300), ($r.Top - 300))
Show '挪走后'
$t = Wait-Snap
if ($t -ge 0) { Write-Host "校回用时 ${t}ms" } else { Write-Host '3 秒内没校回来' }
Show '结果'

Write-Host ''
Write-Host '--- 改成 160x160 ---'
[Snap]::Resize($orb, 160, 160)
Show '改大后'
$t = Wait-Snap
if ($t -ge 0) { Write-Host "校回用时 ${t}ms" } else { Write-Host '3 秒内没校回来' }
Show '结果'
