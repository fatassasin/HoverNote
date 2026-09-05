# 验证删除的笔记会被留在笔记目录的 trash\ 子目录下（笔记目录见 HOVERNOTE_DIR，
# 没设就是 %USERPROFILE%\Documents\HoverNote）。
#
#   powershell -ExecutionPolicy Bypass -File tools/trash-test.ps1 -Phase make
#   powershell -ExecutionPolicy Bypass -File tools/trash-test.ps1 -Phase kill -Row 2
#
# 分两步是为了安全：make 之后先看截图确认哪一行是新建的那篇，再决定 kill 点第几行。
# 直接一把梭很容易点错行，把真正的笔记删掉。

param(
  [ValidateSet('make', 'kill')] [string]$Phase = 'make',
  [int]$Row = 2
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public class Tt {
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
    SetCursorPos(x, y); System.Threading.Thread.Sleep(120);
    mouse_event(0x0002, 0, 0, 0, IntPtr.Zero); System.Threading.Thread.Sleep(45);
    mouse_event(0x0004, 0, 0, 0, IntPtr.Zero);
  }
}
'@

[void][Tt]::SetProcessDPIAware()

$proc = Get-Process -Name 'hovernote' -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $proc) { throw 'HoverNote 没在跑' }
$pid32 = [uint32]$proc.Id

# 折角的命中区是沿卷角轮廓裁的多边形，窗口中心在它外面，得往右下角里挑一点
$Orb = @(3834, 2154)

function Open-Panel {
  [void][Tt]::SetCursorPos(3600, 1900)
  Start-Sleep -Milliseconds 250
  [void][Tt]::SetCursorPos($Orb[0], $Orb[1])
  Start-Sleep -Milliseconds 1600
  $p = [Tt]::Panel($pid32)
  if (($p.Right - $p.Left) -le 0) { throw '面板没弹出来，中止——不能对着桌面乱点' }
  return $p
}

$p = Open-Panel
Write-Host "面板 ($($p.Left),$($p.Top))-($($p.Right),$($p.Bottom))"

# 200% 缩放：CSS 像素 ×2
$S = 2
$Tab = @(($p.Left + 56), ($p.Top + 22))

function Shot([string]$tag) {
  $out = "$env:TEMP\hn-trash-$tag.png"
  $chk = [Tt]::Panel($pid32)
  if (($chk.Right - $chk.Left) -le 0) { throw "截图时面板已经不在了 ($tag)" }
  $w = $p.Right - $p.Left; $h = 340
  $bmp = New-Object System.Drawing.Bitmap $w, $h
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen((New-Object System.Drawing.Point $p.Left, $p.Top),
                    [System.Drawing.Point]::Empty,
                    (New-Object System.Drawing.Size $w, $h))
  $bmp.Save($out, [System.Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose(); $bmp.Dispose()
  Write-Host "  -> $out"
}

if ($Phase -eq 'make') {
  # 新建按钮在右下角；折角也在右下，让位规则把它挪到 right/bottom 各 16 CSS px
  $fab = @(($p.Right - 33 * $S), ($p.Bottom - 33 * $S))
  Write-Host "1. 点新建 ($($fab[0]),$($fab[1]))"
  [Tt]::Click($fab[0], $fab[1])
  Start-Sleep -Milliseconds 700

  Write-Host '2. 往编辑区打字（新建后焦点就在编辑区）'
  [System.Windows.Forms.SendKeys]::SendWait('TRASH TEST 12345{ENTER}second line{ENTER}third line')
  # 存盘有 400ms 防抖，多等一会儿
  Start-Sleep -Milliseconds 1400

  Write-Host '3. 鼠标挪回面板里，别让它自动收起'
  [void][Tt]::SetCursorPos(($p.Left + 200), ($p.Top + 200))
  Start-Sleep -Milliseconds 300
  Write-Host '4. 拉开笔记栏'
  [void][Tt]::SetCursorPos($Tab[0], $Tab[1])
  Start-Sleep -Milliseconds 900
  Shot 'drawer'
  Write-Host ''
  Write-Host '看截图确认新建的是第几行，再跑 -Phase kill -Row <n>'
}
else {
  Write-Host "1. 拉开笔记栏"
  [void][Tt]::SetCursorPos($Tab[0], $Tab[1])
  Start-Sleep -Milliseconds 900

  # 笔记栏 top:30 padding:5，每行 30 —— 第 n 行中心 = 30+5+30*(n-1)+15 CSS
  $rowY = $p.Top + (50 + 30 * ($Row - 1)) * $S
  # X 按钮：笔记栏右边距 8、行右内边距 4、按钮宽 20 —— 中心离面板右边 22 CSS
  $delX = $p.Right - 22 * $S

  Write-Host "2. 悬停到第 $Row 行的 X 上 ($delX,$rowY)，先截一张确认位置"
  [void][Tt]::SetCursorPos($delX, $rowY)
  Start-Sleep -Milliseconds 600
  Shot "hover-row$Row"

  Write-Host '3. 点 X'
  [Tt]::Click($delX, $rowY)
  Start-Sleep -Milliseconds 1200
  Shot "after-delete"
}
