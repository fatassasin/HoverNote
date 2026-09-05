use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::path::PathBuf;
use tauri::AppHandle;

/// 屏幕四角。字符串形式与前端保持一致。
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Corner {
    #[serde(rename = "tl")]
    TopLeft,
    #[serde(rename = "tr")]
    TopRight,
    #[serde(rename = "bl")]
    BottomLeft,
    #[serde(rename = "br")]
    BottomRight,
}

impl Corner {
    pub fn is_left(self) -> bool {
        matches!(self, Corner::TopLeft | Corner::BottomLeft)
    }
    pub fn is_top(self) -> bool {
        matches!(self, Corner::TopLeft | Corner::TopRight)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Note {
    pub id: String,
    /// 只在左侧笔记栏里出现和编辑，编辑区没有标题栏。
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub body: String,
    /// 所属分组的 id，None 表示在顶层。只有一层，组里不会再套组——面板只有
    /// 380px 宽，再深一层缩进就没地方放字了，拖拽的落点判定也会失去分寸。
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub created: i64,
    #[serde(default)]
    pub updated: i64,
}

/// 一个分组。它自己不存成员名单——成员是靠 `Note.group` 反查的，
/// 只有一处记录归属，就不会出现「组里说有、笔记说没有」这种对不上的状态。
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Group {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub collapsed: bool,
}

/// 一套显示配置下的面板几何。
///
/// `key` 由 `platform::display_key` 给出（`3840x2160@192`）。分开记是因为逻辑像素
/// 只保证「换块屏之后看着一样大」，保证不了「装得下」：在 4K 上拉到 600x1000 的面板
/// 换到 1080p 上就顶到天花板，人一定会去改小；改小这件事本身没错，错的是改完之后
/// 连 4K 那份也跟着没了。
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Layout {
    pub key: String,
    pub panel_w: u32,
    pub panel_h: u32,
    pub exp_w: u32,
    pub exp_h: u32,
    pub exp_x: i32,
    pub exp_y: i32,
}

/// 最多记几套显示配置。笔记本 + 底座 + 偶尔接投影仪也就五六套，16 足够宽裕，
/// 同时给这份表一个上界——插过的每一块屏都留一条，总不能无限长下去。
pub const LAYOUT_MAX: usize = 16;

/// 存档格式版本。尺寸字段从物理像素改成逻辑像素。
///
/// 最初迁移曾使用版本 1，但 `#[serde(default)]` 会把旧文件里缺失的 version 直接补成
/// 当时的当前版本，导致迁移被跳过；版本 2 会把那批已被误标为 1 的存档也可靠地重置。
pub const VERSION: u32 = 2;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Persisted {
    pub version: u32,
    pub corner: Corner,
    /// 折角图标左上角的物理坐标；只用来认出上次在哪块显示器上。
    pub orb: Option<(i32, i32)>,
    /// 贴角态的面板尺寸，**逻辑像素**。
    ///
    /// 必须是逻辑而不是物理：显示缩放一变，物理数字的含义就变了，而窗口又会
    /// 被系统按比例缩放一次，存回去误差会一轮轮滚大。逻辑值在任何缩放下都指
    /// 同样大小。
    pub panel_w: u32,
    pub panel_h: u32,
    /// 放大态的尺寸（逻辑像素）和位置（物理屏幕坐标，还原时会夹回工作区）。
    /// 宽为 0 表示还没放大过，第一次按工作区居中算。
    pub exp_w: u32,
    pub exp_h: u32,
    pub exp_x: i32,
    pub exp_y: i32,
    /// 每套显示配置各记一份几何，最近用过的排在最前，超出 `LAYOUT_MAX` 就丢掉队尾。
    ///
    /// 上面那几个 `panel_*` / `exp_*` 是**当前这套配置**的值，切换配置时由
    /// `adopt_layout` 从这张表里搬过来、由 `stash_layout` 写回去。之所以还留着它们
    /// 而不是让所有代码直接查表：一来读的地方很多，二来碰上一套从没见过的配置时
    /// 得有个起点，而「上一块屏上的逻辑尺寸」正是最合理的那个起点。
    ///
    /// 加这个字段没有升 VERSION，理由同下面的 `groups`。
    pub layouts: Vec<Layout>,
    pub active: Option<String>,
    pub notes: Vec<Note>,
    /// 分组表。笔记本身是平铺的，显示顺序就是 `notes` 的顺序；一个组画在它
    /// 第一个成员的位置上。这样加分组不用动 `notes` 的形状，`removed_notes`
    /// 那套按 id 求差集认删除的逻辑原样还能用。
    ///
    /// 加这两个字段没有升 VERSION：它们都带 serde default，老存档读进来就是
    /// 「没有分组」，本来就对。而升版本会触发上面那段几何重置，把用户调好的
    /// 窗口尺寸白白扔掉。
    pub groups: Vec<Group>,
}

impl Default for Persisted {
    fn default() -> Self {
        Self {
            version: VERSION,
            corner: Corner::BottomRight,
            orb: None,
            // 与 tauri.conf.json 里 panel 窗口的初始尺寸一致
            panel_w: 380,
            panel_h: 470,
            exp_w: 0,
            exp_h: 0,
            exp_x: 0,
            exp_y: 0,
            layouts: Vec::new(),
            active: None,
            notes: Vec::new(),
            groups: Vec::new(),
        }
    }
}

impl Persisted {
    /// 把台面上的几何写回这套显示配置，并把它挪到表头。
    ///
    /// 每次尺寸变动都调，所以「离开一块屏之前要先存一下」这种事不用单独做——
    /// 那块屏上的最后一次改动早就落在表里了。
    pub fn stash_layout(&mut self, key: &str) {
        let entry = Layout {
            key: key.to_string(),
            panel_w: self.panel_w,
            panel_h: self.panel_h,
            exp_w: self.exp_w,
            exp_h: self.exp_h,
            exp_x: self.exp_x,
            exp_y: self.exp_y,
        };
        self.layouts.retain(|l| l.key != key);
        self.layouts.insert(0, entry);
        self.layouts.truncate(LAYOUT_MAX);
    }

    /// 切到这套显示配置：把它记着的几何搬到台面上，顺手挪到表头。
    /// 返回 false 表示这套配置从没见过，台面上的值原样不动。
    pub fn adopt_layout(&mut self, key: &str) -> bool {
        let Some(i) = self.layouts.iter().position(|l| l.key == key) else {
            return false;
        };
        let l = self.layouts.remove(i);
        self.panel_w = l.panel_w;
        self.panel_h = l.panel_h;
        self.exp_w = l.exp_w;
        self.exp_h = l.exp_h;
        self.exp_x = l.exp_x;
        self.exp_y = l.exp_y;
        self.layouts.insert(0, l);
        true
    }
}

/// 指定笔记存放目录的环境变量。
///
/// 留这个口子是因为「笔记该放哪」没有一个对所有人都对的答案：有人有单独的数据盘，
/// 有人整个用户目录都在同步盘里。写死一个路径等于逼别人改代码重编。
pub const STORE_DIR_ENV: &str = "HOVERNOTE_DIR";

/// 笔记的存放目录：`%HOVERNOTE_DIR%`，没设就是 `%USERPROFILE%\Documents\HoverNote`。
///
/// 默认值不用 `app_data_dir()`（那是 `%APPDATA%`，在 C 盘）：系统盘重装/重置/恢复
/// 出厂时最容易被连带清掉，而 `AppData\Roaming` 又是做备份时通常不会特意勾选的
/// 目录。文档目录至少是人人都会备份的那个地方。
///
/// 只解析一次。中途改环境变量不该让读和写落到两个不同的地方——那意味着一次保存会
/// 把笔记写进一个谁也不会再去读的目录。
pub fn store_dir() -> PathBuf {
    static DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        let env = std::env::var_os(STORE_DIR_ENV);
        let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"));
        resolve_store_dir(env.as_deref(), home.as_deref())
    })
    .clone()
}

/// 把「环境变量」和「用户主目录」两个输入落成一个目录。
///
/// 单拎出来是为了能测：`store_dir` 读的是进程环境，而且只解析一次，在测试里摆布不动。
fn resolve_store_dir(env: Option<&OsStr>, home: Option<&OsStr>) -> PathBuf {
    if let Some(dir) = env {
        // 设成空串按「没设」算，否则笔记会落到进程的当前工作目录里——
        // 那是安装目录，卸载时会被整个删掉。
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    // 连主目录都拿不到就退到当前工作目录。不猜任何盘符，也好过直接放弃保存。
    let home = home.map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    home.join("Documents").join("HoverNote")
}

/// 被删掉的笔记存在这里。
pub fn trash_dir() -> PathBuf {
    store_dir().join("trash")
}

/// 上一份里有、这一份里没有，且**正文有内容**的笔记——也就是刚被删掉、值得留一份的。
///
/// 前端删笔记不会单独通知后端，它只是把整份列表重新送上来，所以只能靠差集认出来。
/// 这样做还有个好处：无论前端从哪条路径把笔记弄没了都能兜住，不依赖某一个删除按钮
/// 记得去调某个接口。误判的代价只是回收站里多一个文件，漏判才是真的丢东西。
pub fn removed_notes(before: &[Note], after: &[Note]) -> Vec<Note> {
    use std::collections::HashSet;
    let kept: HashSet<&str> = after.iter().map(|n| n.id.as_str()).collect();
    before
        .iter()
        .filter(|n| !kept.contains(n.id.as_str()) && !n.body.trim().is_empty())
        .cloned()
        .collect()
}

/// 把一篇笔记写进回收站，文件名是「本地时间戳 + 标题」。
///
/// 存成 `.md` 而不是 `.json`：这是给人看的最后一份副本，双击就能读，正文不会被
/// JSON 的 `\n` 转义搞得没法看。头部那几行留着标题和时间，需要的话也能照着还原。
pub fn trash(note: &Note) {
    let dir = trash_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let stamp = crate::platform::local_stamp();
    let title = sanitize_filename(&note.title);
    let body = format!(
        "---\n标题: {}\nid: {}\n删除时间: {}\n---\n\n{}\n",
        note.title,
        note.id,
        readable_stamp(&stamp),
        note.body
    );

    // 同一秒删掉两篇同名的会撞名，往后编号。
    for n in 0..100 {
        let name = if n == 0 {
            format!("{stamp} {title}.md")
        } else {
            format!("{stamp} {title}-{n}.md")
        };
        let path = dir.join(name);
        if path.exists() {
            continue;
        }
        // 用带 fsync 的写：这是这篇笔记最后一份副本了。
        let _ = write_synced(&path, body.as_bytes());
        return;
    }
}

/// `20260823-161900` → `2026-08-23 16:19:00`
fn readable_stamp(stamp: &str) -> String {
    // 全是 ASCII 数字和一个短横，按字节切安全；长度不对就原样返回。
    if stamp.len() != 15 {
        return stamp.to_string();
    }
    format!(
        "{}-{}-{} {}:{}:{}",
        &stamp[0..4],
        &stamp[4..6],
        &stamp[6..8],
        &stamp[9..11],
        &stamp[11..13],
        &stamp[13..15]
    )
}

/// 把标题弄成能当文件名的样子。
fn sanitize_filename(title: &str) -> String {
    let mut s: String = title
        .chars()
        .map(|c| {
            if r#"<>:"/\|?*"#.contains(c) || (c as u32) < 0x20 {
                '_'
            } else {
                c
            }
        })
        .collect();
    // Windows 不接受结尾的点和空格
    s = s.trim().trim_end_matches('.').trim().to_string();
    // 按字符截断，别切在多字节字符中间
    if s.chars().count() > 60 {
        s = s.chars().take(60).collect();
    }
    if s.is_empty() {
        s = "无标题".to_string();
    }
    s
}

pub fn store_path(_app: &AppHandle) -> PathBuf {
    let dir = store_dir();
    let _ = std::fs::create_dir_all(&dir);
    dir.join("hovernote.json")
}

/// 读存档的结果。必须把"没有文件"和"读不出来"分开——两者都返回默认值的话，
/// 一次偶然的读失败就会被后续的 save 把好数据覆盖成空的。
pub enum Loaded {
    /// 读到了（或确实是首次运行，没有文件）。可以正常落盘。
    Ok(Persisted),
    /// 文件在，但读不出来或解析不了。**绝对不能落盘**，否则笔记永久丢失。
    Broken(Persisted),
}

impl Loaded {
    pub fn data(&self) -> &Persisted {
        match self {
            Loaded::Ok(d) | Loaded::Broken(d) => d,
        }
    }
    pub fn is_broken(&self) -> bool {
        matches!(self, Loaded::Broken(_))
    }
}

/// 读存档。文件不存在是正常的首次运行；读到一半失败或 JSON 坏掉则视为异常，
/// 会把坏文件改名留证（`hovernote.broken-<n>.json`），并且告诉调用方别再写盘。
pub fn load(app: &AppHandle) -> Loaded {
    let path = store_path(app);

    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // 首次运行，没有文件。这是正常的。
            return Loaded::Ok(Persisted::default());
        }
        Err(_) => {
            // 文件在但读不了：被杀毒/备份程序锁住、瞬时 IO 错误……
            // 这时候什么都别做，尤其别写盘。
            return Loaded::Broken(Persisted::default());
        }
    };

    let mut data = match serde_json::from_str::<Persisted>(&raw) {
        Ok(d) => d,
        Err(_) => {
            // JSON 坏了。先把原始字节挪到一边留证，至少还能手工抢救。
            quarantine(&path);
            return Loaded::Broken(Persisted::default());
        }
    };

    if data.version < VERSION {
        // 旧存档里的尺寸是物理像素，换算不回来（不知道当时是什么缩放），
        // 直接退回默认几何。笔记内容不受影响。
        let fresh = Persisted::default();
        data.panel_w = fresh.panel_w;
        data.panel_h = fresh.panel_h;
        data.exp_w = 0;
        data.exp_h = 0;
        data.exp_x = 0;
        data.exp_y = 0;
        // 分显示配置的那张表也一起清掉：它记的是同一批换算不回来的数字，
        // 留着只会让某一块屏在下次接上时又把旧值搬出来。
        data.layouts.clear();
        data.version = VERSION;
    }
    Loaded::Ok(data)
}

/// 把坏掉的存档改名留着，不覆盖已有的留证文件。
fn quarantine(path: &std::path::Path) {
    for n in 0..100 {
        let dst = path.with_file_name(format!("hovernote.broken-{n}.json"));
        if !dst.exists() {
            let _ = std::fs::rename(path, dst);
            return;
        }
    }
}

/// 落盘：写临时文件 → fsync → 重命名 → fsync 目录。
///
/// 只有 write + rename 是不够的。NTFS 上 rename 的原子性只覆盖元数据，文件内容
/// 和目录项都还可能留在缓存里；进程崩溃扛得住，**突然断电扛不住**，可能拿到
/// 一个空文件。所以数据和目录都要显式刷盘。
///
/// 覆盖之前先把上一版留成 `.bak`，误删和写坏都还有一次回头的机会。
pub fn save(app: &AppHandle, data: &Persisted) {
    let path = store_path(app);
    let Ok(raw) = serde_json::to_string_pretty(data) else {
        return;
    };

    let tmp = path.with_extension("json.tmp");
    if write_synced(&tmp, raw.as_bytes()).is_err() {
        return;
    }

    // 上一版留一份。备份失败不阻断主流程——主文件写成功才是要紧事。
    if path.exists() {
        let _ = std::fs::copy(&path, path.with_extension("bak"));
    }

    if std::fs::rename(&tmp, &path).is_ok() {
        sync_dir(&store_dir());
    }
}

fn write_synced(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::File::create(path)?;
    f.write_all(bytes)?;
    f.sync_all()
}

/// 刷目录项，让重命名本身也落到盘上。Windows 不允许用 File::open 打开目录，
/// 得走 FILE_FLAG_BACKUP_SEMANTICS；失败就算了，不值得为此让保存失败。
#[cfg(windows)]
fn sync_dir(dir: &std::path::Path) {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    if let Ok(d) = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(dir)
    {
        let _ = d.sync_all();
    }
}

#[cfg(not(windows))]
fn sync_dir(dir: &std::path::Path) {
    if let Ok(d) = std::fs::File::open(dir) {
        let _ = d.sync_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UHD: &str = "3840x2160@192";
    const FHD: &str = "1920x1080@96";

    fn sized(w: u32, h: u32) -> Persisted {
        Persisted {
            panel_w: w,
            panel_h: h,
            ..Default::default()
        }
    }

    /// 在 4K 上调好尺寸 → 换到 1080p 上改小 → 再回 4K，必须还是原来那么大。
    /// 这就是这张表存在的全部理由。
    #[test]
    fn 换屏来回不丢尺寸() {
        let mut d = sized(600, 1000);
        d.stash_layout(UHD);

        // 切到 1080p：没见过这套配置，沿用台面上的值开局。
        assert!(!d.adopt_layout(FHD));
        d.stash_layout(FHD);
        // 在小屏上改小。
        d.panel_w = 500;
        d.panel_h = 700;
        d.stash_layout(FHD);

        // 切回 4K：拿回原来那份，不是小屏上那份。
        assert!(d.adopt_layout(UHD));
        assert_eq!((d.panel_w, d.panel_h), (600, 1000));

        // 再切回 1080p：小屏上改的那份也还在。
        assert!(d.adopt_layout(FHD));
        assert_eq!((d.panel_w, d.panel_h), (500, 700));
    }

    /// 放大态的尺寸和位置跟着同一套配置走——位置存的是物理坐标，
    /// 换块屏之后旧坐标可能压根不在桌面上，各记各的才有意义。
    #[test]
    fn 放大态也分配置记() {
        let mut d = sized(380, 470);
        d.exp_w = 1200;
        d.exp_h = 900;
        d.exp_x = 300;
        d.exp_y = 200;
        d.stash_layout(UHD);

        d.adopt_layout(FHD);
        d.exp_w = 800;
        d.exp_h = 600;
        d.exp_x = 50;
        d.exp_y = 40;
        d.stash_layout(FHD);

        assert!(d.adopt_layout(UHD));
        assert_eq!((d.exp_w, d.exp_h, d.exp_x, d.exp_y), (1200, 900, 300, 200));
    }

    /// 同一套配置反复存不该越攒越多，而且每次都要挪到表头。
    #[test]
    fn 重复存同一套只留一条() {
        let mut d = sized(380, 470);
        d.stash_layout(UHD);
        d.stash_layout(FHD);
        d.stash_layout(UHD);
        assert_eq!(d.layouts.len(), 2);
        assert_eq!(d.layouts[0].key, UHD, "最近用过的排最前");
    }

    /// 超出上限就丢队尾——丢的是最久没用过的那套。
    #[test]
    fn 超出上限丢最旧的() {
        let mut d = sized(380, 470);
        for i in 0..(LAYOUT_MAX + 4) {
            d.panel_w = 400 + i as u32;
            d.stash_layout(&format!("cfg-{i}"));
        }
        assert_eq!(d.layouts.len(), LAYOUT_MAX);
        assert_eq!(d.layouts[0].key, format!("cfg-{}", LAYOUT_MAX + 3));
        assert!(!d.layouts.iter().any(|l| l.key == "cfg-0"));
    }

    /// 没见过的配置：台面上的值原样不动，好让它当新配置的起点。
    /// 逻辑像素换块屏之后看着还是一样大，这个起点是合理的。
    #[test]
    fn 没见过的配置沿用台面上的值() {
        let mut d = sized(600, 1000);
        assert!(!d.adopt_layout("从没见过"));
        assert_eq!((d.panel_w, d.panel_h), (600, 1000));
    }

    /// 老存档没有 layouts 字段，读进来该是空表而不是报错。
    #[test]
    fn 老存档缺字段也读得动() {
        let raw = r#"{"version":2,"corner":"br","panel_w":420,"panel_h":600}"#;
        let d: Persisted = serde_json::from_str(raw).expect("老存档应该照样读得动");
        assert!(d.layouts.is_empty());
        assert_eq!((d.panel_w, d.panel_h), (420, 600));
    }

    /// 设了 HOVERNOTE_DIR 就一切照它来，主目录在哪儿都不管。
    #[test]
    fn 环境变量说了算() {
        let dir = resolve_store_dir(
            Some(OsStr::new(r"D:\我的笔记")),
            Some(OsStr::new(r"C:\Users\someone")),
        );
        assert_eq!(dir, PathBuf::from(r"D:\我的笔记"));
    }

    /// 没设就落到文档目录下。别人 clone 下来直接编译就该能跑，不该去碰一个
    /// 只有原作者机器上才有的盘符。
    #[test]
    fn 没设环境变量就用文档目录() {
        let dir = resolve_store_dir(None, Some(OsStr::new(r"C:\Users\someone")));
        assert_eq!(dir, PathBuf::from(r"C:\Users\someone\Documents\HoverNote"));
    }

    /// 空串按「没设」算。否则会落到进程的当前工作目录，也就是安装目录——
    /// 那个目录卸载时会被整个删掉，笔记跟着一起没。
    #[test]
    fn 环境变量是空串等于没设() {
        let dir = resolve_store_dir(Some(OsStr::new("")), Some(OsStr::new(r"C:\Users\someone")));
        assert_eq!(dir, PathBuf::from(r"C:\Users\someone\Documents\HoverNote"));
    }

    /// 主目录也拿不到时不猜盘符。落到相对路径至少还写得进去，
    /// 而猜一个 `C:\` 开头的路径在没有 C 盘写权限的机器上是直接失败。
    #[test]
    fn 主目录拿不到也不猜盘符() {
        let dir = resolve_store_dir(None, None);
        assert_eq!(dir, PathBuf::from("./Documents/HoverNote"));
    }
}
