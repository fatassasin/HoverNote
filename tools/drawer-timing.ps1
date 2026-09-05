# 量「点三角形收起」的时序：点下去之后多久开始变化、多久稳定。
#
#   powershell -ExecutionPolicy Bypass -File tools/drawer-timing.ps1
#
# 做法是连续抓笔记栏那一小块的屏幕像素、算平均亮度。抽屉开着时那块有高亮行和
# 白字，收起后露出的是编辑区，两者亮度差得开。看的是曲线什么时候**开始**动——
# 改之前那 220ms 定时器的症状是「点下去之后一段时间画面完全静止」。

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Drawing;
using System.Runtime.InteropServices;
public class Dt {
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint x, uint y, uint d, IntPtr e);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr p);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  public delegate bool EnumProc(IntPtr h, IntPtr p);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }

  public static RECT Panel(uint want) {
    var found = new RECT();
    EnumWindows((h, p) => {
      uint pid; GetWindowThreadProcessId(h, out pid);
      if (pid != want || !IsWindowVisible(h)) return true;
      RECT r; GetWindowRect(h, out r);
      if (r.Right - r.Left > 200 && r.Bottom - r.Top > 200) { found = r; return false; }
      return true;
    }, IntPtr.Zero);
    return found;
  }

  public static void Click(int x, int y) {
    SetCursorPos(x, y); System.Threading.Thread.Sleep(90);
    mouse_event(0x0002, 0, 0, 0, IntPtr.Zero); System.Threading.Thread.Sleep(40);
    mouse_event(0x0004, 0, 0, 0, IntPtr.Zero);
  }

  // 抓一小块算平均亮度。稀疏取样（每 4 像素一个）就够分辨开/关了。
  public static double Bright(int x, int y, int w, int h) {
    using (var bmp = new Bitmap(w, h))
    using (var g = Graphics.FromImage(bmp)) {
      g.CopyFromScreen(new Point(x, y), Point.Empty, new Size(w, h));
      double sum = 0; int n = 0;
      for (int j = 0; j < h; j += 4)
        for (int i = 0; i < w; i += 4) {
          var c = bmp.GetPixel(i, j);
          sum += (c.R + c.G + c.B) / 3.0; n++;
        }
      return sum / n;
    }
  }

  // 点一下之后立刻开始连续采样，返回 "毫秒 亮度" 的序列
  public static List<string> Trace(int cx, int cy, int x, int y, int w, int h, int frames) {
    var outp = new List<string>();
    var sw = System.Diagnostics.Stopwatch.StartNew();
    Click(cx, cy);
    sw.Restart();   // t0 = 松开鼠标那一刻
    for (int i = 0; i < frames; i++) {
      long t = sw.ElapsedMilliseconds;
      outp.Add(t + " " + Bright(x, y, w, h).ToString("F1"));
      System.Threading.Thread.Sleep(8);
    }
    return outp;
  }
}
'@

[void][Dt]::SetProcessDPIAware()

$proc = Get-Process -Name 'hovernote' -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $proc) { throw 'HoverNote 没在跑' }
$pid32 = [uint32]$proc.Id

Write-Host '1. 移到折角上，等面板弹出'

# 用 SetCursorPos 瞬移过去，两次跳变可能被输入层合并，webview 观察不到指针
# 「离开又进入」，于是 mouseenter 不触发、面板不弹。真实鼠标是连续移动的，不会
# 这样——所以这里分步逼近，并且轮询等待、必要时把光标挪开再回来重试。
function Wait-Panel {
  $steps = @(@(2000, 1200), @(3400, 1800), @(3700, 2050), @(3830, 2150))
  for ($try = 1; $try -le 4; $try++) {
    foreach ($s in $steps) {
      [void][Dt]::SetCursorPos($s[0], $s[1])
      Start-Sleep -Milliseconds 220
    }
    $sw = [Diagnostics.Stopwatch]::StartNew()
    while ($sw.ElapsedMilliseconds -lt 2500) {
      $r = [Dt]::Panel($pid32)
      if (($r.Right - $r.Left) -gt 0) { return $r }
      Start-Sleep -Milliseconds 100
    }
    Write-Host "   第 $try 次没拉出来，挪开重试"
  }
  throw '面板没弹出来，中止——不能对着桌面乱点'
}

$p = Wait-Panel
$L = $p.Left; $T = $p.Top
Write-Host "   面板 ($L,$T)-($($p.Right),$($p.Bottom))"

$Tab = @(($L + 56), ($T + 22))
$CropX = $L + 8; $CropY = $T + 56; $CropW = 460; $CropH = 110

Write-Host '2. 移到三角形上（悬停拉出笔记栏）'
[void][Dt]::SetCursorPos($Tab[0], $Tab[1])
Start-Sleep -Milliseconds 800

Write-Host '3. 点一下钉住'
[Dt]::Click($Tab[0], $Tab[1])
Start-Sleep -Milliseconds 700
$open = [Dt]::Bright($CropX, $CropY, $CropW, $CropH)
Write-Host ("   钉住后亮度 {0:F1}" -f $open)

Write-Host '4. 再点一下收起，连续采样：'
$trace = [Dt]::Trace($Tab[0], $Tab[1], $CropX, $CropY, $CropW, $CropH, 34)
foreach ($line in $trace) {
  $parts = $line.Split(' ')
  $bar = '#' * [int]([double]$parts[1] / 2)
  "{0,5}ms  {1,6}  {2}" -f $parts[0], $parts[1], $bar
}
