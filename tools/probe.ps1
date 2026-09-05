# 验证辅助脚本：列出 HoverNote 的窗口矩形、截屏、模拟点击。
#   powershell -File tools/probe.ps1 -Action shot   -Out C:\tmp\a.png
#   powershell -File tools/probe.ps1 -Action list
#   powershell -File tools/probe.ps1 -Action click  -X 1800 -Y 900
param(
  [string]$Action = 'list',
  [string]$Out = "$env:TEMP\hovernote-shot.png",
  [int]$X = 0,
  [int]$Y = 0,
  [int]$X2 = 0,
  [int]$Y2 = 0
)

Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms

Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public class Probe {
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr p);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowTextW(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint x, uint y, uint d, IntPtr e);
  public delegate bool EnumProc(IntPtr h, IntPtr p);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }

  public static string List(string match) {
    var sb = new StringBuilder();
    EnumWindows((h, p) => {
      if (!IsWindowVisible(h)) return true;
      var t = new StringBuilder(256);
      GetWindowTextW(h, t, 256);
      if (t.ToString() != match) return true;
      RECT r; GetWindowRect(h, out r);
      sb.AppendLine(h.ToInt64() + "\t" + r.Left + "\t" + r.Top + "\t" + (r.Right - r.Left) + "\t" + (r.Bottom - r.Top));
      return true;
    }, IntPtr.Zero);
    return sb.ToString();
  }

  public static void Click(int x, int y) {
    SetCursorPos(x, y);
    System.Threading.Thread.Sleep(120);
    mouse_event(0x0002, 0, 0, 0, IntPtr.Zero); // LEFTDOWN
    System.Threading.Thread.Sleep(60);
    mouse_event(0x0004, 0, 0, 0, IntPtr.Zero); // LEFTUP
  }

  // 按下 → 分步移动 → 松开。分步是必须的：一步跳过去只产生一个 move 事件，
  // 前端的拖动阈值和节流都来不及走完。
  public static void Drag(int x1, int y1, int x2, int y2) {
    SetCursorPos(x1, y1);
    System.Threading.Thread.Sleep(150);
    mouse_event(0x0002, 0, 0, 0, IntPtr.Zero);
    System.Threading.Thread.Sleep(80);
    for (int i = 1; i <= 12; i++) {
      SetCursorPos(x1 + (x2 - x1) * i / 12, y1 + (y2 - y1) * i / 12);
      System.Threading.Thread.Sleep(45);
    }
    System.Threading.Thread.Sleep(120);
    mouse_event(0x0004, 0, 0, 0, IntPtr.Zero);
  }
}
"@

[void][Probe]::SetProcessDPIAware()

switch ($Action) {
  'list' {
    Write-Output "hwnd`tleft`ttop`twidth`theight"
    Write-Output ([Probe]::List('HoverNote'))
  }
  'move' {
    [void][Probe]::SetCursorPos($X, $Y)
  }
  'click' {
    [Probe]::Click($X, $Y)
  }
  'drag' {
    [Probe]::Drag($X, $Y, $X2, $Y2)
  }
  'shot' {
    $b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
    $bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($b.Location, [System.Drawing.Point]::Empty, $b.Size)
    $bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
    Write-Output "$Out $($b.Width)x$($b.Height)"
  }
}
