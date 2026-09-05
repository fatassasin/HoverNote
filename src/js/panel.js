// 笔记面板。编辑区里没有标题栏——改名只在左侧笔记栏里做：双击名字才进编辑，
// 悬停和单击都不进（单击是切笔记，那是这个列表的主要用途）。

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const { getCurrentWindow } = window.__TAURI__.window;

const win = getCurrentWindow();

const els = {
  tabmark: document.getElementById('tabmark'),
  drawer: document.getElementById('drawer'),
  list: document.getElementById('notes'),
  dropline: document.getElementById('dropline'),
  editor: document.getElementById('editor'),
  expand: document.getElementById('expand'),
  add: document.getElementById('add'),
  addrow: document.getElementById('addrow'),
  find: document.getElementById('find'),
  findq: document.getElementById('findq'),
  findprev: document.getElementById('findprev'),
  findnext: document.getElementById('findnext'),
};

// notes 是平铺的，数组顺序就是显示顺序。分组不另存成员名单：归属只写在
// note.group 里，一处记录，就不会出现「组里说有、笔记说没有」这种对不上的状态。
// 一个组画在它第一个成员所在的位置上。
let notes = [];
let groups = [];
let activeId = null;
let pinned = false;
let expanded = false;
let closeTimer = 0;
let saveTimer = 0;
let marked = false;

// --------------------------------------------------------------- 数据

/** 取第一个没被占用的 Note_N。永远从 Note_1 开始试。 */
function nextName() {
  const taken = new Set(notes.map((n) => n.title));
  let i = 1;
  while (taken.has(`Note_${i}`)) i++;
  return `Note_${i}`;
}

/** 同上，给组用。组名和笔记名各排各的号，两边是不同的东西，撞了也不碍事。 */
function nextGroupName() {
  const taken = new Set(groups.map((g) => g.title));
  let i = 1;
  while (taken.has(`Group_${i}`)) i++;
  return `Group_${i}`;
}

function save() {
  clearTimeout(saveTimer);
  saveTimer = setTimeout(() => {
    invoke('save_notes', { notes, groups, active: activeId });
  }, 400);
}

function saveNow() {
  clearTimeout(saveTimer);
  invoke('save_notes', { notes, groups, active: activeId });
}

function current() {
  return notes.find((n) => n.id === activeId) || null;
}

function groupOf(id) {
  return groups.find((g) => g.id === id) || null;
}

function membersOf(id) {
  return notes.filter((n) => n.group === id);
}

/** 丢掉一个成员都不剩的组。只在空了才丢——剩一个也留着，用户可能正打算再拖一个
    进来，而自动解散会把他刚起的组名一起扔了。 */
function pruneGroups() {
  groups = groups.filter((g) => notes.some((n) => n.group === g.id));
}

function newNote() {
  const now = Date.now();
  return {
    id: crypto.randomUUID(),
    title: nextName(),
    body: '',
    group: null,
    created: now,
    updated: now,
  };
}

/**
 * 新建一篇笔记。
 *
 * `rename` 为真就把笔记栏拉出来、直接进名字编辑并全选。新建之后第一件事几乎
 * 总是起名字，而名字只在笔记栏里能改（编辑区没有标题栏），让人自己翻出列表
 * 再双击是白绕一圈；`Note_3` 这种占位名也没什么可留的，全选着直接覆盖就行。
 *
 * 不传就是老样子：焦点落到正文里。启动时补的那篇空笔记走这条——那会儿面板
 * 还没露面，弹一个改名框出来没人看得见。
 */
function createNote(rename) {
  const note = newNote();
  notes.push(note);
  activeId = note.id;
  render();
  saveNow();

  if (!rename) {
    els.editor.focus();
    return;
  }
  openDrawer();
  const input = els.list.querySelector(`.note[data-id="${note.id}"] .name`);
  // 找不到那个输入框（理论上不会）就退回正文，新建之后焦点不能悬空。
  if (input) beginEdit(input, true);
  else els.editor.focus();
}

function deleteNote(id) {
  const at = notes.findIndex((n) => n.id === id);
  if (at < 0) return;
  notes.splice(at, 1);

  if (activeId === id) {
    // 删掉当前这条就落到相邻一条；一条都不剩就补一条空的，
    // 编辑区不能处于「没有笔记」的状态。
    const next = notes[at] || notes[at - 1];
    if (next) {
      activeId = next.id;
    } else {
      const fresh = newNote();
      notes.push(fresh);
      activeId = fresh.id;
    }
  }
  pruneGroups();
  render();
  saveNow();
}

/** 解散一个组：成员全部回到顶层，原地不动。
 *
 * 组上那枚按钮是「解散」不是「删除」——一下删掉好几篇笔记的按钮，长得和单篇
 * 那个删除键一模一样、还挨在同一列，迟早会误按。解散顶多是白拖几下，删错是丢东西。
 */
function dissolveGroup(id) {
  for (const n of notes) if (n.group === id) n.group = null;
  pruneGroups();
  renderList();
  saveNow();
}

function toggleGroup(id) {
  const g = groupOf(id);
  if (!g) return;
  g.collapsed = !g.collapsed;
  renderList();
  saveNow();
}

function setActive(id) {
  if (id === activeId) return;
  activeId = id;
  const note = current();
  loadDoc(note ? note.body : '');
  for (const li of els.list.children) {
    if (li.classList.contains('note')) li.classList.toggle('active', li.dataset.id === id);
  }
  saveNow();
}


// --------------------------------------------------------------- 渲染

/**
 * 把平铺的 notes 摊成要画的行序列。
 *
 * 一个组画在它**第一个成员**所在的位置，成员紧跟其后。这样「组在哪」不用另存
 * 一个字段，从 notes 的顺序就能推出来，也就不存在顺序和组位置对不上的可能。
 */
function rows() {
  const out = [];
  const drawn = new Set();
  for (const note of notes) {
    if (!note.group) {
      out.push({ kind: 'note', note });
      continue;
    }
    const g = groupOf(note.group);
    if (!g) {
      // 指向一个不存在的组。load 时会洗掉，这里再兜一次，免得笔记凭空消失。
      out.push({ kind: 'note', note });
      continue;
    }
    if (drawn.has(g.id)) continue;
    drawn.add(g.id);
    out.push({ kind: 'group', group: g });
    if (!g.collapsed) {
      for (const m of membersOf(g.id)) out.push({ kind: 'note', note: m, sub: true });
    }
  }
  return out;
}

function nameInput(text) {
  const name = document.createElement('input');
  name.className = 'name';
  name.value = text;
  name.readOnly = true;
  name.tabIndex = -1;
  name.spellcheck = false;
  return name;
}

function xButton(cls, label) {
  const b = document.createElement('button');
  b.className = cls;
  b.setAttribute('aria-label', label);
  b.innerHTML =
    '<svg class="ico" viewBox="0 0 18 18" width="12" height="12" aria-hidden="true">' +
    '<path d="M5.6 5.6l6.8 6.8"/><path d="M12.4 5.6l-6.8 6.8"/></svg>';
  return b;
}

function noteRow(note, sub) {
  const li = document.createElement('li');
  li.className = 'note' + (note.id === activeId ? ' active' : '') + (sub ? ' sub' : '');
  li.dataset.id = note.id;
  li.dataset.kind = 'note';

  const dot = document.createElement('span');
  dot.className = 'dot';

  li.append(dot, nameInput(note.title), xButton('del', `删除 ${note.title}`));
  return li;
}

function groupRow(g) {
  const li = document.createElement('li');
  li.className = 'grp' + (g.collapsed ? ' shut' : '');
  li.dataset.id = g.id;
  li.dataset.kind = 'group';

  // 收起/展开。做成按钮而不是让整行去切换：整行还要能拖、能双击改名，
  // 单击再兼一个折叠，三种意图挤在一个手势上，哪个都做不准。
  const tog = document.createElement('button');
  tog.className = 'gtog';
  tog.setAttribute('aria-label', g.collapsed ? `展开 ${g.title}` : `收起 ${g.title}`);
  tog.innerHTML =
    '<svg class="ico" viewBox="0 0 18 18" width="12" height="12" aria-hidden="true">' +
    '<path d="M4.5 7 9 11.4 13.5 7"/></svg>';

  li.append(tog, nameInput(g.title));

  // 只在收起时报数。展开着的时候成员就在下面摆着，再写个数字是多余的；
  // 收起来之后这是唯一还能看出「里面有东西」的线索。
  if (g.collapsed) {
    const n = document.createElement('span');
    n.className = 'gnum';
    n.textContent = membersOf(g.id).length;
    li.append(n);
  }

  li.append(xButton('del gdis', `解散 ${g.title}（笔记会回到顶层，不会被删）`));
  return li;
}

/** 只重画列表。拖拽、折叠、改名走这条——它们没换笔记，不该动编辑区，
    更不该把撤销栈和光标位置一起冲掉。 */
function renderList() {
  els.list.textContent = '';
  for (const row of rows()) {
    els.list.append(row.kind === 'group' ? groupRow(row.group) : noteRow(row.note, row.sub));
  }
}

/** 重画列表并把当前笔记装进编辑区。换了笔记才用这个。 */
function render() {
  renderList();
  const note = current();
  loadDoc(note ? note.body : '');
}


// ------------------------------------------------------------- 改名

function isEditing() {
  return !!els.list.querySelector('.name:not([readonly])');
}

/** 进名字编辑。`fresh` 标记这是新建笔记带出来的那次——名字定完焦点要接着
    落到正文里，那本来就是新建的落点，只是中间插了一步起名字。 */
function beginEdit(input, fresh) {
  if (!input.readOnly) return;
  input.dataset.prev = input.value;
  if (fresh) input.dataset.fresh = '1';
  input.readOnly = false;
  input.tabIndex = 0;
  input.focus();
  input.select();
}

function endEdit(input, commit) {
  if (input.readOnly) return;
  const li = input.closest('.note, .grp');
  const isGroup = li?.dataset.kind === 'group';
  // 组名和笔记名各排各的号，重名只在同类之间才算冲突。
  const pool = isGroup ? groups : notes;
  const item = pool.find((x) => x.id === li?.dataset.id);

  if (commit && item) {
    const name = input.value.trim();
    // 名字不能为空，也不接受和同类重名——否则列表里没法分辨。
    const clash = pool.some((x) => x !== item && x.title === name);
    if (name && !clash) {
      item.title = name;
      if (!isGroup) item.updated = Date.now();
      saveNow();
    }
  }
  if (item) input.value = item.title;

  input.readOnly = true;
  input.tabIndex = -1;
  delete input.dataset.prev;
  delete input.dataset.fresh;
  input.blur();

  // 名字定了，笔记栏交还给平时那套开合。
  //
  // 改名期间它是被 `isEditing()` 钉住的（见 scheduleClose 和下面那个 pointermove）。
  // 而那两条收起的路都是事件驱动的：鼠标早在打字之前就移开了，不会再移一次给
  // 我们第二个机会。所以这里得主动收一下，否则改完名字抽屉就一直支棱着。
  //
  // 钉住了、或者鼠标这会儿还停在笔记栏/页签上，那本来就该开着，不动它。
  if (!pinned && !els.drawer.matches(':hover') && !els.tabmark.matches(':hover')) {
    shutDrawer();
  }
}

// ------------------------------------------------------------- 事件

els.list.addEventListener('click', (e) => {
  // 拖完手指抬起来还会补一个 click。那一下不是「选中」的意思，吃掉。
  if (dragDidMove) {
    dragDidMove = false;
    return;
  }
  const li = e.target.closest('.note, .grp');
  if (!li) return;

  if (li.dataset.kind === 'group') {
    if (e.target.closest('.gdis')) dissolveGroup(li.dataset.id);
    // 名字那块留给双击改名，别的地方点一下都是收起/展开——折叠键当然算，
    // 但整行都能按会好按得多，这一行本来也没有别的单击含义。
    else if (!e.target.closest('.name')) toggleGroup(li.dataset.id);
    return;
  }

  if (e.target.closest('.del')) {
    deleteNote(li.dataset.id);
    return;
  }
  // 单击一律只是选中。改名要再点一次（见下面的 dblclick）——点一下就变成
  // 输入框的话，光是切换笔记都会误触发，而切换才是这个列表的主要用途。
  setActive(li.dataset.id);
});

// 第二下点击才进编辑，且立刻进。
els.list.addEventListener('dblclick', (e) => {
  const name = e.target.closest('.name');
  if (name) beginEdit(name);
});

els.list.addEventListener(
  'blur',
  (e) => {
    if (e.target.classList?.contains('name')) endEdit(e.target, true);
  },
  true,
);

els.list.addEventListener('keydown', (e) => {
  const input = e.target.closest?.('.name');
  if (!input || input.readOnly) return;
  if (e.key !== 'Enter' && e.key !== 'Escape') return;
  e.preventDefault();
  if (e.key === 'Escape') {
    // 拦住，别让 window 上那个 Escape 处理再跑一遍——那会把整个面板收走
    e.stopPropagation();
    input.value = input.dataset.prev ?? input.value;
  }
  // 新建带出来的那次改名，名字一定下来就接着写正文。改已有笔记的名字不抢焦点：
  // 人还在列表里翻，八成是要接着改下一个或者切一篇。
  const fresh = input.dataset.fresh === '1';
  endEdit(input, e.key === 'Enter');
  if (fresh) els.editor.focus();
});

// --------------------------------------------------- 拖拽排序 / 拖成一组
//
// 用 pointer 事件自己做，不用 HTML5 的 draggable：需要区分「插到两行之间」和
// 「摞到某一行上」这两种落点，而原生拖放只告诉你在哪个元素上，落在元素的哪个
// 高度上还得自己算——那还不如整套自己拿着，顺便把自动滚动和幽灵行也管了。
//
// 行高 30px，上下各 8px 判插入、中间 14px 判并组。边缘再窄就点不准，中间再窄
// 就很难故意组队。

const DRAG_SLOP = 4; // 手抖这么多像素内还算点击，不算拖
const EDGE = 8;

let pending = null; // 按下了但还没超过阈值
let drag = null;
let dragDidMove = false; // 拖完那一下 click 要吃掉

function rowInfo(li) {
  if (li.dataset.kind === 'group') {
    const g = groupOf(li.dataset.id);
    return { kind: 'group', group: g, gid: g?.id ?? null };
  }
  const note = notes.find((n) => n.id === li.dataset.id) || null;
  return { kind: 'note', note, gid: note?.group ?? null };
}

/** 组里还没被拖走的成员。落点的锚必须是留在原地的那些——拿正被拖的那条当锚，
    插入时它已经从数组里摘掉了，找不到就只能退到末尾，落点就跑了。 */
function liveMembers(gid) {
  return notes.filter((n) => n.group === gid && !drag?.notes.has(n.id));
}

/** 顶层的插入点。落在组里的行上就吸附到整个组的外面——拖着一个组的时候，
    落点不可能是「另一个组的中间」。 */
function topLine(li, before) {
  const info = rowInfo(li);
  if (info.gid) {
    const mem = liveMembers(info.gid);
    const edge = before ? mem[0] : mem[mem.length - 1];
    return { group: null, anchor: edge?.id ?? null, where: before ? 'before' : 'after' };
  }
  return { group: null, anchor: info.note?.id ?? null, where: before ? 'before' : 'after' };
}

/** 光标在哪儿就该落到哪儿。返回落点方案 + 画给用户看的提示。 */
function dropPlan(y) {
  const end = { group: null, anchor: null, where: 'after', makeWith: null, ui: null };
  const cands = [...els.list.children].filter((el) => !drag.ids.has(el.dataset.id));
  if (!cands.length) return end;

  const firstR = cands[0].getBoundingClientRect();
  if (y < firstR.top) {
    return { ...topLine(cands[0], true), makeWith: null, ui: { el: cands[0], after: false } };
  }
  const lastEl = cands[cands.length - 1];
  const lastR = lastEl.getBoundingClientRect();
  if (y > lastR.bottom) {
    return { ...topLine(lastEl, false), makeWith: null, ui: { el: lastEl, after: true } };
  }

  let el = lastEl;
  let r = lastR;
  for (const c of cands) {
    const cr = c.getBoundingClientRect();
    if (y >= cr.top && y <= cr.bottom) {
      el = c;
      r = cr;
      break;
    }
  }

  // 拖着一个组：只能在顶层的边界之间落，组里不套组。
  if (drag.kind === 'group') {
    const before = y < r.top + r.height / 2;
    return { ...topLine(el, before), makeWith: null, ui: { el, after: !before } };
  }

  const info = rowInfo(el);
  const zone = y < r.top + EDGE ? 'top' : y > r.bottom - EDGE ? 'bottom' : 'mid';

  // 落在组的标题行上：上沿是「插到这个组前面」，其余都是「放进这个组」。
  if (info.kind === 'group') {
    if (zone === 'top') {
      return { ...topLine(el, true), makeWith: null, ui: { el, after: false } };
    }
    const mem = liveMembers(info.gid);
    return {
      group: info.gid,
      anchor: mem[0]?.id ?? null,
      where: 'before',
      makeWith: null,
      ui: { el, into: true },
    };
  }

  const id = info.note?.id ?? null;

  // 顶层的笔记，正中间：这两条并成一个新组。锚就落在被摞的那条上——
  // 不给锚的话新成员会被扔到数组末尾，组虽然还是画得对（组按第一个成员定位，
  // 成员是查出来的），但两个成员在数组里被别的笔记隔开了，往后再拖就容易乱。
  if (!info.gid) {
    if (zone === 'mid') return { group: null, anchor: id, where: 'after', makeWith: id, ui: { el, into: true } };
    return { group: null, anchor: id, where: zone === 'top' ? 'before' : 'after', makeWith: null, ui: { el, after: zone === 'bottom' } };
  }

  // 组里的笔记。正中间＝加进这个组、排在它后面；因此「摞到最后一个成员上」
  // 就是往组末尾追加。
  if (zone === 'mid') {
    return { group: info.gid, anchor: id, where: 'after', makeWith: null, ui: { el, into: true } };
  }
  if (zone === 'top') {
    return { group: info.gid, anchor: id, where: 'before', makeWith: null, ui: { el, after: false } };
  }
  // 下沿。最后一个成员的下沿归顶层——否则一个组后面就再也插不进东西了，
  // 想排到两个组之间会没有落点。组末尾用上面那条「摞到最后一个成员上」去够。
  const mem = membersOf(info.gid);
  const last = mem[mem.length - 1]?.id === id;
  return {
    group: last ? null : info.gid,
    anchor: id,
    where: 'after',
    makeWith: null,
    ui: { el, after: true },
  };
}

/**
 * 落实一次拖放。
 *
 * `moving` 是显式传进来的（拖的是组还是单篇、涉及哪几篇），不从 drag 里读：
 * 调用它的时候 drag 已经清干净了——收尾必须先做，否则中途抛出去就会留下
 * 幽灵行和满身状态类。之前这里直接读 drag，每次松手都在这一行抛
 * TypeError，看着像"拖拽根本没生效"，其实是拖得好好的、只是没落地。
 */
function applyPlan(plan, moving) {
  const moved = notes.filter((n) => moving.notes.has(n.id));
  if (!moved.length) return;

  let gid = plan.group;
  if (plan.makeWith) {
    const host = notes.find((n) => n.id === plan.makeWith);
    if (!host) return;
    if (host.group) {
      gid = host.group;
    } else {
      const g = { id: crypto.randomUUID(), title: nextGroupName(), collapsed: false };
      groups.push(g);
      host.group = g.id;
      gid = g.id;
    }
  }

  // 拖一整个组的时候成员的归属不变，跟着一起挪就行。
  if (moving.kind !== 'group') {
    for (const n of moved) n.group = gid;
  }

  const rest = notes.filter((n) => !moving.notes.has(n.id));
  let at;
  if (!plan.anchor) {
    at = plan.where === 'before' ? 0 : rest.length;
  } else {
    const i = rest.findIndex((n) => n.id === plan.anchor);
    at = i < 0 ? rest.length : plan.where === 'before' ? i : i + 1;
  }
  rest.splice(at, 0, ...moved);
  notes = rest;
  pruneGroups();
}

function paintDrop(plan) {
  for (const el of els.list.children) el.classList.remove('drop-into');
  const ui = plan?.ui;
  if (!ui) {
    els.dropline.classList.remove('on');
    return;
  }
  if (ui.into) {
    els.dropline.classList.remove('on');
    ui.el.classList.add('drop-into');
    return;
  }
  // offsetParent 是 .drawer（它是定位元素），而指示线也挂在 .drawer 里，
  // 所以这个坐标直接能用，抽屉滚动时两者一起动。
  const top = ui.el.offsetTop + (ui.after ? ui.el.offsetHeight : 0);
  els.dropline.style.top = `${top - 1}px`;
  els.dropline.classList.toggle('sub', !!plan.group);
  els.dropline.classList.add('on');
}

function startDrag(li, e) {
  const kind = li.dataset.kind;
  const ids = new Set([li.dataset.id]);
  const noteIds = new Set();
  if (kind === 'group') {
    for (const m of membersOf(li.dataset.id)) {
      ids.add(m.id);
      noteIds.add(m.id);
    }
  } else {
    noteIds.add(li.dataset.id);
  }

  const r = li.getBoundingClientRect();
  const ghost = li.cloneNode(true);
  ghost.classList.add('ghost');
  ghost.classList.remove('sub');
  ghost.style.width = `${r.width}px`;
  document.body.append(ghost);

  drag = {
    kind,
    ids,
    notes: noteIds,
    ghost,
    offX: r.left - e.clientX,
    offY: r.top - e.clientY,
    x: e.clientX,
    y: e.clientY,
    plan: null,
  };

  document.body.classList.add('dragging');
  for (const el of els.list.children) {
    if (ids.has(el.dataset.id)) el.classList.add('drag-src');
  }
  // 拖起来之后才抓指针，这样手指/鼠标移出列表也还收得到事件。
  //
  // 不在 pointerdown 就抓：抓着的时候浏览器会把后续事件都改派给捕获元素，
  // 而随后那个 click 一旦被改派到 <ul> 上，处理函数里的 closest('.note') 就
  // 找不到行了——单纯点一下选笔记会整个失灵。只在真的开拖时抓，普通点击这条
  // 路上压根没有捕获，也就没有这个风险。
  els.list.setPointerCapture(e.pointerId);

  // 按下时可能已经在输入框里拖出一小段选区了，抹掉，否则拖动过程中一直亮着
  window.getSelection()?.removeAllRanges();
  requestAnimationFrame(dragTick);
}

/** 重算落点并把提示画出来。 */
function updateDrop() {
  if (!drag) return;
  drag.plan = dropPlan(drag.y);
  paintDrop(drag.plan);
}

/** 每帧跑一次，只为了贴边自动滚动。滚完要重算落点：光标没动，但底下的行换了。
 *
 * 落点本身不依赖这个循环——rAF 在窗口不可见、被遮住、或者掉帧的时候可能一帧都
 * 不给，那样拖动就成了全程没反馈、松手也没反应。指针一动就算一次才是主路径，
 * 这里只是补上"手停着不动、内容却在滚"这一种情况。 */
function dragTick() {
  if (!drag) return;
  const r = els.drawer.getBoundingClientRect();
  const PAD = 22;
  let d = 0;
  if (drag.y < r.top + PAD) d = -Math.ceil((r.top + PAD - drag.y) / 3);
  else if (drag.y > r.bottom - PAD) d = Math.ceil((drag.y - (r.bottom - PAD)) / 3);
  if (d) {
    els.drawer.scrollTop += d;
    updateDrop();
  }
  requestAnimationFrame(dragTick);
}

function endDrag(commit) {
  if (!drag) return;
  const plan = drag.plan;
  // 收尾要先做干净：幽灵行、状态类、指示线都得撤掉，之后再动数据。
  // 所以 applyPlan 需要的东西在这儿先抄出来，不能等它自己去 drag 上拿。
  const moving = { kind: drag.kind, notes: drag.notes };
  drag.ghost.remove();
  drag = null;
  document.body.classList.remove('dragging');
  els.dropline.classList.remove('on');

  if (commit && plan) {
    applyPlan(plan, moving);
    renderList();
    saveNow();
  } else {
    renderList();
  }
}

els.list.addEventListener('pointerdown', (e) => {
  if (e.button !== 0) return;

  // 每次按下都先清掉「刚拖过」的标记，放在所有提前 return 之前。
  //
  // 拖远之后松手，那一下 click 会派发到 <ul> 上（按下和抬起不在同一行，取的是
  // 共同祖先），处理函数在 closest 那一步就返回了，标记没人消费。若这里再因为
  // 按在删除键上而提前返回、跳过清理，紧接着那次删除就会被当成「拖完的余波」
  // 吃掉——按一下没反应，得按第二下。
  dragDidMove = false;

  const li = e.target.closest('.note, .grp');
  if (!li) return;
  // 那几个按钮各有各的动作，按在上面不是要拖
  if (e.target.closest('.del, .gtog')) return;
  // 正在改名：这时候在输入框里按下去是要选字，不是要拖
  if (isEditing()) return;

  pending = { li, x: e.clientX, y: e.clientY };
});

els.list.addEventListener('pointermove', (e) => {
  if (pending) {
    if (Math.hypot(e.clientX - pending.x, e.clientY - pending.y) < DRAG_SLOP) return;
    const li = pending.li;
    pending = null;
    dragDidMove = true;
    startDrag(li, e);
  }
  if (!drag) return;
  e.preventDefault();
  drag.x = e.clientX;
  drag.y = e.clientY;
  drag.ghost.style.transform = `translate(${e.clientX + drag.offX}px, ${e.clientY + drag.offY}px)`;
  // 指针一动就重算，不等下一帧：这是落点的主路径，rAF 那个循环只管自动滚动。
  updateDrop();
});

els.list.addEventListener('pointerup', (e) => {
  pending = null;
  if (els.list.hasPointerCapture(e.pointerId)) els.list.releasePointerCapture(e.pointerId);
  endDrag(true);
});

// 拖到一半被打断（系统手势、失焦……）：原样放回去，别留下半截状态。
els.list.addEventListener('pointercancel', () => {
  pending = null;
  endDrag(false);
});

// ------------------------------------------------------------- 笔记栏开合
//
// 书签栏：悬停拉出，移开收起；点一下钉住。

function openDrawer() {
  clearTimeout(closeTimer);
  document.body.classList.add('drawer-open');
}

function scheduleClose() {
  clearTimeout(closeTimer);
  closeTimer = setTimeout(() => {
    if (pinned || isEditing() || drag) return;
    document.body.classList.remove('drawer-open');
  }, 220);
}

/** 只管把笔记栏收起来，不碰正开着的改名。`endEdit` 收尾时走这条——
    它要是去调 closeDrawer，两个函数就会互相调进去。 */
function shutDrawer() {
  clearTimeout(closeTimer);
  pinned = false;
  document.body.classList.remove('drawer-open');
}

/** 收起笔记栏，并把正开着的改名一并结束（按提交算）。
 *
 * 改名必须跟着一起结束：`isEditing()` 是笔记栏的「别收」闸门，而输入框是画在
 * 笔记栏里的。笔记栏被强行收走之后它还留在 DOM 里、还不是 readOnly，闸门就
 * 一直是开的，此后每一条收起的路都被它挡住——这扇门再也关不严了。 */
function closeDrawer() {
  const input = els.list.querySelector('.name:not([readonly])');
  if (input) endEdit(input, true);
  shutDrawer();
}

for (const el of [els.tabmark, els.drawer]) {
  el.addEventListener('mouseenter', openDrawer);
  el.addEventListener('mouseleave', scheduleClose);
}

els.tabmark.addEventListener('click', () => {
  pinned = !pinned;
  if (pinned) openDrawer();
  // 点击收起要立刻生效。scheduleClose 那 220ms 宽限期是给悬停路径的——鼠标从
  // 页签往下移进笔记栏时中间隔着一段，两边都不在，没有宽限期抽屉会在半途消失。
  // 点击没有这段空隙要跨，等待纯粹是延迟，而且展开是即时的，一等就不对称了。
  else closeDrawer();
});

// 光标掉到笔记栏下沿以下就立刻收起来。往下走只可能是奔着正文去的，这时候还
// 支棱着一大片列表挡在上面纯属碍事。
//
// 这条比「钉住」优先：钉住是说「别因为鼠标随便飘一下就收」，不是说「我明确往
// 下走了你也别动」。往上/往左右移开仍然受钉住保护，那几个方向本来就够不着正文。
//
// 改名和拖拽期间不收：一个会把没写完的名字连输入框一起撤掉，另一个正需要列表在。
window.addEventListener('pointermove', (e) => {
  if (!document.body.classList.contains('drawer-open')) return;
  if (drag || pending || isEditing()) return;
  // 留 2px 余量，免得正好停在边界上时反复开合
  if (e.clientY > els.drawer.getBoundingClientRect().bottom + 2) closeDrawer();
});

// ------------------------------------------------------------- 正文编辑区
//
// contenteditable 而不是 textarea：要让 ## 打完立刻变成大字，字号就得真的变，
// 而 textarea 要求所有字符同字号，否则光标位置算不准。
//
// 每次输入都把整篇重画一遍，再按字符偏移把光标放回去。听着重，但一篇便签撑死
// 几千字，重画一次不到一毫秒；换来的是不必去追浏览器往 contenteditable 里插进来
// 的各种结构——它爱插什么插什么，下一帧全被覆盖掉。

// plaintext-only 让粘贴自动降级成纯文本，也挡掉浏览器自带的加粗/斜体命令：
// 格式在这里只由符号决定，不该有第二个来源。万一这个取值不被支持，属性会整个
// 失效、这块就变成不可编辑，所以设完立刻验一下，不行就退回 true。
els.editor.setAttribute('contenteditable', 'plaintext-only');
if (!els.editor.isContentEditable) {
  els.editor.setAttribute('contenteditable', 'true');
}

let composing = false;

/** 从 DOM 读回源码。每个顶层子节点算一行。 */
function docText() {
  const out = [];
  for (const node of els.editor.childNodes) out.push(node.textContent);
  return out.join('\n');
}

/** 光标在整篇源码里的字符偏移；行与行之间算一个换行符。 */
function caretRange() {
  const sel = window.getSelection();
  if (!sel || !sel.rangeCount) return null;
  const r = sel.getRangeAt(0);
  if (!els.editor.contains(r.startContainer)) return null;
  return {
    start: offsetOf(r.startContainer, r.startOffset),
    end: offsetOf(r.endContainer, r.endOffset),
  };
}

function offsetOf(node, off) {
  const root = els.editor;
  // 选区直接落在编辑区上时，off 是子节点下标，也就是"前面那些整行"
  if (node === root) {
    let n = 0;
    for (let i = 0; i < off && i < root.childNodes.length; i++) {
      n += root.childNodes[i].textContent.length + 1;
    }
    return n;
  }

  let ln = node;
  while (ln && ln.parentNode !== root) ln = ln.parentNode;
  if (!ln) return 0;

  let base = 0;
  for (const sib of root.childNodes) {
    if (sib === ln) break;
    base += sib.textContent.length + 1;
  }
  return base + withinLine(ln, node, off);
}

function withinLine(ln, node, off) {
  if (node === ln) {
    // 全删空之后浏览器会在顶层留一个裸文本节点，这时 off 就是字符偏移
    if (ln.nodeType === Node.TEXT_NODE) return off;
    let n = 0;
    for (let i = 0; i < off && i < ln.childNodes.length; i++) {
      n += ln.childNodes[i].textContent.length;
    }
    return n;
  }
  let n = 0;
  const walk = document.createTreeWalker(ln, NodeFilter.SHOW_TEXT);
  let t;
  while ((t = walk.nextNode())) {
    if (t === node) return n + off;
    n += t.data.length;
  }
  return n;
}

/** 按字符偏移找回 DOM 里的落点。 */
function locate(target) {
  const root = els.editor;
  let n = 0;
  for (const ln of root.childNodes) {
    const len = ln.textContent.length;
    if (target <= n + len) {
      let rest = target - n;
      if (ln.nodeType === Node.TEXT_NODE) {
        return { node: ln, off: Math.min(rest, ln.data.length) };
      }
      const walk = document.createTreeWalker(ln, NodeFilter.SHOW_TEXT);
      let t;
      while ((t = walk.nextNode())) {
        if (rest <= t.data.length) return { node: t, off: rest };
        rest -= t.data.length;
      }
      // 空行：这一行只有一个 <br>，光标落在它前面
      return { node: ln, off: 0 };
    }
    n += len + 1;
  }
  const last = root.lastChild;
  if (!last) return { node: root, off: 0 };
  if (last.nodeType === Node.TEXT_NODE) return { node: last, off: last.data.length };
  return { node: last, off: last.childNodes.length };
}

function placeCaret(start, end) {
  const a = locate(start);
  const b = end === start ? a : locate(end);
  if (!a || !b) return;
  const r = document.createRange();
  try {
    r.setStart(a.node, a.off);
    r.setEnd(b.node, b.off);
  } catch {
    return; // 偏移落在了已经不存在的结构上，宁可不动光标也别抛
  }
  const sel = window.getSelection();
  sel.removeAllRanges();
  sel.addRange(r);
}

/** 把源码画到编辑区；给了 caret 就把光标放回那个字符偏移。 */
function paint(text, caret) {
  // 命中在每次重画前重算，不留缓存。文本可能刚被改过、也可能刚换了一篇笔记，
  // 让偏移过夜就会标到错的地方去。一次 indexOf 扫几 KB，代价可以忽略。
  findHits = findOn ? scan(text, findQuery) : [];
  if (findAt >= findHits.length) findAt = 0;

  els.editor.innerHTML = MD.paint(text);
  els.editor.classList.toggle('blank', !text);
  highlight();
  if (caret) placeCaret(caret.start, caret.end);
  activeLine();
  if (findOn) findStatus();
}

/** 光标所在那一行亮起来：只有这一行的符号回到正常亮度，方便改。 */
function activeLine() {
  const sel = window.getSelection();
  let ln = null;
  if (sel && sel.rangeCount) {
    let node = sel.getRangeAt(0).startContainer;
    if (els.editor.contains(node)) {
      ln = node;
      while (ln && ln.parentNode !== els.editor) ln = ln.parentNode;
    }
  }
  for (const el of els.editor.children) el.classList.toggle('on', el === ln);
}

// ----------------------------------------------------------- 查找
//
// Ctrl+F 从顶部浮下来，每打一个字重新匹配一次，命中处染橙色。
//
// 高亮是画完之后在文本节点上拆出来的，不是在 MD.paint 吐出的那串 HTML 里做替换。
// 那串东西里有标签，直接替换的话搜 "a" 会打中 <a href>、"class" 会打中 class=，
// 标记当场就撕烂了。拆文本节点只动文本，textContent 一个字都不变，于是
// 「DOM 文本逐字等于源码」那条不变式还在——光标偏移、选区、复制全都照旧。

let findOn = false;
let findQuery = '';
let findHits = [];
let findAt = 0;
// 改关键词时从这个偏移往后找第一处。开搜索时它是光标位置，之后跟着当前命中走，
// 这样一边补字一边找不会每次都被扔回文首。
let findFrom = 0;

/**
 * 在源码里找出所有命中，返回起始偏移的升序数组。长度一律是 findQuery.length。
 *
 * 用 indexOf 而不是正则：关键词是用户直接打进来的，`(` 这种字符会让正则抛异常，
 * 而 `.*` 这种又会变成完全不同的意思——查找框里打什么就该找什么。
 */
function scan(text, q) {
  if (!q) return [];

  let hay = text.toLowerCase();
  let needle = q.toLowerCase();
  // 极少数字符转小写之后长度会变（比如 'İ' 会变成两个码元），一旦变了，
  // 小写串上的偏移就和源码对不上，整篇会标偏。这时退回区分大小写地找，
  // 宁可少匹配几处，也不能把橙色标到别的字上。
  if (hay.length !== text.length || needle.length !== q.length) {
    hay = text;
    needle = q;
  }

  const out = [];
  let at = 0;
  // 上限是给病态输入兜底的：在一篇长笔记里搜单个空格，几万处高亮画出来只是卡顿，
  // 没有任何人能用。到顶就停——后面还有没有匹配，这里不再关心。
  while (out.length < 1000) {
    const i = hay.indexOf(needle, at);
    if (i < 0) break;
    out.push(i);
    // 不重叠：`aa` 在 `aaa` 里算一处，和各家编辑器的惯例一致
    at = i + needle.length;
  }
  return out;
}

/** 按 findHits 把命中处包进 <mark>。必须在 innerHTML 写完之后、放光标之前调。 */
function highlight() {
  if (!findHits.length) return;
  const len = findQuery.length;

  let base = 0; // 当前这一行第一个字符在整篇里的偏移
  let h = 0; // 下一个还没安置的命中
  for (const ln of els.editor.childNodes) {
    const span = ln.textContent.length;

    // 关键词里不可能有换行（查找框是单行输入），所以一处命中必定整个落在某一行里，
    // 只要起点在这一行，终点就一定也在。命中是升序的，一个指针扫过去就够了。
    const spots = [];
    while (h < findHits.length && findHits[h] < base + span) {
      spots.push({ i: h, a: findHits[h] - base, b: findHits[h] + len - base });
      h++;
    }
    if (spots.length && ln.nodeType === Node.ELEMENT_NODE) markLine(ln, spots);

    base += span + 1; // +1 是行尾那个换行符
  }
}

/** 把一行里的若干段（行内偏移）包进 <mark>。 */
function markLine(ln, spots) {
  // 先把文本节点连同各自的起始偏移收齐再动手。边走边拆的话，TreeWalker 会不会
  // 看见新拆出来的节点是不确定的，而且拆过之后后面那些偏移也未必还算数。
  const nodes = [];
  let at = 0;
  const walk = document.createTreeWalker(ln, NodeFilter.SHOW_TEXT);
  let t;
  while ((t = walk.nextNode())) {
    nodes.push({ node: t, from: at });
    at += t.data.length;
  }

  for (const { node, from } of nodes) {
    const to = from + node.data.length;
    let cut = node; // 还没被拆走的左半截，偏移始终是从 from 起算的

    // 从右往左拆。splitText 只会改动切点右边，左边那截的偏移原封不动，
    // 所以倒着走就不用在每次拆完之后重算剩下那些位置。
    for (let s = spots.length - 1; s >= 0; s--) {
      const { i, a, b } = spots[s];
      const lo = Math.max(a, from);
      const hi = Math.min(b, to);
      if (lo >= hi) continue; // 这一处不落在当前文本节点里

      // 甩掉右边不该染的部分。条件按 cut 当前的长度判，不是按节点原始的长度：
      // 右边可能已经被上一处切走了。两处命中紧挨着时（在 aaaa 里搜 aa）差值正好
      // 为零，这时什么都不用切，切了只会多出一个空文本节点。
      if (hi - from < cut.data.length) cut.splitText(hi - from);
      const mid = lo > from ? cut.splitText(lo - from) : cut;

      const m = document.createElement('mark');
      if (i === findAt) m.className = 'cur';
      mid.parentNode.insertBefore(m, mid);
      m.appendChild(mid);

      if (mid === cut) break; // 命中一直顶到节点开头，左边没东西可拆了
    }
  }
}

/** findAt 挪到 findFrom 之后的第一处；没有就回到第一处。 */
function findSeek() {
  findAt = 0;
  for (let i = 0; i < findHits.length; i++) {
    if (findHits[i] >= findFrom) {
      findAt = i;
      break;
    }
  }
}

/** 没匹配上就把输入框描红。计数去掉之后，这是唯一的"没找到"信号。 */
function findStatus() {
  els.find.classList.toggle('miss', !!findQuery && !findHits.length);
}

function findScroll() {
  els.editor.querySelector('mark.cur')?.scrollIntoView({ block: 'nearest' });
}

/** 关键词变了：重算、跳到光标之后的第一处、重画。 */
function findChanged() {
  findQuery = els.findq.value;
  findHits = scan(docText(), findQuery);
  findSeek();
  // 光标传 null：焦点在查找框里，编辑区本来就没有选区可留
  paint(docText(), null);
  findScroll();
}

function findStep(dir) {
  if (!findHits.length) return;
  findAt = (findAt + dir + findHits.length) % findHits.length;
  findFrom = findHits[findAt]; // 接着补关键词时从这儿往后找，别弹回文首
  paint(docText(), null);
  findScroll();
}

function findOpen() {
  if (findOn) {
    // 已经开着再按一次 Ctrl+F：把关键词全选上，直接改写就行——各家都是这个行为
    els.findq.focus();
    els.findq.select();
    return;
  }

  const caret = caretRange();
  findFrom = caret ? caret.start : 0;
  // 选中一段再按 Ctrl+F，就拿它当关键词。跨行的选区不要：查找框是单行的，
  // 塞进去的换行会被吃掉，搜出来的东西和选中的那段对不上。
  if (caret && caret.end > caret.start) {
    const picked = docText().slice(caret.start, caret.end);
    if (picked && !picked.includes('\n')) findQuery = picked;
  }

  findOn = true;
  document.body.classList.add('find-open');
  closeDrawer(); // 抽屉也是从顶上下来的，两个叠在一起没法看

  els.findq.value = findQuery;
  findHits = scan(docText(), findQuery);
  findSeek();
  paint(docText(), null);
  els.findq.focus();
  els.findq.select();
  findScroll();
}

/**
 * 关掉查找条，清掉高亮。
 *
 * `keep` 是关掉之后要留住的选区。不传就选中当前照亮的那一处——按 Esc 退出查找，
 * 想干的多半就是改这个词，光标直接落在上面最省事。点正文那条路必须传：那一下
 * 点击已经把光标放到人想去的地方了，再跳回旧的命中处等于把人从刚点的位置拽走。
 */
function findClose(keep) {
  if (!findOn) return;

  let sel = keep;
  if (sel === undefined) {
    const at = findHits[findAt];
    sel = at == null ? null : { start: at, end: at + findQuery.length };
  }

  findOn = false;
  document.body.classList.remove('find-open');
  els.find.classList.remove('miss');

  // 先 focus 再画：innerHTML 换过之后焦点还在编辑区上，这时候设的选区才落得住。
  els.editor.focus();
  paint(docText(), sel);
}

// ----------------------------------------------------------- 撤销
//
// 自己维护：每次输入都重写 innerHTML，浏览器原生的撤销记录活不过一次重画。

let undoStack = [];
let redoStack = [];
let undoAt = 0;

function record(text, caret) {
  const top = undoStack[undoStack.length - 1];
  if (top && top.text === text) return;
  const now = Date.now();
  // 连续打字合并成一个还原点，否则按一次 Ctrl+Z 只退一个字，退回上一句要按几十次
  if (top && undoStack.length > 1 && now - undoAt < 600) {
    undoStack[undoStack.length - 1] = { text, caret };
  } else {
    undoStack.push({ text, caret });
    if (undoStack.length > 300) undoStack.shift();
  }
  undoAt = now;
  redoStack = [];
}

function restore(snap) {
  const note = current();
  if (note) {
    note.body = snap.text;
    note.updated = Date.now();
    save();
  }
  paint(snap.text, snap.caret);
  els.editor.focus();
  undoAt = 0; // 撤销之后下一次输入必定另起一个还原点，别合并进来
}

function undo() {
  if (undoStack.length < 2) return;
  redoStack.push(undoStack.pop());
  restore(undoStack[undoStack.length - 1]);
}

function redo() {
  const snap = redoStack.pop();
  if (!snap) return;
  undoStack.push(snap);
  restore(snap);
}

/** 换笔记：重画并清空撤销栈——两篇笔记的撤销历史不该串在一起。 */
function loadDoc(text) {
  const src = text || '';
  // 换了一篇，上一篇的光标位置没有意义了，查找从头开始数
  findFrom = 0;
  findAt = 0;
  paint(src);
  undoStack = [{ text: src, caret: null }];
  redoStack = [];
  undoAt = 0;
}

/** 用新源码替换全文，并把光标放到 at。粘贴走这条路。 */
function replaceAll(text, at) {
  const caret = { start: at, end: at };
  const note = current();
  if (note) {
    note.body = text;
    note.updated = Date.now();
    save();
  }
  record(text, caret);
  paint(text, caret);
}

// ----------------------------------------------------------- 编辑区事件

els.editor.addEventListener('input', () => {
  const text = docText();
  const note = current();
  if (note) {
    note.body = text;
    note.updated = Date.now();
    save();
  }
  // 输入法组合期间一个字都不能动：重画会把没提交的候选文字连同组合状态一起冲掉，
  // 中文就成了打一个字掉一个字。等 compositionend 再画。
  if (composing) return;
  const caret = caretRange();
  record(text, caret);
  paint(text, caret);
});

els.editor.addEventListener('compositionstart', () => {
  composing = true;
});

els.editor.addEventListener('compositionend', () => {
  composing = false;
  const text = docText();
  const caret = caretRange();
  record(text, caret);
  paint(text, caret);
});

els.editor.addEventListener('paste', (e) => {
  const data = e.clipboardData?.getData('text/plain');
  if (data == null) return;
  e.preventDefault();
  const text = docText();
  const caret = caretRange() || { start: text.length, end: text.length };
  const clean = data.replace(/\r\n?/g, '\n');
  replaceAll(text.slice(0, caret.start) + clean + text.slice(caret.end), caret.start + clean.length);
});

els.editor.addEventListener('keydown', (e) => {
  if (!(e.ctrlKey || e.metaKey)) return;
  const k = e.key.toLowerCase();
  if (k === 'z' && !e.shiftKey) {
    e.preventDefault();
    undo();
  } else if (k === 'y' || (k === 'z' && e.shiftKey)) {
    e.preventDefault();
    redo();
  }
});

// Ctrl+点击才打开链接。这是编辑区，平时单击得照常把光标放进去——点一下就跳走的话
// 改链接文字就没法做了。跳转本身必须交给系统浏览器：面板自己就是个 webview，
// 让它跟着 <a> 走等于把笔记界面换成一个没有地址栏的网页，回不来。
els.editor.addEventListener('click', (e) => {
  const a = e.target.closest?.('a');
  if (!a || !(e.ctrlKey || e.metaKey)) return;
  e.preventDefault();
  invoke('open_url', { url: a.getAttribute('href') });
});

// 点正文就把查找条收起来。找到了要改，手自然是往那个词上点——还得先记着按一下
// Esc 才能清掉满屏橙色，是白多的一步。
//
// 走 click 不走 pointerdown：关掉要重画整个编辑区，而按下那一刻浏览器还没把光标
// 放进去，DOM 一换光标就没了着落；拖着选一段更是会直接断在半路。等这一下点完
// （或者这一段选完）再收，把选区抄下来原样还回去，点哪儿光标就还在哪儿。
els.editor.addEventListener('click', () => {
  if (!findOn) return;
  // 取不到选区就退回默认的「选中当前那一处」，总比重画之后光标无处可去强。
  findClose(caretRange() || undefined);
});

document.addEventListener('selectionchange', () => {
  if (document.activeElement === els.editor) activeLine();
});

// 两个新建入口做同一件事：建好之后把笔记栏拉出来、光标停在名字上等着改。
// 两边都不能顺手收起笔记栏了——要改的那个输入框就画在里面。
//
// 收起的时机交给 `endEdit`：名字定了（回车）、放弃了（Esc）、或者点到笔记栏
// 外面去了（输入框失焦），它才把笔记栏交还给平时那套悬停开合。
els.add.addEventListener('click', () => createNote(true));

// 列表末尾那一行。右下角那枚圆形加号也能新建，但翻着列表的时候手已经在这儿了，
// 不该再把鼠标甩到对角去。
els.addrow.addEventListener('click', () => createNote(true));

els.expand.addEventListener('click', () => invoke('toggle_expand'));

// ----------------------------------------------------------- 查找条事件

// 输入法组合期间照样重算。这里和正文那边不一样：重画的是编辑区，查找框自己没被动过，
// 组合状态不会被冲掉。中途拿半截拼音去搜通常一处都匹配不上，显示"无结果"而已，
// 等选完词 input 会再来一次，结果就对了。
els.findq.addEventListener('input', findChanged);

els.findq.addEventListener('keydown', (e) => {
  if (e.key === 'Enter') {
    e.preventDefault();
    findStep(e.shiftKey ? -1 : 1);
  } else if (e.key === 'Escape') {
    e.preventDefault();
    // 拦住，别让下面 window 上那个 Escape 处理再跑一遍
    e.stopPropagation();
    findClose();
  }
});

els.findprev.addEventListener('click', () => findStep(-1));
els.findnext.addEventListener('click', () => findStep(1));

// 八向缩放：把拖拽交给系统窗口管理器，比自己算尺寸跟手得多。
for (const handle of document.querySelectorAll('.rz')) {
  handle.addEventListener('pointerdown', (e) => {
    if (e.button !== 0) return;
    e.preventDefault();
    win.startResizeDragging(handle.dataset.dir);
  });
}

window.addEventListener('keydown', (e) => {
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'f' && !isEditing()) {
    e.preventDefault();
    findOpen();
    return;
  }
  if (e.key !== 'Escape' || isEditing()) return;
  // Esc 一次只退一层：搜索开着的时候按 Esc 想的是「退出搜索」，不是「把面板收走」。
  if (findOn) findClose();
  else invoke('hide_panel');
});

window.addEventListener('contextmenu', (e) => {
  if (!e.target.closest('textarea, input')) e.preventDefault();
});

// 告诉后端「用户真的动了这个面板」，此后不再是鼠标划过就收的预览态。
function mark() {
  if (marked) return;
  marked = true;
  invoke('mark_interacted');
}
window.addEventListener('pointerdown', mark, true);
window.addEventListener('keydown', mark, true);

// 用户拖完边框或挪完窗口：让后端记住几何。贴角态只关心尺寸（位置由锚点决定），
// 放大态位置和尺寸都要记。
let geomTimer = 0;
function geometryChanged() {
  clearTimeout(geomTimer);
  geomTimer = setTimeout(() => invoke('panel_geometry'), 280);
}
win.onResized(geometryChanged);
win.onMoved(() => {
  if (expanded) geometryChanged();
});

// 折角压在面板自己的角上，被盖住的那个控件要让开，见 panel.css。
const CORNER_CLASSES = ['orb-tl', 'orb-tr', 'orb-bl', 'orb-br'];
function applyCorner(corner) {
  const cls = `orb-${corner}`;
  document.body.classList.remove(...CORNER_CLASSES);
  document.body.classList.add(CORNER_CLASSES.includes(cls) ? cls : 'orb-br');
}
listen('hn:corner', (e) => applyCorner(e.payload));

listen('hn:shown', () => {
  marked = false;
  closeDrawer();
  // 面板重新露出来时不该还挂着上一次的搜索——那是上一轮的上下文，
  // 而且一片橙色高亮会盖过「这是刚打开的一篇笔记」这个第一眼。
  findClose();
});

listen('hn:expanded', (e) => {
  expanded = !!e.payload;
  document.body.classList.toggle('expanded', expanded);
  if (expanded) els.editor.focus();
});

// --------------------------------------------------------------- 启动

(async () => {
  applyCorner(await invoke('current_corner'));

  const data = await invoke('load_state');
  notes = Array.isArray(data.notes) ? data.notes : [];
  groups = Array.isArray(data.groups) ? data.groups : [];
  // 兼容早期没有 title 字段的数据，以及手工编辑坏掉的存档。
  for (const note of notes) {
    if (!note.title) note.title = '';
  }
  for (const note of notes) {
    if (!note.title) note.title = nextName();
  }

  // 分组是后加的字段，也可能被手工编辑弄拧。指向一个不存在的组等于没分组——
  // 这里不修的话那几篇笔记会在列表里彻底不见（rows() 找不到组就没法安置它们）。
  const live = new Set(groups.map((g) => g.id));
  for (const note of notes) {
    if (note.group && !live.has(note.group)) note.group = null;
  }
  for (const g of groups) {
    if (!g.title) g.title = '';
  }
  for (const g of groups) {
    if (!g.title) g.title = nextGroupName();
  }
  pruneGroups();

  activeId = notes.some((n) => n.id === data.active) ? data.active : notes[0]?.id ?? null;

  if (!notes.length) {
    createNote();
  } else {
    render();
  }
})();

