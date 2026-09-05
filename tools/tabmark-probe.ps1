# 抓顶栏按钮拿到**键盘焦点**时的样子。
#
#   powershell -ExecutionPolicy Bypass -File tools/tabmark-probe.ps1 -Tag before
#
# 复现路径：在编辑区里按 Tab。textarea 不拦 Tab，焦点会跳到顶栏按钮上，这是货真价实
# 的键盘导航，:focus-visible 必然成立，Chromium 就画默认焦点圈。
#
# 走过的弯路（都是合成输入的问题，不是产品缺陷）：
#   * 「点一下按钮 + 事先按过方向键」并不会让 Chromium 判成 focus-visible——
#     鼠标点击拿到的焦点不算，跟之前有没有键盘交互无关。用这条路抓到的图里没有圈。
#   * SetCursorPos 瞬移之后立刻按键，webview 会漏掉 mouseenter/click，所以移动后要停顿。
#   * 面板会自己收，截到黑图，所以每次截图前先断言可见。

param([string]$Tag = 'now')

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public class Tm {
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint x, uint y, uint d, IntPtr e);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint f, IntPtr e);
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
  // 移到位之后停久一点再按下——瞬移 + 立刻按键会被 webview 漏掉
  public static void Click(int x, int y) {
    SetCursorPos(x, y); System.Threading.Thread.Sleep(650);
    mouse_event(0x0002, 0, 0, 0, IntPtr.Zero); System.Threading.Thread.Sleep(60);
    mouse_event(0x0004, 0, 0, 0, IntPtr.Zero);
  }
  public static void Tab() {
    keybd_event(0x09, 0, 0, IntPtr.Zero); System.Threading.Thread.Sleep(60);
    keybd_event(0x09, 0, 2, IntPtr.Zero);
  }
}
'@

[void][Tm]::SetProcessDPIAware()
$proc = Get-Process -Name 'hovernote' -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $proc) { throw 'HoverNote 没在跑' }
$pid32 = [uint32]$proc.Id

function Wait-Panel {
  $steps = @(@(2000, 1200), @(3400, 1800), @(3700, 2050), @(3830, 2150))
  for ($try = 1; $try -le 5; $try++) {
    foreach ($s in $steps) { [void][Tm]::SetCursorPos($s[0], $s[1]); Start-Sleep -Milliseconds 250 }
    $sw = [Diagnostics.Stopwatch]::StartNew()
    while ($sw.ElapsedMilliseconds -lt 2000) {
      $r = [Tm]::Panel($pid32)
      if (($r.Right - $r.Left) -gt 0) { return $r }
      Start-Sleep -Milliseconds 100
    }
    Write-Host "   第 $try 次没拉出来，挪开重试"
  }
  throw '面板始终没弹出来'
}

$p = Wait-Panel
$L = $p.Left; $T = $p.Top
Write-Host "面板 ($L,$T)-($($p.Right),$($p.Bottom))"

# 顶栏整条：左边是书签页签，右边是放大按钮，Tab 会依次落在它们上面
$CW = ($p.Right - $p.Left); $CH = 80

function Shot([string]$name) {
  $v = [Tm]::Panel($pid32)
  if (($v.Right - $v.Left) -le 0) { throw "截 $name 时面板已经不可见了（它自己收了）" }
  $out = "$env:TEMP\hn-tab-$Tag-$name.png"
  $bmp = New-Object System.Drawing.Bitmap $CW, $CH
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen((New-Object System.Drawing.Point($v.Left, $v.Top)), ([System.Drawing.Point]::Empty), (New-Object System.Drawing.Size($CW, $CH)))
  $bmp.Save($out, [System.Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose(); $bmp.Dispose()
  Write-Host "  -> $out"
}

Write-Host '1. 点编辑区拿到焦点'
[Tm]::Click(($L + 400), ($T + 900))
Start-Sleep -Milliseconds 500
Shot 'editor'

Write-Host '2. 按 Tab：焦点跳到顶栏按钮（键盘导航，focus-visible 必然成立）'
[Tm]::Tab()
Start-Sleep -Milliseconds 600
Shot 'tab1'

Write-Host '3. 再按一次 Tab：落到下一个按钮'
[Tm]::Tab()
Start-Sleep -Milliseconds 600
Shot 'tab2'

