// 折角图标：鼠标浮上去就展开笔记，拖动只能在四个角之间换位置。
//
// 它不跟着鼠标自由移动——需求是这枚折角只能待在四个角上，不能停在边上，
// 也不能飘到屏幕中间。所以拖动的语义是「挑一个角」：把光标的屏幕坐标交给
// 后端，后端算出最近的角再整块搬过去。前端连窗口坐标都不用碰。

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const orb = document.getElementById('orb');
const hit = document.getElementById('hit');

/** 判定为拖动的位移阈值（CSS 像素），小于它算单击。 */
const DRAG_SLOP = 4;

const CORNER_CLASSES = ['corner-tl', 'corner-tr', 'corner-bl', 'corner-br'];

let drag = null;
let pending = null;
let raf = 0;

function applyCorner(corner) {
  const cls = `corner-${corner}`;
  document.body.classList.remove(...CORNER_CLASSES);
  document.body.classList.add(CORNER_CLASSES.includes(cls) ? cls : 'corner-br');
}

function flush() {
  raf = 0;
  if (!pending) return;
  const at = pending;
  pending = null;
  invoke('orb_drag_to', at);
}

// 浮上来就展开。拖动过程中不触发，否则刚把面板收起来又被自己弹开。
//
// mouseenter 只在「进入」时发一次，可是面板收起来的时候鼠标往往还停在折角上
// （比如刚点过一次收起，或者拖完松手），那之后再也不会有 enter 事件，
// 悬停就失灵了。所以补一条节流过的 mousemove 兜底。
let lastPeek = 0;
function peek() {
  if (drag) return;
  const now = performance.now();
  if (now - lastPeek < 220) return;
  lastPeek = now;
  invoke('peek_panel');
}

hit.addEventListener('mouseenter', peek);
hit.addEventListener('mousemove', peek);

hit.addEventListener('pointerdown', (e) => {
  if (e.button !== 0) return;
  e.preventDefault();
  hit.setPointerCapture(e.pointerId);
  drag = { id: e.pointerId, sx: e.screenX, sy: e.screenY, moved: 0, grabbed: false };
});

hit.addEventListener('pointermove', (e) => {
  if (!drag || e.pointerId !== drag.id) return;
  const dx = Math.abs(e.screenX - drag.sx);
  const dy = Math.abs(e.screenY - drag.sy);
  drag.moved = Math.max(drag.moved, dx + dy);
  if (drag.moved < DRAG_SLOP) return;

  if (!drag.grabbed) {
    drag.grabbed = true;
    document.body.classList.add('dragging');
    invoke('orb_grab');
  }

  // 交屏幕物理坐标，剩下的判断都在后端做——它才知道有几块显示器、工作区多大。
  const dpr = window.devicePixelRatio || 1;
  pending = { x: Math.round(e.screenX * dpr), y: Math.round(e.screenY * dpr) };
  if (!raf) raf = requestAnimationFrame(flush);
});

function finish(e) {
  if (!drag || (e && e.pointerId !== drag.id)) return;
  const wasDrag = drag.grabbed;
  drag = null;
  document.body.classList.remove('dragging');

  if (wasDrag) {
    if (raf) {
      cancelAnimationFrame(raf);
      raf = 0;
    }
    flush();
    invoke('orb_settle');
  } else {
    invoke('toggle_panel');
  }
}

hit.addEventListener('pointerup', finish);
hit.addEventListener('pointercancel', finish);
orb.addEventListener('contextmenu', (e) => e.preventDefault());

listen('hn:corner', (e) => applyCorner(e.payload));
invoke('current_corner').then(applyCorner);
