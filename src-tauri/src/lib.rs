mod platform;
mod state;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State, WebviewWindow,
    WindowEvent,
};

use platform::Area;
use state::{Corner, Group, Note, Persisted};

/// 折角图标的边长（逻辑像素），与 tauri.conf.json 保持一致。
const ORB_SIZE: f64 = 28.0;
/// 判定「光标还在窗口上」时向外放宽的容差，避免边缘 1px 抖动导致误隐藏。
const HOVER_SLACK: f64 = 4.0;

/// 设 HOVERNOTE_TRACE=1 打开窗口显隐的诊断日志。这类置顶小窗的显隐问题
/// 从外部观察极不可靠（截屏抓不到合成层、光标一动状态就变），必须让程序自报。
///
/// 值可以是任意非空字符串（走默认路径），也可以直接给一个文件路径。
fn trace_sink() -> Option<&'static std::path::PathBuf> {
    static SINK: std::sync::OnceLock<Option<std::path::PathBuf>> = std::sync::OnceLock::new();
    SINK.get_or_init(|| {
        let raw = std::env::var_os("HOVERNOTE_TRACE")?;
        let s = raw.to_string_lossy().into_owned();
        // release 是 windows 子系统，没有控制台，stderr 直接进黑洞——诊断必须落文件。
        Some(if s.contains(['\\', '/']) {
            std::path::PathBuf::from(s)
        } else {
            std::env::temp_dir().join("hovernote-trace.log")
        })
    })
    .as_ref()
}

fn trace_write(args: std::fmt::Arguments<'_>) {
    let Some(path) = trace_sink() else { return };
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "[hn] {args}");
    }
}

macro_rules! tr {
    ($($arg:tt)*) => {
        crate::trace_write(format_args!($($arg)*));
    };
}

pub struct Shared {
    data: Mutex<Persisted>,
    /// 是否处于居中放大态。放大时不自动隐藏。
    expanded: AtomicBool,
    /// 面板显示后是否被点过 / 打过字。没交互过 = 纯预览，鼠标一离开就收。
    interacted: AtomicBool,
    /// 在这个时刻之前不执行自动隐藏，给刚弹出的面板一点缓冲。
    grace: Mutex<Instant>,
    /// 在这个时刻之前忽略 resize 回调——那是程序自己改的尺寸，不是用户拖的。
    resize_guard: Mutex<Instant>,
    /// 当前显示配置的指纹（见 `platform::display_key`）。None = 还没认过。
    ///
    /// 记尺寸时**一律用这个缓存的 key**，不临场再问一遍系统：改分辨率的那一瞬间
    /// 系统已经在报新配置了，而这时手上的尺寸还是旧屏那一份，临场问就会把旧屏的
    /// 尺寸写进新屏的记忆里，正好盖掉要用的那份。
    display: Mutex<Option<String>>,
    /// 上一次**观察到**的指纹，用来做「连着两次一样才认」的去抖。见 `sync_layout`。
    display_seen: Mutex<Option<String>>,
    /// 存档读不出来时置位，此后**一律不写盘**。
    ///
    /// 读失败（文件被杀毒/备份程序锁住、瞬时 IO 错误、JSON 坏掉）时内存里是一份
    /// 空数据；要是照常保存，就会把磁盘上那份好笔记覆盖成空的，且不可逆。宁可这次
    /// 运行不记住任何改动，也不能拿用户的笔记冒险。
    read_only: AtomicBool,
}

impl Shared {
    fn new(loaded: &state::Loaded) -> Self {
        let now = Instant::now();
        Self {
            data: Mutex::new(loaded.data().clone()),
            expanded: AtomicBool::new(false),
            interacted: AtomicBool::new(false),
            grace: Mutex::new(now),
            resize_guard: Mutex::new(now),
            display: Mutex::new(None),
            display_seen: Mutex::new(None),
            read_only: AtomicBool::new(loaded.is_broken()),
        }
    }

    fn is_read_only(&self) -> bool {
        self.read_only.load(Ordering::Relaxed)
    }

    fn hold_grace(&self, ms: u64) {
        if let Ok(mut g) = self.grace.lock() {
            *g = Instant::now() + Duration::from_millis(ms);
        }
    }

    fn hold_resize(&self, ms: u64) {
        if let Ok(mut g) = self.resize_guard.lock() {
            *g = Instant::now() + Duration::from_millis(ms);
        }
    }
}

// ---------------------------------------------------------------- 几何工具

fn orb_of(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window("orb")
}

fn panel_of(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window("panel")
}

fn scale_of(win: &WebviewWindow) -> f64 {
    win.scale_factor().unwrap_or(1.0)
}

/// 窗口所在显示器的缩放，**先问系统**再退到 tao 缓存的那份。
///
/// 凡是拿缩放去换算「要存下来的」或「要摆出去的」尺寸，都得走这个。理由见
/// `platform::dpi_scale`：tao 那份缓存只在它处理完 WM_DPICHANGED 之后才更新，
/// 而改分辨率那一瞬间恰恰是最需要准确值的时候。
fn live_scale(win: &WebviewWindow) -> f64 {
    platform::dpi_scale(win).unwrap_or_else(|| scale_of(win))
}

/// 窗口当前的逻辑尺寸。缩放取不到就返回 None，让调用方**别记**——
/// 宁可这一次不更新，也不能把错的倍率换算出来的数字写进存档：那是永久的，
/// 下次切回这块屏就是照着错的还原。
fn logical_size_of(win: &WebviewWindow) -> Option<(u32, u32)> {
    let scale = platform::dpi_scale(win).or_else(|| win.scale_factor().ok())?;
    if scale <= 0.0 {
        return None;
    }
    let size = win.outer_size().ok()?;
    let w = (f64::from(size.width) / scale).round() as u32;
    let h = (f64::from(size.height) / scale).round() as u32;
    (w > 0 && h > 0).then_some((w, h))
}

/// 折角该有的边长（物理像素）。
///
/// 拿不到缩放就返回 None，让调用方**别动尺寸**——绝不退回 1.0 猜一个。在 200%
/// 的屏上猜错就是把折角改成一半大，而且改完还会引出下一轮改回去，来回折腾。
/// 先问系统（`GetDpiForWindow`），它答不上来再退到 tao 缓存的那份。
fn orb_side(orb: &WebviewWindow) -> Option<i32> {
    let scale = platform::dpi_scale(orb).or_else(|| orb.scale_factor().ok())?;
    let side = (ORB_SIZE * scale).round() as i32;
    (side > 0).then_some(side)
}

/// 窗口的物理矩形。
fn rect_of(win: &WebviewWindow) -> Option<Area> {
    let p = win.outer_position().ok()?;
    let s = win.outer_size().ok()?;
    Some(Area {
        left: p.x,
        top: p.y,
        right: p.x + s.width as i32,
        bottom: p.y + s.height as i32,
    })
}

fn area_around(win: &WebviewWindow) -> Area {
    let r = rect_of(win);
    let (cx, cy) = match r {
        Some(r) => ((r.left + r.right) / 2, (r.top + r.bottom) / 2),
        None => (0, 0),
    };
    platform::work_area_at(cx, cy).unwrap_or(Area {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1040,
    })
}

fn clamp(v: i32, lo: i32, hi: i32) -> i32 {
    if lo > hi {
        lo
    } else {
        v.max(lo).min(hi)
    }
}

/// 工作区里离给定点最近的那个角。
fn nearest_corner(area: &Area, x: i32, y: i32) -> Corner {
    let left = x < area.left + area.width() / 2;
    let top = y < area.top + area.height() / 2;
    match (top, left) {
        (true, true) => Corner::TopLeft,
        (true, false) => Corner::TopRight,
        (false, true) => Corner::BottomLeft,
        (false, false) => Corner::BottomRight,
    }
}

/// 把折角图标严丝合缝地摆进工作区的某个角——不留任何边距，它就是屏幕的角。
///
/// `hint` 是上次记下的位置，只用来决定「是哪一块显示器」——光靠 corner
/// 在多显示器下没法区分，否则每次启动都会跳回主屏。
fn place_orb(app: &AppHandle, corner: Corner, hint: Option<(i32, i32)>) {
    let Some(orb) = orb_of(app) else { return };
    let Ok(size) = orb.outer_size() else { return };
    let area = hint
        .and_then(|(x, y)| platform::work_area_at(x, y))
        .unwrap_or_else(|| area_around(&orb));
    let (w, h) = (size.width as i32, size.height as i32);

    let x = if corner.is_left() {
        area.left
    } else {
        area.right - w
    };
    let y = if corner.is_top() {
        area.top
    } else {
        area.bottom - h
    };
    let _ = orb.set_position(PhysicalPosition::new(x, y));
    // 摆过去之后再校一次：跨到不同缩放的显示器上时，系统是在移动**之后**才重新
    // 缩放窗口的，按移动前的尺寸算出来的位置会留出一条缝。
    snap_orb(app);
}

/// 折角当前所在显示器的工作区。用实时位置而不是存档里的坐标——工作区随时会变。
fn area_under_orb(orb: &WebviewWindow) -> Option<(Area, Area)> {
    let rect = rect_of(orb)?;
    let cx = (rect.left + rect.right) / 2;
    let cy = (rect.top + rect.bottom) / 2;
    Some((rect, platform::work_area_at(cx, cy)?))
}

/// 把折角校回它该在的角上——贴死工作区，尺寸按所在显示器的缩放算。
/// 返回是否真的动过。
///
/// 尺寸先校，因为右下角的位置是拿尺寸倒推的。但**位置每一轮都要算**，不能等尺寸
/// 先对上：改分辨率会连带改 DPI，系统在 WM_DPICHANGED 里会按它自己的建议矩形回改
/// 窗口，和这里的 set_size 顶牛；只要有一轮收不干净，几次循环就全耗在尺寸上，
/// 位置的代码一次都跑不到，而返回值还是 true——外面看着像在正常工作，折角却一直
/// 歪着，每 400ms 白转一次。位置一律按**当前实际**尺寸算，所以哪怕尺寸暂时差几个
/// 像素，它也还是贴在边上的；尺寸下一轮再收。
fn snap_orb(app: &AppHandle) -> bool {
    let Some(orb) = orb_of(app) else { return false };
    let Some(shared) = app.try_state::<Shared>() else {
        return false;
    };
    let Ok(corner) = shared.data.lock().map(|d| d.corner) else {
        return false;
    };

    let mut changed = false;
    let mut landed = None;
    for _ in 0..4 {
        let Some((rect, area)) = area_under_orb(&orb) else {
            break;
        };

        // 留 1px 容差：这里不追求尺寸精确，只是不能差到让贴边看得出来。
        if let Some(side) = orb_side(&orb) {
            if (rect.width() - side).abs() > 1 || (rect.height() - side).abs() > 1 {
                let _ = orb.set_size(PhysicalSize::new(side as u32, side as u32));
                changed = true;
            }
        }

        // 重新量一次：尺寸可能刚改过，而位置是按尺寸算的。
        let Some(rect) = rect_of(&orb) else { break };
        let x = if corner.is_left() {
            area.left
        } else {
            area.right - rect.width()
        };
        let y = if corner.is_top() {
            area.top
        } else {
            area.bottom - rect.height()
        };
        if rect.left == x && rect.top == y {
            break; // 已经贴死了
        }
        let _ = orb.set_position(PhysicalPosition::new(x, y));
        landed = Some((x, y));
        changed = true;
        tr!("snap_orb {corner:?}: {},{} -> {x},{y}", rect.left, rect.top);
    }

    // 校正后的落点也要记进「上次在哪」。这个值只用来在下次启动时认显示器，原先
    // 只有用户手动拖动才会写，于是改完分辨率（尤其是插拔外接屏这种最常见的成因）
    // 之后它就一直停在旧坐标上，下次启动便可能把折角摆回一块已经不存在的区域附近。
    // 只改内存里的，不在这儿落盘——这个函数每 400ms 跑一次，而存盘要写临时文件、
    // fsync、复制备份；退出和每次存笔记都会带着它一起写下去，够了。
    if let Some(xy) = landed {
        if let Ok(mut d) = shared.data.lock() {
            d.orb = Some(xy);
        }
    }
    changed
}

/// 面板同样贴死在那个角上，让折角正好压在它自己的角上。
fn anchor_panel(app: &AppHandle, shared: &Shared) {
    if shared.expanded.load(Ordering::Relaxed) {
        return;
    }
    let (Some(orb), Some(panel)) = (orb_of(app), panel_of(app)) else {
        return;
    };
    let corner = shared
        .data
        .lock()
        .map(|d| d.corner)
        .unwrap_or(Corner::BottomRight);

    let Ok(psize) = panel.outer_size() else { return };
    let (pw, ph) = (psize.width as i32, psize.height as i32);
    let area = area_around(&orb);

    let x = if corner.is_left() {
        area.left
    } else {
        area.right - pw
    };
    let y = if corner.is_top() {
        area.top
    } else {
        area.bottom - ph
    };
    let _ = panel.set_position(PhysicalPosition::new(x, y));
}

/// 把折角重新抬到最上层。面板和折角都是置顶窗口，面板一显示就会插到
/// 置顶层的最前面把折角盖住——而折角必须始终压在界面上，它是那道折痕。
fn raise_orb(app: &AppHandle) {
    if let Some(orb) = orb_of(app) {
        platform::raise_to_top(&orb);
    }
}

/// 放大态该用的位置和尺寸（物理像素）。
///
/// 存的是逻辑尺寸，按当前缩放换算，并夹回当前工作区——换了分辨率或者拔掉外接屏
/// 之后，旧的坐标和尺寸可能整个落在屏幕外。
fn expanded_geometry(shared: &Shared, panel: &WebviewWindow) -> Option<(i32, i32, i32, i32)> {
    let area = area_around(panel);
    let s = live_scale(panel);
    let min_w = (300.0 * s) as i32;
    let min_h = (240.0 * s) as i32;
    let (ew, eh, ex, ey) = shared
        .data
        .lock()
        .ok()
        .map(|d| (d.exp_w, d.exp_h, d.exp_x, d.exp_y))?;

    Some(if ew > 0 && eh > 0 {
        // 还原上次在这套显示配置下放大的样子。
        let w = clamp((ew as f64 * s).round() as i32, min_w, area.width());
        let h = clamp((eh as f64 * s).round() as i32, min_h, area.height());
        (
            w,
            h,
            clamp(ex, area.left, area.right - w),
            clamp(ey, area.top, area.bottom - h),
        )
    } else {
        // 第一次放大：按工作区比例取一个舒服的尺寸，居中。
        let w = clamp(
            ((area.width() as f64 * 0.66).min(1120.0 * s)).round() as i32,
            min_w,
            area.width(),
        );
        let h = clamp(
            ((area.height() as f64 * 0.74).min(860.0 * s)).round() as i32,
            min_h,
            area.height(),
        );
        (
            w,
            h,
            area.left + (area.width() - w) / 2,
            area.top + (area.height() - h) / 2,
        )
    })
}

/// 把台面上记着的几何真的落到面板窗口上。
fn apply_layout(app: &AppHandle, shared: &Shared) {
    let Some(panel) = panel_of(app) else { return };
    // 接下来这几下 set_size 是程序自己改的，不能再被当成用户拖的记回去。
    shared.hold_resize(700);

    if shared.expanded.load(Ordering::Relaxed) {
        if let Some((w, h, x, y)) = expanded_geometry(shared, &panel) {
            let _ = panel.set_size(PhysicalSize::new(w as u32, h as u32));
            let _ = panel.set_position(PhysicalPosition::new(x, y));
        }
        return;
    }

    let Ok((w, h)) = shared.data.lock().map(|d| (d.panel_w, d.panel_h)) else {
        return;
    };
    // 贴角态跟着折角走，所以缩放和工作区都按**折角那块屏**算。启动时面板还停在
    // 系统给的默认位置上（多半是主屏），拿它自己去问就会问到另一块屏的答案。
    let anchor = orb_of(app).unwrap_or_else(|| panel.clone());
    let area = area_around(&anchor);
    let s = live_scale(&anchor);
    // 夹回工作区：从大屏换到小屏时，原来那个高度可能整个超出屏幕。夹的只是
    // **这次摆出来的尺寸**，存档里那份原样不动——切回大屏还要照原样还原。
    let pw = clamp((w as f64 * s).round() as i32, 1, area.width());
    let ph = clamp((h as f64 * s).round() as i32, 1, area.height());
    let _ = panel.set_size(PhysicalSize::new(pw as u32, ph as u32));
    anchor_panel(app, shared);
}

/// `sync_layout` 的结论。
#[derive(Clone, Copy, PartialEq, Eq)]
enum Display {
    /// 和认定的那套一致，一切照常。
    Same,
    /// 读数和认定的不一样，但还没连着读到两次同样的——正在变，这一轮什么都别记。
    InFlux,
    /// 换定了，新配置那套几何已经搬上台面并摆好。
    Switched,
}

/// 认一下现在是哪套显示配置；换了就把那套配置下记着的几何搬出来用。
///
/// 「换屏了」这件事必须**赶在记录尺寸之前**判出来。改分辨率时系统会在 WM_DPICHANGED
/// 里按比例把窗口重排一遍，那次重排会顺着 `onResized` 走到 `panel_geometry`；要是照常
/// 记下来，就等于拿旧屏的尺寸把新屏上次调好的那份盖掉，再切回去也找不回来了。所以
/// `panel_geometry` 的头一件事就是调它，只要不是 `Same` 就直接返回，这一次不记。
///
/// 光靠 400ms 的轮询不够——轮询和 resize 回调谁先到没有保证，而只要记录先跑一步，
/// 那份记忆就已经没了。两条路都先过这里，谁先到都是「换记忆」而不是「记尺寸」。
///
/// 换定之前要连着读到两次一样的读数。窗口的 DPI 上下文和它所在显示器的坐标空间
/// 会短暂对不上，那当口量出来的是个根本不存在的显示配置（实测启动瞬间，唯一那块
/// 3840x2160@192 的屏被报成 `1920x1080@168`）。认下去就会在表里留一套幽灵配置，
/// 更糟的是可能拿它那份尺寸去摆窗口。一闪而过的读数过不了这道去抖。
fn sync_layout(app: &AppHandle, shared: &Shared) -> Display {
    let Some(orb) = orb_of(app) else {
        return Display::Same;
    };
    let Some(key) = platform::display_key(&orb) else {
        return Display::Same;
    };

    let same = shared
        .display
        .lock()
        .map(|cur| cur.as_deref() == Some(key.as_str()))
        .unwrap_or(true);
    if same {
        return Display::Same;
    }

    let stable = match shared.display_seen.lock() {
        Ok(mut seen) => {
            let stable = seen.as_deref() == Some(key.as_str());
            if !stable {
                *seen = Some(key.clone());
            }
            stable
        }
        Err(_) => return Display::Same,
    };
    if !stable {
        tr!("display 读到 {key}，等下一轮确认");
        return Display::InFlux;
    }

    match shared.display.lock() {
        Ok(mut cur) => *cur = Some(key.clone()),
        Err(_) => return Display::Same,
    }

    let known = match shared.data.lock() {
        Ok(mut d) => {
            let known = d.adopt_layout(&key);
            if !known {
                // 头一回见这套配置：拿台面上现有的尺寸给它开个户。逻辑像素换块屏
                // 之后看着还是一样大，用上一块屏的值开局是合理的起点。
                d.stash_layout(&key);
            }
            known
        }
        Err(_) => return Display::Same,
    };
    tr!("display -> {key}（{}）", if known { "有记录" } else { "新配置" });

    apply_layout(app, shared);
    Display::Switched
}

fn show_panel(app: &AppHandle, shared: &Shared) {
    let Some(panel) = panel_of(app) else { return };
    anchor_panel(app, shared); // 先摆位再显示，避免看到窗口从旧位置跳过来
    shared.interacted.store(false, Ordering::Relaxed);
    shared.hold_grace(650);
    let _ = panel.show();
    let _ = panel.set_always_on_top(true);
    raise_orb(app);
    let _ = app.emit("hn:shown", ());
    tr!("show_panel rect={:?}", rect_of(&panel));
}

fn hide_panel_now(app: &AppHandle, shared: &Shared) {
    let Some(panel) = panel_of(app) else { return };
    shared.interacted.store(false, Ordering::Relaxed);
    let _ = panel.hide();
    tr!("hide_panel");
}

fn persist(app: &AppHandle, shared: &Shared) {
    // 存档没读出来时内存里是空数据，写下去就把磁盘上的好笔记抹了。
    if shared.is_read_only() {
        tr!("persist skipped: read-only (存档未能读取)");
        return;
    }
    // 放大态的几何是活的（拖动/缩放随时在改），落盘前先抓一次当前值，
    // 这样每一个存档点都不会漏掉它。
    if let Some(panel) = panel_of(app) {
        remember_expanded(shared, &panel);
    }
    if let Ok(d) = shared.data.lock() {
        state::save(app, &d);
    }
}

/// 记下面板当前尺寸（放大态下的临时尺寸不算数）。存逻辑像素，见 Persisted 的注释。
fn remember_panel_size(shared: &Shared, panel: &WebviewWindow) {
    if shared.expanded.load(Ordering::Relaxed) {
        return;
    }
    let Some((w, h)) = logical_size_of(panel) else {
        return;
    };
    let key = shared.display.lock().ok().and_then(|k| k.clone());
    if let Ok(mut d) = shared.data.lock() {
        d.panel_w = w;
        d.panel_h = h;
        if let Some(key) = key {
            d.stash_layout(&key);
        }
    }
}

// ------------------------------------------------------------------- 命令

#[tauri::command]
fn load_state(shared: State<'_, Shared>) -> Persisted {
    shared.data.lock().map(|d| d.clone()).unwrap_or_default()
}

#[tauri::command]
fn save_notes(
    app: AppHandle,
    shared: State<'_, Shared>,
    notes: Vec<Note>,
    groups: Vec<Group>,
    active: Option<String>,
) {
    // 只在锁里做交换，文件写在锁外——回收站要落盘，不该让别人等着。
    let removed = match shared.data.lock() {
        Ok(mut d) => {
            let removed = state::removed_notes(&d.notes, &notes);
            d.notes = notes;
            d.groups = groups;
            d.active = active;
            removed
        }
        Err(_) => return,
    };

    // 存档没读出来时内存里本就是空的，这时的"消失"不是删除，别往回收站里灌垃圾。
    if !shared.is_read_only() {
        for note in &removed {
            tr!("trash: {} ({} 字)", note.title, note.body.chars().count());
            state::trash(note);
        }
    }
    persist(&app, &shared);
}

/// 折角被按住准备拖动：先把面板收掉，拖起来才干净。
#[tauri::command]
fn orb_grab(app: AppHandle, shared: State<'_, Shared>) {
    if !shared.expanded.load(Ordering::Relaxed) {
        hide_panel_now(&app, &shared);
    }
    shared.hold_grace(400);
}

/// 拖动中：把图标搬到离光标最近的那个角。
///
/// 它不跟着鼠标自由移动——需求是这枚折角只能在四个角之间换位置，不能停在
/// 某条边上，也不能飘到屏幕中间。所以拖动的语义是「选一个角」，而不是「挪窗口」。
/// x/y 是光标的屏幕物理坐标。
#[tauri::command]
fn orb_drag_to(app: AppHandle, shared: State<'_, Shared>, x: i32, y: i32) {
    let Some(orb) = orb_of(&app) else { return };
    let area = platform::work_area_at(x, y).unwrap_or_else(|| area_around(&orb));
    let corner = nearest_corner(&area, x, y);

    let changed = match shared.data.lock() {
        Ok(mut d) if d.corner != corner => {
            d.corner = corner;
            true
        }
        _ => false,
    };
    if !changed {
        return;
    }
    place_orb(&app, corner, Some((x, y)));
    anchor_panel(&app, &shared);
    let _ = app.emit("hn:corner", corner);
    tr!("orb_drag_to {corner:?}");
}

/// 松手：把落定的角和位置存下来。
#[tauri::command]
fn orb_settle(app: AppHandle, shared: State<'_, Shared>) {
    let Some(orb) = orb_of(&app) else { return };
    if let Some(r) = rect_of(&orb) {
        if let Ok(mut d) = shared.data.lock() {
            d.orb = Some((r.left, r.top));
        }
    }
    persist(&app, &shared);
}

/// 鼠标浮到折角上就展开笔记；已经开着就什么都不做，免得反复重置隐藏计时。
#[tauri::command]
fn peek_panel(app: AppHandle, shared: State<'_, Shared>) {
    let Some(panel) = panel_of(&app) else { return };
    if panel.is_visible().unwrap_or(false) {
        return;
    }
    show_panel(&app, &shared);
}

#[tauri::command]
fn current_corner(shared: State<'_, Shared>) -> Corner {
    shared
        .data
        .lock()
        .map(|d| d.corner)
        .unwrap_or(Corner::BottomRight)
}

#[tauri::command]
fn toggle_panel(app: AppHandle, shared: State<'_, Shared>) {
    let Some(panel) = panel_of(&app) else { return };
    let visible = panel.is_visible().unwrap_or(false);
    tr!("toggle_panel visible={visible}");
    if visible {
        // 放大态下点折角，先缩回角落而不是直接消失。
        if shared.expanded.load(Ordering::Relaxed) {
            collapse(&app, &shared);
        } else {
            hide_panel_now(&app, &shared);
        }
    } else {
        show_panel(&app, &shared);
    }
}

#[tauri::command]
fn hide_panel(app: AppHandle, shared: State<'_, Shared>) {
    if shared.expanded.load(Ordering::Relaxed) {
        collapse(&app, &shared);
    }
    hide_panel_now(&app, &shared);
}

/// 面板被点击或输入过——从这一刻起它不再是「鼠标划过就收」的预览态。
#[tauri::command]
fn mark_interacted(shared: State<'_, Shared>) {
    shared.interacted.store(true, Ordering::Relaxed);
}

/// 打开 Markdown 预览里的链接。前端拦下 `<a>` 的默认跳转再调这个——面板自己
/// 就是个 webview，让它跳走就等于把笔记界面换成了一个没有地址栏的网页。
#[tauri::command]
fn open_url(url: String) {
    if !platform::open_external(&url) {
        tr!("open_url 拒绝: {url}");
    }
}

/// 记下放大态当前的位置和尺寸。放大后拖动和缩放都会改它，所以缩回前、
/// 拖完、缩放完、退出前都要调一次，否则这些调整下次放大就丢了。
fn remember_expanded(shared: &Shared, panel: &WebviewWindow) {
    if !shared.expanded.load(Ordering::Relaxed) {
        return;
    }
    let (Some((w, h)), Ok(pos)) = (logical_size_of(panel), panel.outer_position()) else {
        return;
    };
    let key = shared.display.lock().ok().and_then(|k| k.clone());
    if let Ok(mut d) = shared.data.lock() {
        d.exp_w = w;
        d.exp_h = h;
        // 位置留物理屏幕坐标，还原时会夹回当前工作区
        d.exp_x = pos.x;
        d.exp_y = pos.y;
        if let Some(key) = key {
            d.stash_layout(&key);
        }
    }
}

fn expand(app: &AppHandle, shared: &Shared) {
    let Some(panel) = panel_of(app) else { return };
    remember_panel_size(shared, &panel);

    let Some((w, h, x, y)) = expanded_geometry(shared, &panel) else {
        return;
    };

    shared.expanded.store(true, Ordering::Relaxed);
    shared.hold_resize(700);
    let _ = panel.set_size(PhysicalSize::new(w as u32, h as u32));
    let _ = panel.set_position(PhysicalPosition::new(x, y));
    let _ = panel.show();
    let _ = panel.set_focus();
    raise_orb(app);
    let _ = app.emit("hn:expanded", true);
    tr!("expand {w}x{h} @ {x},{y}");
    // 显式落盘：放大是用户的明确动作，不该等到下一次存笔记才留住。
    persist(app, shared);
}

fn collapse(app: &AppHandle, shared: &Shared) {
    let Some(panel) = panel_of(app) else { return };
    remember_expanded(shared, &panel); // 先把放大态的几何存下来再切走

    shared.expanded.store(false, Ordering::Relaxed);
    shared.hold_grace(650);
    shared.interacted.store(false, Ordering::Relaxed);
    // 走 apply_layout 而不是直接 set_size：贴角尺寸也要夹回工作区，
    // 否则从大屏换到小屏之后一缩回来就是一块顶出屏幕的板子。
    apply_layout(app, shared);
    let _ = app.emit("hn:expanded", false);
    tr!("collapse");
    persist(app, shared);
}

#[tauri::command]
fn toggle_expand(app: AppHandle, shared: State<'_, Shared>) -> bool {
    if shared.expanded.load(Ordering::Relaxed) {
        collapse(&app, &shared);
        false
    } else {
        expand(&app, &shared);
        true
    }
}

/// 面板的位置或尺寸被用户改过了。放大态记进 exp_*，贴角态记尺寸并重新贴回角上。
#[tauri::command]
fn panel_geometry(app: AppHandle, shared: State<'_, Shared>) {
    // 先认显示配置。换过了的话，这次回调多半是系统在 WM_DPICHANGED 里按比例重排
    // 引起的，不是用户拖的——这时候要做的是换一套记忆，而不是把手上这个尺寸记进
    // 新配置里，那会把上次在这块屏上调好的尺寸盖掉。
    match sync_layout(&app, &shared) {
        Display::Switched => {
            persist(&app, &shared);
            return;
        }
        // 读数正在变，还没定下来。这会儿手上的尺寸属于哪块屏都说不准，别记。
        Display::InFlux => return,
        Display::Same => {}
    }
    // 程序自己刚改过尺寸的话，这次回调是它引起的，不是用户拖的。
    if let Ok(g) = shared.resize_guard.lock() {
        if Instant::now() < *g {
            return;
        }
    }
    let Some(panel) = panel_of(&app) else { return };
    if shared.expanded.load(Ordering::Relaxed) {
        remember_expanded(&shared, &panel);
    } else {
        remember_panel_size(&shared, &panel);
        anchor_panel(&app, &shared);
    }
    persist(&app, &shared);
}

#[tauri::command]
fn quit_app(app: AppHandle, shared: State<'_, Shared>) {
    persist(&app, &shared);
    app.exit(0);
}

// -------------------------------------------------------------- 自动隐藏

fn point_in(area: &Area, x: i32, y: i32, slack: i32) -> bool {
    x >= area.left - slack
        && x < area.right + slack
        && y >= area.top - slack
        && y < area.bottom + slack
}

/// 轮询光标，而不是靠 DOM 的 mouseleave——鼠标快速划出窗口时 DOM 事件
/// 会丢，而且光标飘到窗口外的别的应用上时 webview 根本收不到事件。
fn spawn_hover_watch(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(110));

        let Some(shared) = app.try_state::<Shared>() else {
            continue;
        };
        let Some(panel) = panel_of(&app) else { continue };

        if !panel.is_visible().unwrap_or(false) {
            continue;
        }
        if shared.expanded.load(Ordering::Relaxed) {
            continue;
        }
        if let Ok(g) = shared.grace.lock() {
            if Instant::now() < *g {
                continue;
            }
        }
        // 已经点进去在编辑了：只有当面板同时失去焦点才收，否则打字打到一半
        // 鼠标偶然滑出去就消失是不可接受的。
        if shared.interacted.load(Ordering::Relaxed) && panel.is_focused().unwrap_or(false) {
            continue;
        }

        let Some((cx, cy)) = platform::cursor_pos() else {
            continue;
        };
        let slack = (HOVER_SLACK * scale_of(&panel)).round() as i32;
        let panel_rect = rect_of(&panel);
        let orb_rect = orb_of(&app).and_then(|o| rect_of(&o));
        let over_panel = panel_rect.is_some_and(|r| point_in(&r, cx, cy, slack));
        let over_orb = orb_rect.is_some_and(|r| point_in(&r, cx, cy, slack));
        if over_panel || over_orb {
            continue;
        }

        if std::env::var_os("HOVERNOTE_TRACE").is_some() {
            eprintln!(
                "[hide] cursor=({cx},{cy}) panel={panel_rect:?} orb={orb_rect:?} \
                 interacted={} focused={:?}",
                shared.interacted.load(Ordering::Relaxed),
                panel.is_focused(),
            );
        }

        let app2 = app.clone();
        let _ = app.run_on_main_thread(move || {
            if let Some(s) = app2.try_state::<Shared>() {
                hide_panel_now(&app2, &s);
            }
        });
    });
}

// --------------------------------------------------------------- 启动装配

/// 折角的位置不能只在启动和换角时算一次。
///
/// 它依赖两样随时会变的东西：显示器工作区（`rcWork`）和显示缩放。开机自启时最容易
/// 出事——登录那一刻任务栏可能还没建出来，这时 `rcWork` 就是整块屏幕，折角被摆到
/// 屏幕最底下，等任务栏出现它就悬在半空（或干脆被压在任务栏后面）。之后改分辨率、
/// 把任务栏挪到别的边、开关任务栏自动隐藏、插拔显示器，同样会让原先贴死的位置失效。
///
/// 这些情况没有一个统一可靠的通知点，与其一个个去挂钩子（而且在窗口事件回调里回头
/// 改窗口容易打转），不如按固定节奏对一次答案：每 400ms 三次 Win32 调用，代价可以
/// 忽略，且对所有原因一视同仁。校正是幂等的，贴死之后就什么都不做。
fn spawn_corner_watch(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(400));
        let app2 = app.clone();
        // 回主线程做：这里要连着读写窗口几何，别和事件循环抢。
        let _ = app.run_on_main_thread(move || {
            let Some(shared) = app2.try_state::<Shared>() else {
                return;
            };
            // 显示配置变了就换一套尺寸记忆。和折角的校正搭同一趟车：改分辨率、
            // 插拔显示器、改缩放各走各的消息，没有一个统一可靠的通知点，而窗口
            // 正好被甩到屏幕外时可能一条都收不到。
            let switched = sync_layout(&app2, &shared) == Display::Switched;
            if switched {
                // 换配置是稀罕事，不是每 400ms 都写盘。而这一刻正是值得留住的：
                // 头一回见的那套配置刚在表里开了户。
                persist(&app2, &shared);
            }
            if !snap_orb(&app2) && !switched {
                return;
            }
            // 折角挪了，正开着的面板得跟着回到同一个角上。
            let visible = panel_of(&app2)
                .and_then(|p| p.is_visible().ok())
                .unwrap_or(false);
            if visible {
                anchor_panel(&app2, &shared);
            }
        });
    });
}

fn setup_window_chrome(app: &AppHandle) {
    if let Some(panel) = panel_of(app) {
        platform::round_corners(&panel);
        platform::no_border(&panel);
        platform::hide_from_alt_tab(&panel);
    }
    if let Some(orb) = orb_of(app) {
        platform::no_border(&orb);
        platform::square_corners(&orb);
        platform::hide_from_alt_tab(&orb);
        // 必须先摘掉标题栏样式，否则系统撑出的最小宽度会顶掉下面的 set_size。
        platform::strip_caption(&orb);
        if let Some(px) = orb_side(&orb) {
            let _ = orb.set_size(PhysicalSize::new(px as u32, px as u32));
        }
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "呼出笔记", true, None::<&str>)?;
    let reset = MenuItem::with_id(app, "reset", "折角回到右下角", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出 HoverNote", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &reset, &quit])?;

    let mut tray = TrayIconBuilder::with_id("tray")
        .tooltip("HoverNote")
        .menu(&menu)
        .show_menu_on_left_click(false);
    if let Some(icon) = app.default_window_icon().cloned() {
        tray = tray.icon(icon);
    }

    tray.on_menu_event(|app, event| {
        let Some(shared) = app.try_state::<Shared>() else {
            return;
        };
        match event.id.as_ref() {
            "open" => show_panel(app, &shared),
            "reset" => {
                if let Ok(mut d) = shared.data.lock() {
                    d.corner = Corner::BottomRight;
                    d.orb = None; // 连显示器记忆一起清掉，回到当前主屏
                }
                place_orb(app, Corner::BottomRight, None);
                if let Some(orb) = orb_of(app) {
                    let _ = orb.show();
                }
                anchor_panel(app, &shared);
                let _ = app.emit("hn:corner", Corner::BottomRight);
                persist(app, &shared);
            }
            "quit" => {
                persist(app, &shared);
                app.exit(0);
            }
            _ => {}
        }
    })
    .on_tray_icon_event(|tray, event| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = event
        {
            let app = tray.app_handle();
            if let Some(shared) = app.try_state::<Shared>() {
                if let Some(orb) = orb_of(app) {
                    let _ = orb.show();
                }
                show_panel(app, &shared);
            }
        }
    })
    .build(app)?;

    Ok(())
}

/// 摆放窗口。必须等事件循环跑起来之后才能做。
///
/// `setup` 回调是在 `run()` 之前跑的，那时事件循环还没开始转。任何需要往循环里
/// 投一条消息再等回执的调用——`set_size` / `show` / `outer_position` / `is_visible`
/// ——都会直接以 `FailedToReceiveMessage` 失败。失败是静默的（这些方法返回
/// `Result`，之前一律 `let _ =` 丢掉了），于是折角窗口就停在系统给的默认尺寸上
/// （262×71，标题栏撑出来的最小宽度）并且永远不显示：进程活着、托盘也在，屏幕角上
/// 却什么都没有。开机自启时尤其容易撞上，因为登录瞬间机器最忙。
fn init_windows(app: &AppHandle) {
    let Some(shared) = app.try_state::<Shared>() else {
        return;
    };
    // 先把要用的值取出来再放锁——anchor_panel 内部还要再锁一次。
    let (corner, hint) = match shared.data.lock() {
        Ok(d) => (d.corner, d.orb),
        Err(_) => return,
    };

    setup_window_chrome(app);

    // 面板尺寸要等折角落位之后再摆：贴角态的尺寸是按折角那块屏的缩放和工作区
    // 算的，而这会儿面板还停在系统给的默认位置上（多半是主屏）。
    place_orb(app, corner, hint);
    if let Some(orb) = orb_of(app) {
        let _ = orb.show();
        let _ = orb.set_always_on_top(true);
        tr!(
            "orb ready: rect={:?} visible={:?}",
            rect_of(&orb),
            orb.is_visible()
        );
    }
    // 认显示配置的活儿交给 spawn_corner_watch——它要连着读到两次一样的读数才认，
    // 而启动这一刻恰恰是最容易读到假值的时候（见 sync_layout）。这里先按台面上的
    // 尺寸（上次退出时那份）把面板摆好；等配置认定了，apply_layout 会再摆一次。
    // 面板这会儿还是隐藏的，看不见这次调整。
    apply_layout(app, &shared);
    // 摆好之后才开始盯——盯的是"摆好的位置会不会失效"，见 spawn_corner_watch。
    spawn_corner_watch(app.clone());
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 已经有一个在跑就直接退出。开机自启和手动点开可能撞在一起，两个实例会
    // 各贴一个折角、各建一个托盘图标，还会互相覆盖存档。
    if !platform::claim_single_instance() {
        return;
    }

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            load_state,
            save_notes,
            orb_grab,
            orb_drag_to,
            orb_settle,
            toggle_panel,
            peek_panel,
            current_corner,
            hide_panel,
            mark_interacted,
            open_url,
            toggle_expand,
            panel_geometry,
            quit_app,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            // 冷启动时 WebView2 可能要几十秒才把窗口建出来（首次运行、杀毒扫描、
            // 或上一次被强杀留下的 WebView2 残留还占着用户数据目录的锁）。这条日志
            // 标出"窗口已经就绪、setup 开始跑"的时刻，用来区分"卡在建窗口"和
            // "建好了但没显示"——两者从外面看都是"进程活着但什么都没有"。
            tr!(
                "setup: orb={} panel={}",
                orb_of(&handle).is_some(),
                panel_of(&handle).is_some()
            );
            let loaded = state::load(&handle);
            if loaded.is_broken() {
                // 存档在但读不出来。内存里现在是一份空数据，这一整轮运行都不会写盘
                // （见 Shared::read_only），否则就把磁盘上的好笔记覆盖没了。
                tr!(
                    "存档读取失败，本次运行只读，不会写盘：{}",
                    state::store_dir().display()
                );
            } else {
                // load 可能迁移旧版几何；立即落盘，避免用户尚未触发其他保存动作时
                // 旧的物理像素尺寸仍留在文件里，下一次启动又重复迁移。
                state::save(&handle, loaded.data());
            }
            app.manage(Shared::new(&loaded));

            // 摆窗口的活儿全部挪到 RunEvent::Ready 里做——这里事件循环还没转，
            // 见 init_windows 的注释。托盘不碰窗口，可以就地建。
            build_tray(&handle)?;
            spawn_hover_watch(handle);
            Ok(())
        })
        .on_window_event(|window, event| {
            // 无边框窗口没有关闭按钮，但系统仍可能发来关闭请求；一律退到托盘。
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let app = window.app_handle();
                if let Some(shared) = app.try_state::<Shared>() {
                    hide_panel_now(app, &shared);
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("启动 HoverNote 失败")
        .run(|app, event| match event {
            // 事件循环已经开始转，这时候摆窗口才不会 FailedToReceiveMessage。
            tauri::RunEvent::Ready => init_windows(app),
            tauri::RunEvent::ExitRequested { .. } => {
                if let Some(shared) = app.try_state::<Shared>() {
                    persist(app, &shared);
                }
            }
            _ => {}
        });
}
