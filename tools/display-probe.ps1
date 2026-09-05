# 列出每块显示器的「分辨率@DPI」，并和存档里分配置记下的面板尺寸对照。
#
#   powershell -ExecutionPolicy Bypass -File tools/display-probe.ps1
#
# 这里算出来的 key 必须和 platform.rs 里 display_key 给的一模一样——面板尺寸就是
# 按它分开记的。key 要是对不上（比如启动那一瞬间 DPI 还没定下来），存档里就会多出
# 一套根本不存在的显示配置，而真正那块屏的记忆反倒可能被挤掉。

$ErrorActionPreference = 'Stop'

Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public class Disp {
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool EnumDisplayMonitors(IntPtr dc, IntPtr clip, EnumProc cb, IntPtr d);
  [DllImport("user32.dll")] public static extern bool GetMonitorInfoW(IntPtr mon, ref MONITORINFO mi);
  [DllImport("user32.dll")] public static extern IntPtr MonitorFromWindow(IntPtr h, uint flags);
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWinProc cb, IntPtr p);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetClassName(IntPtr h, System.Text.StringBuilder buf, int n);
  [DllImport("shcore.dll")] public static extern int GetDpiForMonitor(IntPtr mon, int type, out uint x, out uint y);
  public delegate bool EnumProc(IntPtr m, IntPtr dc, IntPtr r, IntPtr d);
  public delegate bool EnumWinProc(IntPtr h, IntPtr p);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  [StructLayout(LayoutKind.Sequential)] public struct MONITORINFO { public int cbSize; public RECT Monitor, Work; public uint Flags; }

  public static List<string> Monitors() {
    var outp = new List<string>();
    EnumDisplayMonitors(IntPtr.Zero, IntPtr.Zero, delegate(IntPtr m, IntPtr dc, IntPtr r, IntPtr d) {
      var mi = new MONITORINFO(); mi.cbSize = Marshal.SizeOf(typeof(MONITORINFO));
      GetMonitorInfoW(m, ref mi);
      uint dx, dy; GetDpiForMonitor(m, 0, out dx, out dy);
      bool primary = (mi.Flags & 1u) != 0u;
      outp.Add(string.Format("{0}x{1}@{2}   rect=({3},{4})-({5},{6})  工作区=({7},{8})-({9},{10})  主屏={11}",
        mi.Monitor.Right - mi.Monitor.Left, mi.Monitor.Bottom - mi.Monitor.Top, dx,
        mi.Monitor.Left, mi.Monitor.Top, mi.Monitor.Right, mi.Monitor.Bottom,
        mi.Work.Left, mi.Work.Top, mi.Work.Right, mi.Work.Bottom, primary));
      return true;
    }, IntPtr.Zero);
    return outp;
  }

  public static List<IntPtr> ByPid(uint want) {
    var list = new List<IntPtr>();
    EnumWindows(delegate(IntPtr h, IntPtr p) {
      uint pid; GetWindowThreadProcessId(h, out pid);
      if (pid == want) list.Add(h);
      return true;
    }, IntPtr.Zero);
    return list;
  }

  public static RECT Rect(IntPtr h) { RECT r; GetWindowRect(h, out r); return r; }

  public static string Cls(IntPtr h) {
    var b = new System.Text.StringBuilder(256);
    GetClassName(h, b, b.Capacity);
    return b.ToString();
  }

  // 和 platform.rs 的 display_key 走同一组调用：MonitorFromWindow + GetDpiForWindow
  public static string Key(IntPtr h) {
    uint dpi = GetDpiForWindow(h);
    IntPtr m = MonitorFromWindow(h, 2); // MONITOR_DEFAULTTONEAREST
    var mi = new MONITORINFO(); mi.cbSize = Marshal.SizeOf(typeof(MONITORINFO));
    if (m == IntPtr.Zero || !GetMonitorInfoW(m, ref mi) || dpi == 0) return "<拿不到>";
    return string.Format("{0}x{1}@{2}",
      mi.Monitor.Right - mi.Monitor.Left, mi.Monitor.Bottom - mi.Monitor.Top, dpi);
  }
}
'@

[void][Disp]::SetProcessDPIAware()

Write-Host '=== 系统里实际有的显示器 ==='
[Disp]::Monitors() | ForEach-Object { Write-Host "  $_" }

$proc = Get-Process -Name 'hovernote' -ErrorAction SilentlyContinue | Select-Object -First 1
if ($proc) {
  Write-Host ''
  Write-Host "=== HoverNote 的窗口（PID $($proc.Id)）==="
  foreach ($h in [Disp]::ByPid([uint32]$proc.Id)) {
    $r = [Disp]::Rect($h)
    $w = $r.Right - $r.Left; $ht = $r.Bottom - $r.Top
    if ($w -le 0 -or $ht -le 0) { continue }
    # 一个进程里不止两个窗口：托盘图标要一个收消息的窗（class=tray_icon_app，2880x1599），
    # tao 还有个事件靶窗（class=Tao Thread Event Target，26x26 的方块）。光按「小方块
    # 就是折角」会把后者认成折角。真正的应用窗口 class 是 `Tauri Window`，只有这两个。
    $cls = [Disp]::Cls($h)
    $tag = if ($cls -ne 'Tauri Window') { '非应用' }
           elseif ($w -eq $ht) { '折角' }
           else { '面板' }
    Write-Host ("  {0,-8} {1}x{2} @({3},{4})  key={5}  class={6}" -f $tag, $w, $ht, $r.Left, $r.Top, [Disp]::Key($h), $cls)
  }
} else {
  Write-Host ''
  Write-Host 'HoverNote 没在跑'
}

# 笔记目录和 state.rs 的 resolve_store_dir 用同一套规则：先看环境变量，再退到文档目录。
# 用户级变量要下次登录才进得到这个进程里，所以两处都得看。
$dir = $env:HOVERNOTE_DIR
if (-not $dir) { $dir = [Environment]::GetEnvironmentVariable('HOVERNOTE_DIR', 'User') }
if (-not $dir) { $dir = Join-Path $env:USERPROFILE 'Documents\HoverNote' }
$store = Join-Path $dir 'hovernote.json'
Write-Host ''
Write-Host "=== 存档 ($store) ==="
if (Test-Path $store) {
  $j = Get-Content $store -Raw | ConvertFrom-Json
  Write-Host '分显示配置记着的面板尺寸（最近用过的在最前）：'
  Write-Host ("  台面上: panel={0}x{1}  exp={2}x{3}" -f $j.panel_w, $j.panel_h, $j.exp_w, $j.exp_h)
  if (-not $j.layouts -or $j.layouts.Count -eq 0) {
    Write-Host '  （空）'
  } else {
    $j.layouts | ForEach-Object {
      Write-Host ("  {0,-16} panel={1}x{2}  exp={3}x{4} @({5},{6})" -f `
        $_.key, $_.panel_w, $_.panel_h, $_.exp_w, $_.exp_h, $_.exp_x, $_.exp_y)
    }
  }
} else {
  Write-Host '  （还没有这个文件——没跑过，或者 HOVERNOTE_DIR 指到了别处）'
}
