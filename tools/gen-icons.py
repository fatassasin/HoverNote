# 从用户提供的原图生成全部图标，不再手绘近似。
#
#   python tools/gen-icons.py
#
# 两张源图，各管一头：
#
#   tools/assets/fold-reference.webp   只有卷角、白纸部分透明。折角小窗要的正是
#                                      「只画卷角、其余留空」，直接重采样即可。
#   tools/assets/app-icon.png          用户给的成品应用图标：黑色圆角方块、纸纹、
#                                      右下卷角都已经画进去了。这里只缩放，不再
#                                      往上合成任何东西——早先那版是拿卷角自己贴
#                                      到一个纯色圆角方块上凑出来的，圆角半径、
#                                      内缩量全是猜的，和成品图对不上。
#
# 产出：
#   src/assets/fold.png        折角窗口用
#   src-tauri/icons/*.png      应用图标
#   src-tauri/icons/icon.ico   同上，多尺寸
#
# 改完图标必须重新 cargo build --release：图标是编译期嵌进 exe 资源里的，
# 光换掉这几个文件，装好的那个 exe 不会有任何变化。

from pathlib import Path
from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
FOLD_REF = ROOT / 'tools' / 'assets' / 'fold-reference.webp'
APP_REF = ROOT / 'tools' / 'assets' / 'app-icon.png'
ORB_OUT = ROOT / 'src' / 'assets' / 'fold.png'
ICON_DIR = ROOT / 'src-tauri' / 'icons'


def load(path: Path) -> Image.Image:
    if not path.exists():
        raise SystemExit(f'缺少原图：{path}')
    return Image.open(path).convert('RGBA')


def make_orb(size: int = 512) -> None:
    """折角窗口的素材：原样重采样，直角顶点仍在右下。"""
    ORB_OUT.parent.mkdir(parents=True, exist_ok=True)
    load(FOLD_REF).resize((size, size), Image.LANCZOS).save(ORB_OUT)


def main() -> None:
    make_orb()

    app = load(APP_REF)
    ICON_DIR.mkdir(parents=True, exist_ok=True)
    cache: dict[int, Image.Image] = {}

    def at(n: int) -> Image.Image:
        if n not in cache:
            cache[n] = app.resize((n, n), Image.LANCZOS)
        return cache[n]

    for name, n in [
        ('32x32.png', 32),
        ('128x128.png', 128),
        ('128x128@2x.png', 256),
        ('icon.png', 512),
    ]:
        at(n).save(ICON_DIR / name)

    # 每一档都从原图单独缩，不让 Pillow 拿 256 那张再往下降。小尺寸上纸纹和卷角
    # 的高光很容易糊成一团灰，多缩一次就更糊一次。
    ico_sizes = [16, 24, 32, 48, 64, 128, 256]
    at(256).save(
        ICON_DIR / 'icon.ico',
        format='ICO',
        sizes=[(n, n) for n in ico_sizes],
        append_images=[at(n) for n in ico_sizes if n != 256],
    )

    print('orb   ->', ORB_OUT)
    print('icons ->', ICON_DIR)
    for f in sorted(ICON_DIR.iterdir()):
        print('  ', f.name)


if __name__ == '__main__':
    main()
