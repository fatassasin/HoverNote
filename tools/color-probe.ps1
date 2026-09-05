# 量一下面板实际渲染出来的底色。窗口是半透明的，CSS 里写的值不等于屏幕上的值，
# 只有采屏幕像素才算数。
#
#   powershell -ExecutionPolicy Bypass -File tools/color-probe.ps1

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public class Cp {
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
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
}
'@

[void][Cp]::SetProcessDPIAware()
$proc = Get-Process -Name 'hovernote' -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $proc) { throw 'HoverNote 没在跑' }
$pid32 = [uint32]$proc.Id

[void][Cp]::SetCursorPos(3600, 1900); Start-Sleep -Milliseconds 250
[void][Cp]::SetCursorPos(3834, 2154); Start-Sleep -Milliseconds 1600
$p = [Cp]::Panel($pid32)
if (($p.Right - $p.Left) -le 0) { throw '面板没弹出来' }
Write-Host "面板 ($($p.Left),$($p.Top))-($($p.Right),$($p.Bottom))"

# 采编辑区里一块没有文字的地方：正文起始行的左边距内，往下够远避开已有文字的行
$w = 200; $h = 60
$sx = $p.Left + 20; $sy = $p.Bottom - 160
$bmp = New-Object System.Drawing.Bitmap $w, $h
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen((New-Object System.Drawing.Point $sx, $sy),
                  [System.Drawing.Point]::Empty,
                  (New-Object System.Drawing.Size $w, $h))

$tally = @{}
for ($y = 0; $y -lt $h; $y++) {
  for ($x = 0; $x -lt $w; $x++) {
    $c = $bmp.GetPixel($x, $y)
    $k = "{0},{1},{2}" -f $c.R, $c.G, $c.B
    $tally[$k] = 1 + $tally[$k]
  }
}
$g.Dispose(); $bmp.Dispose()

Write-Host "采样区 ($sx,$sy) ${w}x${h}，出现最多的颜色："
$tally.GetEnumerator() | Sort-Object Value -Descending | Select-Object -First 5 | ForEach-Object {
  $rgb = $_.Key -split ','
  $pct = [math]::Round(100 * $_.Value / ($w * $h), 1)
  Write-Host ("  RGB({0,3},{1,3},{2,3})  {3,5}%   b-r = {4}" -f $rgb[0], $rgb[1], $rgb[2], $pct, ([int]$rgb[2] - [int]$rgb[0]))
}
