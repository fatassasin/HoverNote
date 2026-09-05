# HoverNote 安装：装到用户目录、建开始菜单快捷方式、登记开机自启、启动。
# 全程不需要管理员权限——装在 %LOCALAPPDATA% 下，自启走「启动」文件夹。
#
#   powershell -ExecutionPolicy Bypass -File tools/install.ps1
#   powershell -ExecutionPolicy Bypass -File tools/install.ps1 -NoAutoStart
#   powershell -ExecutionPolicy Bypass -File tools/install.ps1 -DataDir 'D:\我的笔记'
#   powershell -ExecutionPolicy Bypass -File tools/install.ps1 -Uninstall

param(
  [switch]$NoAutoStart,
  [switch]$Uninstall,
  # 笔记存放目录。给了就写进用户级环境变量 HOVERNOTE_DIR，程序每次启动都读它。
  # 不给就沿用已经设过的值，都没有就是 %USERPROFILE%\Documents\HoverNote。
  [string]$DataDir
)

$ErrorActionPreference = 'Stop'

$AppName   = 'HoverNote'
$Root      = Split-Path -Parent $PSScriptRoot
$InstallDir = Join-Path $env:LOCALAPPDATA "Programs\$AppName"
$StartMenu = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs'
$Shortcut  = Join-Path $StartMenu "$AppName.lnk"
$RunKey    = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$ExePath   = Join-Path $InstallDir "$AppName.exe"

# 和 state.rs 里的 resolve_store_dir 走同一套规则。两边必须一致——这个函数只是
# 用来打印和提示的，真正决定写到哪儿的是程序自己，说的和做的不一样最误导人。
function Get-DataDir {
  if ($env:HOVERNOTE_DIR) { return $env:HOVERNOTE_DIR }
  # 进程环境里没有不代表没设过：用户级变量要等下次登录才会进到这个进程里来。
  $user = [Environment]::GetEnvironmentVariable('HOVERNOTE_DIR', 'User')
  if ($user) { return $user }
  return (Join-Path $env:USERPROFILE 'Documents\HoverNote')
}

# 自启只走「启动」文件夹，不再写 HKCU 的 Run 键。
#
# 两个理由。一是可见可控：启动文件夹里的快捷方式会出现在「设置 → 应用 → 启动」
# 里，用户可以自己开关；同时登记 Run 键则会在那份列表里出现两条同名项，开关一条
# 另一条照跑。二是这台机器上 Run 键本来就不可靠：Shell-Core 日志（事件 9705/9707）
# 显示每次登录都会枚举该键但只执行其中一部分条目，HoverNote 从未被执行过，而同一
# 次登录里启动文件夹的条目全部正常拉起。
#
# 快捷方式直接指向 GUI 子系统的 exe（PE Subsystem = 2），登录时不会经过任何
# 控制台宿主，所以不会闪黑框。
$StartupDir = [Environment]::GetFolderPath('Startup')
$StartupLnk = Join-Path $StartupDir "$AppName.lnk"
# 任务管理器/设置里禁用过的启动项，记录在这两个键下。
$ApprovedFolder = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\StartupFolder'
$ApprovedRun    = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run'

function Stop-App {
  Get-Process -Name 'hovernote' -ErrorAction SilentlyContinue |
    Stop-Process -Force -ErrorAction SilentlyContinue
  Start-Sleep -Milliseconds 600
}

if ($Uninstall) {
  Stop-App
  Remove-ItemProperty -Path $RunKey -Name $AppName -ErrorAction SilentlyContinue
  Remove-ItemProperty -Path $ApprovedRun -Name $AppName -ErrorAction SilentlyContinue
  Remove-ItemProperty -Path $ApprovedFolder -Name "$AppName.lnk" -ErrorAction SilentlyContinue
  Remove-Item $StartupLnk -ErrorAction SilentlyContinue
  Remove-Item $Shortcut -ErrorAction SilentlyContinue
  Remove-Item $InstallDir -Recurse -Force -ErrorAction SilentlyContinue
  # 笔记一律不删。卸载和「不要这些笔记了」是两件事，替人做后一个决定没有回头路。
  # HOVERNOTE_DIR 也留着：重新装回来时还能对上原来那批笔记。
  Write-Host "已卸载。笔记数据保留在 $(Get-DataDir)\ ，可自行删除。"
  exit 0
}

if ($DataDir) {
  # 写用户级变量，登录后启动的进程都能读到。
  [Environment]::SetEnvironmentVariable('HOVERNOTE_DIR', $DataDir, 'User')
  # 同时塞进当前进程：下面 Start-Process 拉起来的程序继承的是这份环境，
  # 只写注册表的话得等下次登录才生效，这一次启动会落到默认目录去。
  $env:HOVERNOTE_DIR = $DataDir
  New-Item -ItemType Directory -Force -Path $DataDir | Out-Null
}

# release 优先；没构建过就退而用 debug，至少能先跑起来
$src = Join-Path $Root 'src-tauri\target\release\hovernote.exe'
if (-not (Test-Path $src)) {
  $src = Join-Path $Root 'src-tauri\target\debug\hovernote.exe'
}
if (-not (Test-Path $src)) {
  throw "找不到已构建的可执行文件。先运行：cd src-tauri && cargo build --release"
}
Write-Host "使用: $src"

Stop-App
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
Copy-Item $src $ExePath -Force

# 快捷方式的图标指向单独的 .ico，比依赖 exe 里嵌的资源更稳
$ico = Join-Path $Root 'src-tauri\icons\icon.ico'
$icoDst = Join-Path $InstallDir 'icon.ico'
if (Test-Path $ico) { Copy-Item $ico $icoDst -Force }

$wsh = New-Object -ComObject WScript.Shell

function New-AppShortcut([string]$Path) {
  $lnk = $wsh.CreateShortcut($Path)
  $lnk.TargetPath = $ExePath
  $lnk.WorkingDirectory = $InstallDir
  $lnk.Description = '屏幕角上折起的一角，鼠标浮上去展开笔记'
  if (Test-Path $icoDst) { $lnk.IconLocation = "$icoDst,0" } else { $lnk.IconLocation = "$ExePath,0" }
  $lnk.Save()
}

New-AppShortcut $Shortcut
Write-Host "开始菜单快捷方式: $Shortcut"

# 早先的版本同时写过 Run 键，会在「设置 → 启动」里多出一条重复项；一律清掉。
Remove-ItemProperty -Path $RunKey -Name $AppName -ErrorAction SilentlyContinue
Remove-ItemProperty -Path $ApprovedRun -Name $AppName -ErrorAction SilentlyContinue

if ($NoAutoStart) {
  Remove-Item $StartupLnk -ErrorAction SilentlyContinue
  Write-Host '开机自启: 已关闭'
} else {
  New-AppShortcut $StartupLnk
  # 之前在任务管理器/设置里被禁用过的话，记录留在 StartupApproved 下，
  # 光放回快捷方式是不会启动的；删掉那条记录让它回到默认的"已启用"。
  Remove-ItemProperty -Path $ApprovedFolder -Name "$AppName.lnk" -ErrorAction SilentlyContinue
  Write-Host '开机自启: 已开启（启动文件夹，可在「设置 → 应用 → 启动」里开关）'
  Write-Host "  $StartupLnk"
}

Start-Process -FilePath $ExePath -WorkingDirectory $InstallDir
Write-Host ''
Write-Host "笔记目录: $(Get-DataDir)"
Write-Host '  （换个地方存：-DataDir ''D:\我的笔记''，或自行设环境变量 HOVERNOTE_DIR）'
Write-Host ''
Write-Host "装好了。折角在屏幕右下角，鼠标浮上去就展开。"
Write-Host "开始菜单搜索 “$AppName” 可以随时启动；托盘图标右键可以退出。"
