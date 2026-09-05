# 验证辅助：在指定位置铺一块彩色背板，用来判断毛玻璃面板的透明度和染色。
# 桌面是纯黑的话亚克力看不出任何效果，必须有非黑背景做参照。
#   powershell -File tools/backdrop.ps1 -X 3052 -Y 1024 -W 760 -H 940
# 进程一直跑到被 kill 为止。
param([int]$X = 0, [int]$Y = 0, [int]$W = 800, [int]$H = 900)

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public class Dpi { [DllImport("user32.dll")] public static extern bool SetProcessDPIAware(); }
"@
[void][Dpi]::SetProcessDPIAware()

$form = New-Object System.Windows.Forms.Form
$form.FormBorderStyle = [System.Windows.Forms.FormBorderStyle]::None
$form.StartPosition = [System.Windows.Forms.FormStartPosition]::Manual
$form.Location = New-Object System.Drawing.Point($X, $Y)
$form.ClientSize = New-Object System.Drawing.Size($W, $H)
$form.ShowInTaskbar = $false
$form.TopMost = $false

$form.Add_Paint({
  param($src, $e)
  $g = $e.Graphics
  # 斜向彩色渐变 + 粗网格：渐变看染色，网格看模糊强度
  $rect = New-Object System.Drawing.Rectangle 0, 0, $W, $H
  $c1 = [System.Drawing.Color]::FromArgb(255, 240, 120, 40)
  $c2 = [System.Drawing.Color]::FromArgb(255, 40, 110, 240)
  $brush = New-Object System.Drawing.Drawing2D.LinearGradientBrush $rect, $c1, $c2, 35.0
  $g.FillRectangle($brush, $rect)

  $pen = New-Object System.Drawing.Pen ([System.Drawing.Color]::FromArgb(255, 255, 255, 255)), 6
  for ($i = 0; $i -lt $W; $i += 80) { $g.DrawLine($pen, $i, 0, $i, $H) }
  for ($j = 0; $j -lt $H; $j += 80) { $g.DrawLine($pen, 0, $j, $W, $j) }
  $pen.Dispose(); $brush.Dispose()
})

$form.Show()
[System.Windows.Forms.Application]::Run($form)
