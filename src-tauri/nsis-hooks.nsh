; HoverNote 的 NSIS 安装钩子。
;
; 挂在 tauri.conf.json 的 bundle.windows.nsis.installerHooks 上，makensis 会把下面
; 这几个宏插进安装器对应的阶段。Tauri 的默认模板已经管了：装到 %LOCALAPPDATA%、
; 建开始菜单快捷方式、写卸载信息、装完可勾选启动。它唯一不管的是**登录自启**，
; 这个文件补的就是这一件事，规则和 tools/install.ps1 里那套保持一致。
;
; 四个宏全部定义，哪怕是空的：模板里是无条件 !insertmacro 的，少一个就编不过。

; 自启只走「启动」文件夹，不写 HKCU 的 Run 键。
;
; 两个理由。一是可见可控：启动文件夹里的快捷方式会出现在「设置 → 应用 → 启动」里，
; 用户可以自己开关；同时登记 Run 键则会在那份列表里出现两条同名项，关掉一条另一条
; 照跑。二是 Run 键在实测的机器上本来就不可靠：Shell-Core 日志（事件 9705/9707）
; 显示每次登录都会枚举该键但只执行其中一部分条目，HoverNote 从未被执行过，而同一次
; 登录里启动文件夹的条目全部正常拉起。
;
; 快捷方式直接指向 GUI 子系统的 exe（PE Subsystem = 2），登录时不经过控制台宿主，
; 不闪黑框。

!define HN_STARTUP_LNK "HoverNote.lnk"
!define HN_APPROVED_FOLDER "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\StartupFolder"
!define HN_APPROVED_RUN "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run"
!define HN_RUN_KEY "Software\Microsoft\Windows\CurrentVersion\Run"

!macro NSIS_HOOK_PREINSTALL
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; 启动文件夹是每个用户各一份。安装模式在配置里钉死成 currentUser，模板本来也是
  ; current，这里显式再设一次，免得以后改了安装模式这段悄悄写到 All Users 去。
  SetShellVarContext current

  ; 快捷方式的工作目录取自 SetOutPath，不设的话会继承安装器当时的当前目录。
  SetOutPath "$INSTDIR"

  ; 用模板定义的主程序名，别把 "HoverNote.exe" 写死——productName 一改这里就会
  ; 指向一个不存在的文件，而快捷方式指错是不报错的，只是开机之后什么都不发生。
  !ifdef MAINBINARYNAME
    StrCpy $R0 "$INSTDIR\${MAINBINARYNAME}.exe"
  !else
    StrCpy $R0 "$INSTDIR\HoverNote.exe"
  !endif

  CreateShortcut "$SMSTARTUP\${HN_STARTUP_LNK}" "$R0" "" "$R0" 0

  ; 以前在任务管理器/设置里被禁用过的话，记录留在 StartupApproved 下，光把快捷方式
  ; 放回去是不会启动的；删掉那条记录让它回到默认的「已启用」。
  DeleteRegValue HKCU "${HN_APPROVED_FOLDER}" "${HN_STARTUP_LNK}"

  ; 早先的 install.ps1 版本同时写过 Run 键。装到这一步一律清掉，否则从旧版升上来的
  ; 机器会在「设置 → 启动」里留着两条同名项。
  DeleteRegValue HKCU "${HN_RUN_KEY}" "HoverNote"
  DeleteRegValue HKCU "${HN_APPROVED_RUN}" "HoverNote"
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; 卸载时把自启撤干净。这些都在 $INSTDIR 之外，模板删安装目录带不走它们——
  ; 留下的话就是一个指向已删除 exe 的快捷方式，每次登录都试着拉一个不存在的程序。
  SetShellVarContext current
  Delete "$SMSTARTUP\${HN_STARTUP_LNK}"
  DeleteRegValue HKCU "${HN_APPROVED_FOLDER}" "${HN_STARTUP_LNK}"
  DeleteRegValue HKCU "${HN_RUN_KEY}" "HoverNote"
  DeleteRegValue HKCU "${HN_APPROVED_RUN}" "HoverNote"
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; 笔记一个都不删。笔记在 %USERPROFILE%\Documents\HoverNote（或 HOVERNOTE_DIR），
  ; 本来就在安装目录之外，模板碰不到——这里也绝不主动去碰。卸载和「不要这些笔记了」
  ; 是两件事，替人做后一个决定没有回头路。
  ; HOVERNOTE_DIR 同理留着：重新装回来还能对上原来那批笔记。
!macroend
