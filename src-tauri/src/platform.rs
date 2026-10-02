//! Windows 平台层：DWM 圆角/边框、窗口样式、显示器工作区、光标位置。
//!
//! DWM/User32 调用手写 FFI 而不是引入 `windows` crate，是为了不被它的版本
//! 迭代牵着走（HWND 在不同大版本里在 isize 和 *mut c_void 之间反复横跳），
//! 也省掉一份几百 MB 的元数据依赖。

use tauri::WebviewWindow;

/// 物理像素的矩形，left/top 含，right/bottom 不含。
#[derive(Clone, Copy, Debug)]
pub struct Area {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Area {
    pub fn width(&self) -> i32 {
        self.right - self.left
    }
    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
}

#[cfg(windows)]
mod ffi {
    use core::ffi::c_void;

    pub const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
    pub const DWMWA_BORDER_COLOR: u32 = 34;
    pub const DWMWCP_DONOTROUND: u32 = 1;
    pub const DWMWCP_ROUND: u32 = 2;
    /// DWMWA_COLOR_NONE：让 DWM 干脆不画窗口边线。
    pub const DWM_COLOR_NONE: u32 = 0xFFFF_FFFE;
    pub const MONITOR_DEFAULTTONEAREST: u32 = 2;

    /// CreateMutexW 撞名时 GetLastError 的返回值。
    pub const ERROR_ALREADY_EXISTS: u32 = 183;

    pub const GWL_STYLE: i32 = -16;
    pub const GWL_EXSTYLE: i32 = -20;
    pub const WS_CAPTION: i32 = 0x00C0_0000;
    pub const WS_SYSMENU: i32 = 0x0008_0000;
    pub const WS_MINIMIZEBOX: i32 = 0x0002_0000;
    pub const WS_MAXIMIZEBOX: i32 = 0x0001_0000;
    pub const WS_EX_TOOLWINDOW: i32 = 0x0000_0080;
    pub const WS_EX_APPWINDOW: i32 = 0x0004_0000;
    pub const SWP_FRAMECHANGED: u32 = 0x0020;
    pub const SWP_NOMOVE: u32 = 0x0002;
    pub const SWP_NOSIZE: u32 = 0x0001;
    pub const SWP_NOZORDER: u32 = 0x0004;
    pub const SWP_NOACTIVATE: u32 = 0x0010;
    pub const HWND_TOPMOST: isize = -1;
    /// GetAncestor：从命中的子窗口（WebView2 那堆 Chrome_* 子窗）回到顶层窗口。
    pub const GA_ROOT: u32 = 2;

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    pub struct Point {
        pub x: i32,
        pub y: i32,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    pub struct Rect {
        pub left: i32,
        pub top: i32,
        pub right: i32,
        pub bottom: i32,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    pub struct MonitorInfo {
        pub cb_size: u32,
        pub rc_monitor: Rect,
        pub rc_work: Rect,
        pub dw_flags: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    pub struct SystemTime {
        pub year: u16,
        pub month: u16,
        pub day_of_week: u16,
        pub day: u16,
        pub hour: u16,
        pub minute: u16,
        pub second: u16,
        pub milliseconds: u16,
    }

    #[link(name = "dwmapi")]
    extern "system" {
        pub fn DwmSetWindowAttribute(
            hwnd: isize,
            attr: u32,
            value: *const c_void,
            size: u32,
        ) -> i32;
    }

    #[link(name = "kernel32")]
    extern "system" {
        pub fn CreateMutexW(attrs: *const c_void, owner: i32, name: *const u16) -> isize;
        pub fn GetLastError() -> u32;
        pub fn GetLocalTime(time: *mut SystemTime);
        pub fn GlobalAlloc(flags: u32, bytes: usize) -> isize;
        pub fn GlobalLock(mem: isize) -> *mut c_void;
        pub fn GlobalUnlock(mem: isize) -> i32;
        pub fn GlobalFree(mem: isize) -> isize;
        pub fn GlobalSize(mem: isize) -> usize;
        pub fn Sleep(ms: u32);
    }

    /// 剪贴板里的 UTF-16 文本。
    pub const CF_UNICODETEXT: u32 = 13;
    /// SetClipboardData 要的是可移动内存；交出去之后归系统管，不能再释放。
    pub const GMEM_MOVEABLE: u32 = 0x0002;

    pub const SW_SHOWNORMAL: i32 = 1;

    #[link(name = "shell32")]
    extern "system" {
        pub fn ShellExecuteW(
            hwnd: isize,
            verb: *const u16,
            file: *const u16,
            params: *const u16,
            dir: *const u16,
            show: i32,
        ) -> isize;
    }

    pub const COINIT_APARTMENTTHREADED: u32 = 0x2;

    #[link(name = "ole32")]
    extern "system" {
        pub fn CoInitializeEx(reserved: *mut c_void, flags: u32) -> i32;
    }

    #[link(name = "user32")]
    extern "system" {
        pub fn GetCursorPos(point: *mut Point) -> i32;
        pub fn GetDpiForWindow(hwnd: isize) -> u32;
        pub fn MonitorFromPoint(point: Point, flags: u32) -> isize;
        pub fn MonitorFromWindow(hwnd: isize, flags: u32) -> isize;
        pub fn GetMonitorInfoW(monitor: isize, info: *mut MonitorInfo) -> i32;
        pub fn GetWindowLongW(hwnd: isize, index: i32) -> i32;
        pub fn SetWindowLongW(hwnd: isize, index: i32, value: i32) -> i32;
        pub fn WindowFromPoint(point: Point) -> isize;
        pub fn GetAncestor(hwnd: isize, flags: u32) -> isize;
        pub fn GetForegroundWindow() -> isize;
        pub fn OpenClipboard(owner: isize) -> i32;
        pub fn CloseClipboard() -> i32;
        pub fn EmptyClipboard() -> i32;
        pub fn GetClipboardData(format: u32) -> isize;
        pub fn SetClipboardData(format: u32, mem: isize) -> isize;
        pub fn SetWindowPos(
            hwnd: isize,
            after: isize,
            x: i32,
            y: i32,
            cx: i32,
            cy: i32,
            flags: u32,
        ) -> i32;
    }
}

/// 窗口句柄的裸整数值。`.0` 在不同 windows crate 版本里是 isize 或裸指针，
/// `as isize` 对两者都成立，这样就不用跟着它的大版本改代码。
#[cfg(windows)]
fn raw_hwnd(window: &WebviewWindow) -> Option<isize> {
    window.hwnd().ok().map(|h| h.0 as isize)
}

#[cfg(windows)]
fn commit_frame(hwnd: isize) {
    unsafe {
        ffi::SetWindowPos(
            hwnd,
            0,
            0,
            0,
            0,
            0,
            ffi::SWP_FRAMECHANGED
                | ffi::SWP_NOMOVE
                | ffi::SWP_NOSIZE
                | ffi::SWP_NOZORDER
                | ffi::SWP_NOACTIVATE,
        );
    }
}

/// 让无边框窗口走系统圆角，和 Win11 上其它窗口保持一致的 8px 半径。
#[cfg(windows)]
pub fn round_corners(window: &WebviewWindow) {
    set_dword_attr(window, ffi::DWMWA_WINDOW_CORNER_PREFERENCE, ffi::DWMWCP_ROUND);
}

#[cfg(not(windows))]
pub fn round_corners(_window: &WebviewWindow) {}

/// 明确要求不圆角。Win11 默认会把窗口四角裁圆，那会把折角图标的直角削掉——
/// 它必须严丝合缝地顶在屏幕角上才成立。
#[cfg(windows)]
pub fn square_corners(window: &WebviewWindow) {
    set_dword_attr(
        window,
        ffi::DWMWA_WINDOW_CORNER_PREFERENCE,
        ffi::DWMWCP_DONOTROUND,
    );
}

#[cfg(not(windows))]
pub fn square_corners(_window: &WebviewWindow) {}

/// 把窗口重新插到置顶层的最前面。
///
/// 不能用 Tauri 的 `set_always_on_top(true)`——窗口本来就是置顶的，tao 看到
/// 状态没变就直接跳过，压根不会重排 z-order。这里直接下 SetWindowPos。
#[cfg(windows)]
pub fn raise_to_top(window: &WebviewWindow) {
    let Some(hwnd) = raw_hwnd(window) else { return };
    unsafe {
        ffi::SetWindowPos(
            hwnd,
            ffi::HWND_TOPMOST,
            0,
            0,
            0,
            0,
            ffi::SWP_NOMOVE | ffi::SWP_NOSIZE | ffi::SWP_NOACTIVATE,
        );
    }
}

#[cfg(not(windows))]
pub fn raise_to_top(_window: &WebviewWindow) {}

/// 窗口在 (x, y) 这个点上是不是被别人压住了。
///
/// 置顶不是一劳永逸的：`WS_EX_TOPMOST` 只保证**排进去的那一刻**在非置顶窗口前面，
/// 此后系统不再持续纠正，置顶层内部也仍然是谁后抬谁在上。于是一个置顶窗口完全可能
/// 滞留在别人后面——实测折角曾整块被同为 Tauri 的置顶全屏窗口 Localless 盖住，而
/// 折角只在面板显示时抬过一次，此后再没人把它抬回来。一旦沉下去鼠标就够不到卷角，
/// `peek_panel` 一次都不会被调用。窗口还在、位置也对，只是事件到不了。
///
/// 判断走 `WindowFromPoint` 而不是比 z-order：真正要问的就是「这一点上的鼠标事件
/// 会发给谁」，而那正是这个 API 的答案。命中的多半是 WebView2 的子窗口，所以要
/// `GetAncestor` 回到顶层再比。
///
/// **任务栏也算压住。** 折角贴在屏幕角上，任务栏自动隐藏时滑出来那条带子正好盖住
/// 它；而实测常态是折角排在露出来的任务栏**前面**（探点仍归折角），所以抬到它前面
/// 是恢复常态。反过来把任务栏列为例外，只会让折角在偶尔真被它盖住时彻底卡死。
#[cfg(windows)]
pub fn buried_at(window: &WebviewWindow, x: i32, y: i32) -> bool {
    let Some(hwnd) = raw_hwnd(window) else {
        return false;
    };
    unsafe {
        let hit = ffi::WindowFromPoint(ffi::Point { x, y });
        if hit == 0 {
            return false;
        }
        let root = ffi::GetAncestor(hit, ffi::GA_ROOT);
        root != 0 && root != hwnd
    }
}

#[cfg(not(windows))]
pub fn buried_at(_window: &WebviewWindow, _x: i32, _y: i32) -> bool {
    false
}

/// 这个窗口此刻是不是系统的前台窗口。
///
/// 不用 tao 的 `is_focused()`：那是它自己记的一份，只在收到 WM_NCACTIVATE /
/// WM_KILLFOCUS 时才翻。遇到会话切换——远程桌面连上又断开、锁屏再解锁——前台
/// 被系统直接换走，这两条消息可能根本不来，它那份就一直停在「有焦点」。实测连过
/// 手机远程之后，点了别处面板也不收，得放大再缩回一次（放大会 set_focus，把
/// 那份状态重新同步一遍）才好。直接问系统就没有同步不同步的问题。
///
/// 前台窗口总是顶层窗口；焦点落在 WebView2 的子窗口里时也一样，所以不用 GetAncestor。
#[cfg(windows)]
pub fn is_foreground(window: &WebviewWindow) -> bool {
    let Some(hwnd) = raw_hwnd(window) else {
        return false;
    };
    unsafe { ffi::GetForegroundWindow() == hwnd }
}

#[cfg(not(windows))]
pub fn is_foreground(window: &WebviewWindow) -> bool {
    window.is_focused().unwrap_or(false)
}

/// 关掉 DWM 画的那圈窗口边线。圆角窗口在 Win11 上默认带 1px 描边，
/// 套在折角图标外面就是一个很明显的方框。
#[cfg(windows)]
pub fn no_border(window: &WebviewWindow) {
    set_dword_attr(window, ffi::DWMWA_BORDER_COLOR, ffi::DWM_COLOR_NONE);
}

#[cfg(not(windows))]
pub fn no_border(_window: &WebviewWindow) {}

#[cfg(windows)]
fn set_dword_attr(window: &WebviewWindow, attr: u32, value: u32) {
    let Some(hwnd) = raw_hwnd(window) else { return };
    unsafe {
        ffi::DwmSetWindowAttribute(
            hwnd,
            attr,
            &value as *const u32 as *const core::ffi::c_void,
            std::mem::size_of::<u32>() as u32,
        );
    }
}

/// 从 Alt+Tab 里摘出去。skipTaskbar 在 tao 里是靠 ITaskbarList::DeleteTab 实现的，
/// 只去掉任务栏按钮，窗口样式上仍带着 WS_EX_APPWINDOW，照样会出现在切换列表里。
#[cfg(windows)]
pub fn hide_from_alt_tab(window: &WebviewWindow) {
    let Some(hwnd) = raw_hwnd(window) else { return };
    unsafe {
        let ex = ffi::GetWindowLongW(hwnd, ffi::GWL_EXSTYLE);
        let next = (ex & !ffi::WS_EX_APPWINDOW) | ffi::WS_EX_TOOLWINDOW;
        if next != ex {
            ffi::SetWindowLongW(hwnd, ffi::GWL_EXSTYLE, next);
            commit_frame(hwnd);
        }
    }
}

#[cfg(not(windows))]
pub fn hide_from_alt_tab(_window: &WebviewWindow) {}

/// 摘掉标题栏相关的窗口样式。`decorations: false` 只是让 tao 在 WM_NCCALCSIZE
/// 里把边框吃掉，WS_CAPTION|WS_SYSMENU 仍然在，于是系统会按“得放得下标题栏按钮”
/// 强制一个最小宽度（200% 缩放下约 262px），56px 的折角图标根本做不出来。
#[cfg(windows)]
pub fn strip_caption(window: &WebviewWindow) {
    let Some(hwnd) = raw_hwnd(window) else { return };
    unsafe {
        let style = ffi::GetWindowLongW(hwnd, ffi::GWL_STYLE);
        let next = style
            & !(ffi::WS_CAPTION | ffi::WS_SYSMENU | ffi::WS_MINIMIZEBOX | ffi::WS_MAXIMIZEBOX);
        if next != style {
            ffi::SetWindowLongW(hwnd, ffi::GWL_STYLE, next);
            commit_frame(hwnd);
        }
    }
}

#[cfg(not(windows))]
pub fn strip_caption(_window: &WebviewWindow) {}

#[cfg(windows)]
pub fn cursor_pos() -> Option<(i32, i32)> {
    let mut p = ffi::Point::default();
    let ok = unsafe { ffi::GetCursorPos(&mut p) } != 0;
    ok.then_some((p.x, p.y))
}

#[cfg(not(windows))]
pub fn cursor_pos() -> Option<(i32, i32)> {
    None
}

/// 窗口所在显示器的缩放倍率，直接问系统要。
///
/// 不用 tao 缓存的 `scale_factor()`：那个值是它处理 WM_DPICHANGED 时才更新的，
/// 而最要紧的恰恰是改分辨率那一瞬间——消息还没走到、或者窗口这会儿正好被甩到
/// 屏幕外压根收不到，缓存里就还是旧倍率。折角的边长是按倍率算的，倍率错了边长
/// 就错一倍，而尺寸一旦长期对不上，`snap_orb` 里那轮尺寸校正就永远收不了尾。
#[cfg(windows)]
pub fn dpi_scale(window: &WebviewWindow) -> Option<f64> {
    let hwnd = raw_hwnd(window)?;
    let dpi = unsafe { ffi::GetDpiForWindow(hwnd) };
    // 失败返回 0。这里不替它兜底成 96——猜错的代价是把折角改成一半或两倍大，
    // 交给调用方决定「拿不到就别动尺寸」。
    (dpi > 0).then(|| f64::from(dpi) / 96.0)
}

#[cfg(not(windows))]
pub fn dpi_scale(_window: &WebviewWindow) -> Option<f64> {
    None
}

/// 当前这套显示配置的指纹，形如 `3840x2160@192`。面板尺寸按它分开记。
///
/// 用显示器的**整块矩形**而不是工作区：任务栏改自动隐藏、挪到别的边、多排一行，
/// 都会让工作区变化，可那并不是「换了块屏」，不该另起一份尺寸记忆、更不该把已经
/// 记好的那份挤掉。
///
/// 带上 DPI 是因为同样的分辨率配不同缩放是两种完全不同的观感——1920x1080 在 100%
/// 下能摊开一大片，在 150% 下只装得下三分之二，记成同一份等于没分。
///
/// 走 `MonitorFromWindow` 而不是拿窗口中心点去 `MonitorFromPoint`：改分辨率时窗口
/// 可能整个被甩到桌面之外，那时候算出来的中心点不在任何显示器里，还得另外兜底。
///
/// **返回值不能直接就信**。窗口的 DPI 上下文和它所在显示器的坐标空间会短暂对不上，
/// 这时候量出来的是个根本不存在的组合——实测启动那一瞬间，唯一那块 3840x2160@192
/// 的屏被报成了 `1920x1080@168`（矩形按旧倍率虚拟化过，DPI 又是另一份）。所以调用方
/// （`sync_layout`）要连着读到两次一样的才认。
#[cfg(windows)]
pub fn display_key(window: &WebviewWindow) -> Option<String> {
    let hwnd = raw_hwnd(window)?;
    let dpi = unsafe { ffi::GetDpiForWindow(hwnd) };
    if dpi == 0 {
        return None;
    }
    let monitor = unsafe { ffi::MonitorFromWindow(hwnd, ffi::MONITOR_DEFAULTTONEAREST) };
    if monitor == 0 {
        return None;
    }
    let mut info = ffi::MonitorInfo {
        cb_size: std::mem::size_of::<ffi::MonitorInfo>() as u32,
        ..Default::default()
    };
    if unsafe { ffi::GetMonitorInfoW(monitor, &mut info) } == 0 {
        return None;
    }
    let m = info.rc_monitor;
    let (w, h) = (m.right - m.left, m.bottom - m.top);
    if w <= 0 || h <= 0 {
        return None;
    }
    Some(format!("{w}x{h}@{dpi}"))
}

#[cfg(not(windows))]
pub fn display_key(_window: &WebviewWindow) -> Option<String> {
    None
}

/// 包含给定点的显示器工作区（已扣掉任务栏）。折角默认停在右下角，
/// 正好是任务栏所在处，所以这里必须用 rcWork 而不是整块屏幕。
#[cfg(windows)]
pub fn work_area_at(x: i32, y: i32) -> Option<Area> {
    let point = ffi::Point { x, y };
    let monitor = unsafe { ffi::MonitorFromPoint(point, ffi::MONITOR_DEFAULTTONEAREST) };
    if monitor == 0 {
        return None;
    }
    let mut info = ffi::MonitorInfo {
        cb_size: std::mem::size_of::<ffi::MonitorInfo>() as u32,
        ..Default::default()
    };
    if unsafe { ffi::GetMonitorInfoW(monitor, &mut info) } == 0 {
        return None;
    }
    let w = info.rc_work;
    Some(Area {
        left: w.left,
        top: w.top,
        right: w.right,
        bottom: w.bottom,
    })
}

#[cfg(not(windows))]
pub fn work_area_at(_x: i32, _y: i32) -> Option<Area> {
    None
}

/// 抢占单实例锁：拿到返回 true，已经有一个在跑返回 false。
///
/// 开机自启和手动点开可能同时发生（比如自启迟迟没被处理、用户先去开始菜单点了一下），
/// 两个实例会各自贴在角上、各建一个托盘图标，并且轮流把对方写的存档覆盖掉。
///
/// 句柄故意不释放——进程活多久就持有多久，退出时由系统回收。
#[cfg(windows)]
pub fn claim_single_instance() -> bool {
    // Local\ 前缀限定在当前登录会话内，不同用户各自能开自己的一份。
    let name: Vec<u16> = "Local\\com.shawn.hovernote.instance"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let handle = ffi::CreateMutexW(std::ptr::null(), 1, name.as_ptr());
        // 撞名时 CreateMutexW 仍然返回有效句柄，必须查 GetLastError 才知道是不是第一个。
        handle != 0 && ffi::GetLastError() != ffi::ERROR_ALREADY_EXISTS
    }
}

#[cfg(not(windows))]
pub fn claim_single_instance() -> bool {
    true
}

/// 本地时间的 `YYYYMMDD-HHMMSS`，给回收站的文件名用。
///
/// 用本地时间而不是 UTC：这个字符串是给人看的——在文件夹里一眼认出"我是什么时候
/// 删的这条"，差八个时区就没用了。项目没有 chrono/time 依赖，而 `GetLocalTime`
/// 直接给出已经换算好的年月日时分秒，比自己从 epoch 推算再处理时区省事得多。
#[cfg(windows)]
pub fn local_stamp() -> String {
    let mut t = ffi::SystemTime::default();
    unsafe { ffi::GetLocalTime(&mut t) };
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        t.year, t.month, t.day, t.hour, t.minute, t.second
    )
}

#[cfg(not(windows))]
pub fn local_stamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}

/// 协议白名单。前端渲染 Markdown 时也判一次，但那一次只保证「渲染出来的 `<a>`
/// 是干净的」；这个命令从 IPC 进来，前端那道拦不住它，所以这里必须独立判一遍。
fn allowed_scheme(url: &str) -> bool {
    let Some((scheme, rest)) = url.split_once(':') else {
        return false;
    };
    !rest.is_empty() && matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https" | "mailto")
}

/// 用系统默认程序打开链接。面板本身就是个 webview，让它跟着 `<a>` 跳走，
/// 应用就没了——笔记界面会变成一个网页，且没有地址栏能回来。
#[cfg(windows)]
pub fn open_external(url: &str) -> bool {
    if !allowed_scheme(url) {
        return false;
    }
    let verb: Vec<u16> = "open".encode_utf16().chain(std::iter::once(0)).collect();
    let target: Vec<u16> = url.encode_utf16().chain(std::iter::once(0)).collect();

    // ShellExecuteW 要走 shell 的关联查找，可能阻塞几百毫秒（冷启动浏览器时更久），
    // 不能占着 Tauri 的命令线程；而且它依赖 COM，得在自己这条线程上初始化。
    std::thread::spawn(move || unsafe {
        ffi::CoInitializeEx(std::ptr::null_mut(), ffi::COINIT_APARTMENTTHREADED);
        ffi::ShellExecuteW(
            0,
            verb.as_ptr(),
            target.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            ffi::SW_SHOWNORMAL,
        );
    });
    true
}

#[cfg(not(windows))]
pub fn open_external(url: &str) -> bool {
    allowed_scheme(url)
}

/// 打开剪贴板。别的程序（剪贴板历史、远程桌面、各种同步工具）正拿着它的时候会
/// 失败，而它们通常几毫秒就放手——重试几次，比让右键粘贴偶尔「按了没反应」强。
#[cfg(windows)]
fn open_clipboard(owner: isize) -> bool {
    for _ in 0..10 {
        if unsafe { ffi::OpenClipboard(owner) } != 0 {
            return true;
        }
        unsafe { ffi::Sleep(10) };
    }
    false
}

/// 读剪贴板里的文本。没有文本（比如复制的是一张图）或打不开时返回 None。
///
/// 不让网页自己用 `navigator.clipboard.readText()`：WebView2 读剪贴板要过权限，
/// 而放行它的开关 Tauri 只在代码里建窗口时给，面板是从配置文件建的，够不着。
#[cfg(windows)]
pub fn clipboard_read(window: &WebviewWindow) -> Option<String> {
    let hwnd = raw_hwnd(window).unwrap_or(0);
    if !open_clipboard(hwnd) {
        return None;
    }
    let text = unsafe {
        let mem = ffi::GetClipboardData(ffi::CF_UNICODETEXT);
        let ptr = if mem == 0 { std::ptr::null_mut() } else { ffi::GlobalLock(mem) };
        if ptr.is_null() {
            None
        } else {
            // 按块大小封顶，不全信结尾那个 NUL——别的程序塞进来的数据不一定守规矩。
            let cap = ffi::GlobalSize(mem) / 2;
            let wide = std::slice::from_raw_parts(ptr as *const u16, cap);
            let len = wide.iter().position(|&c| c == 0).unwrap_or(cap);
            let s = String::from_utf16_lossy(&wide[..len]);
            ffi::GlobalUnlock(mem);
            Some(s)
        }
    };
    unsafe { ffi::CloseClipboard() };
    text
}

#[cfg(not(windows))]
pub fn clipboard_read(_window: &WebviewWindow) -> Option<String> {
    None
}

/// 把文本写进剪贴板。
///
/// owner 必须给真窗口：用 NULL 打开的话，EmptyClipboard 会把所有者置空，
/// 文档写明这之后 SetClipboardData 会失败。
#[cfg(windows)]
pub fn clipboard_write(window: &WebviewWindow, text: &str) -> bool {
    let Some(hwnd) = raw_hwnd(window) else {
        return false;
    };
    // 剪贴板上的文本约定是 CRLF；只给 LF 的话，贴进记事本这类老程序会连成一行。
    let wide: Vec<u16> = text
        .replace("\r\n", "\n")
        .replace('\n', "\r\n")
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    if !open_clipboard(hwnd) {
        return false;
    }
    let ok = unsafe {
        ffi::EmptyClipboard();
        let mem = ffi::GlobalAlloc(ffi::GMEM_MOVEABLE, wide.len() * 2);
        let ptr = if mem == 0 { std::ptr::null_mut() } else { ffi::GlobalLock(mem) };
        if ptr.is_null() {
            if mem != 0 {
                ffi::GlobalFree(mem);
            }
            false
        } else {
            std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr as *mut u16, wide.len());
            ffi::GlobalUnlock(mem);
            // 成功之后这块内存归系统，不能再 free；失败了还是我们的。
            if ffi::SetClipboardData(ffi::CF_UNICODETEXT, mem) == 0 {
                ffi::GlobalFree(mem);
                false
            } else {
                true
            }
        }
    };
    unsafe { ffi::CloseClipboard() };
    ok
}

#[cfg(not(windows))]
pub fn clipboard_write(_window: &WebviewWindow, _text: &str) -> bool {
    false
}
