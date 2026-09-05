# 验证改名的触发方式：悬停不出输入框、单击不出、双击才出。
#
#   powershell -ExecutionPolicy Bypass -File tools/rename-test.ps1
#
# 截图裁到笔记栏第一行那一小块，输出到 %TEMP%\hn-rename-*.png。

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public class Rn {
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint x, uint y, uint d, IntPtr e);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr p);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  public delegate bool EnumProc(IntPtr h, IntPtr p);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }

  // 进程里可见的、比折角大的那个窗口就是面板；返回 0,0,0,0 表示还没显示
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

  public static void Down() { mouse_event(0x0002, 0, 0, 0, IntPtr.Zero); }
  public static void Up()   { mouse_event(0x0004, 0, 0, 0, IntPtr.Zero); }
  public static void Click(int x, int y) {
    SetCursorPos(x, y); System.Threading.Thread.Sleep(90);
    Down(); System.Threading.Thread.Sleep(45); Up();
  }
  // 两下之间隔 70ms，稳稳落在系统双击时限（默认 500ms）内
  public static void Double(int x, int y) {
    SetCursorPos(x, y); System.Threading.Thread.Sleep(90);
    Down(); System.Threading.Thread.Sleep(40); Up();
    System.Threading.Thread.Sleep(70);
    Down(); System.Threading.Thread.Sleep(40); Up();
  }
}
'@

[void][Rn]::SetProcessDPIAware()

$proc = Get-Process -Name 'hovernote' -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $proc) { throw 'HoverNote 没在跑' }
$pid32 = [uint32]$proc.Id

# 折角窗口是 56x56 贴在右下角，但命中区是沿卷角轮廓裁出来的多边形——
# 窗口中心落在多边形外面（y=50% 处左边界在 75%），必须往右下角里再挑一点。
$Orb = @(3834, 2154)

Write-Host '1. 鼠标移到折角上，等面板弹出'
[void][Rn]::SetCursorPos(3600, 1900)   # 先落在别处，保证下一步是一次真正的移动
Start-Sleep -Milliseconds 250
[void][Rn]::SetCursorPos($Orb[0], $Orb[1])
Start-Sleep -Milliseconds 1500

$p = [Rn]::Panel($pid32)
if (($p.Right - $p.Left) -le 0) { throw '面板没弹出来，中止——不能对着桌面乱点' }
$PanelL = $p.Left; $PanelT = $p.Top
Write-Host "   面板矩形 ($PanelL,$PanelT)-($($p.Right),$($p.Bottom))"

$Tab  = @(($PanelL + 56),  ($PanelT + 22))   # 书签页签
$Name = @(($PanelL + 120), ($PanelT + 100))  # 第一行的名字

function Shot([string]$tag) {
  $out = "$env:TEMP\hn-rename-$tag.png"
  $w = 460; $h = 110
  $bmp = New-Object System.Drawing.Bitmap $w, $h
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $src = New-Object System.Drawing.Point (($PanelL + 8), ($PanelT + 56))
  $size = New-Object System.Drawing.Size $w, $h
  $g.CopyFromScreen($src, [System.Drawing.Point]::Empty, $size)
  $bmp.Save($out, [System.Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose(); $bmp.Dispose()
  Write-Host "  -> $out"
}

Write-Host '2. 移到书签页签上，拉出笔记栏'
[void][Rn]::SetCursorPos($Tab[0], $Tab[1])
Start-Sleep -Milliseconds 700

Write-Host '3. 鼠标悬停在名字上（不点）'
[void][Rn]::SetCursorPos($Name[0], $Name[1])
Start-Sleep -Milliseconds 600
Shot 'hover'

Write-Host '4. 单击一次'
[Rn]::Click($Name[0], $Name[1])
Start-Sleep -Milliseconds 600
Shot 'click1'

Write-Host '5. 双击'
[Rn]::Double($Name[0], $Name[1])
Start-Sleep -Milliseconds 600
Shot 'dblclick'

# 别把笔记名留在编辑态：按 Esc 取消
Add-Type -AssemblyName System.Windows.Forms
[System.Windows.Forms.SendKeys]::SendWait('{ESC}')
Start-Sleep -Milliseconds 400
Shot 'after-esc'
