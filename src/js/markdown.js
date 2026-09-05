// 实时 Markdown：把一行源码渲染成带样式的 HTML，**符号原样留在输出里**，只是淡显。
//
// 不删符号是这套东西能站住的关键。DOM 里的文本和源码逐字一致，于是光标偏移就是
// 字符偏移——选区、复制、方向键、Home/End 全都不用另做映射。把符号藏起来的编辑器
// （Typora、Obsidian 那种）得为这些各写一套补偿：光标走到隐藏区要跳过去，选中一段
// 复制出来要把符号补回来，删除时要判断删的是不是标记。这里一条都不欠。
//
// 代价是符号一直看得见。但淡到 25% 之后，一眼扫过去看到的是标题的大字和粗体的粗，
// 符号退成了背景纹理；而光标落到哪一行，那一行的符号就恢复正常亮度，方便改。
//
// 自己写而不是引第三方：这个前端没有打包器，引库就得把几十 KB 代码复制进仓库，
// 还没有任何东西替我们跟踪它的版本。而且现成的库都渲染成"最终 HTML"，没有一个
// 会把源码符号一起吐出来——这里最需要的恰恰是那个。

const MD = (() => {
  const ESC = { '&': '&amp;', '<': '&lt;', '>': '&gt;' };
  const esc = (s) => s.replace(/[&<>]/g, (c) => ESC[c]);

  // 标记符号。传进来的必须是已经转义过的文本。
  const mk = (s) => `<span class="mk">${s}</span>`;

  // 只放行明确安全的协议。面板是个 webview，一个 javascript: 链接就是一次注入。
  function safeUrl(url) {
    return /^(https?:\/\/|mailto:)\S/i.test(url) ? url : null;
  }

  // ------------------------------------------------------------ 行内

  function inline(raw) {
    // 先把整行转义，之后所有规则都在转义后的文本上跑。原文里的 `<` 到这一步
    // 已经是 `&lt;`，任何标签都不可能活着到达输出。转义只动 & < >，不碰
    // * ` [ ] ( ) 这些标记字符，所以规则照常匹配。
    let s = esc(raw);

    // 已经成形的片段先存起来换成占位符，免得后面的规则钻进 <a href> 里去改。
    const held = [];
    const hold = (html) => `${held.push(html) - 1}`;

    // 行内代码最先吃掉：它里面的星号反引号都不再是标记。
    s = s.replace(/`([^`\n]+)`/g, (_, body) =>
      hold(mk('`') + `<code>${body}</code>` + mk('`')),
    );

    // 图片不内嵌，按链接处理。内嵌就得放开 CSP 的 img-src 去连外网，
    // 而"打开一篇笔记"不该顺带向某个图床发一次请求。
    s = s.replace(/!\[([^\]\n]*)\]\(([^)\s]+)\)/g, (whole, alt, href) => {
      const url = safeUrl(href);
      if (!url) return whole;
      return hold(mk('![') + `<a href="${url}">${alt || url}</a>` + mk(`](${href})`));
    });

    s = s.replace(/\[([^\]\n]+)\]\(([^)\s]+)\)/g, (whole, text, href) => {
      const url = safeUrl(href);
      if (!url) return whole;
      return hold(mk('[') + `<a href="${url}">${text}</a>` + mk(`](${href})`));
    });

    s = s.replace(/\*\*([^*\n]+)\*\*/g, (_, b) =>
      hold(mk('**') + `<strong>${b}</strong>` + mk('**')),
    );
    s = s.replace(/~~([^~\n]+)~~/g, (_, b) => hold(mk('~~') + `<del>${b}</del>` + mk('~~')));

    // 斜体只认星号，不认下划线。中文里 `a_b_c` 这种命名太常见，`_` 当标记会
    // 频繁误伤——中文字符不算 \w，拦不住它。星号没有这个问题。
    s = s.replace(/(^|[^\w*])\*([^*\n]+)\*(?![\w*])/g, (_, pre, b) =>
      pre + hold(mk('*') + `<em>${b}</em>` + mk('*')),
    );

    // 展开占位符。要循环——粗体里可能套着行内代码，一层展不完。
    for (let i = 0; i < 8 && s.includes(''); i++) {
      s = s.replace(/(\d+)/g, (_, n) => held[n]);
    }
    return s;
  }

  // ------------------------------------------------------------ 行

  /**
   * 渲染一行。`inCode` 是"当前是否在围栏代码块内部"，由调用方按顺序传递。
   * 返回 `{ cls, html, fence }`——`fence` 为真表示这一行是围栏，调用方要翻转状态。
   */
  function line(src, inCode) {
    const fence = /^\s{0,3}(```|~~~)/.test(src);
    if (fence) return { cls: 'code fence', html: mk(esc(src)), fence: true };
    if (inCode) return { cls: 'code', html: esc(src) || '<br>' };

    const head = src.match(/^(#{1,6})(\s+)(.*)$/);
    if (head) {
      return { cls: `h${head[1].length}`, html: mk(head[1] + head[2]) + inline(head[3]) };
    }

    const quote = src.match(/^(\s*>\s?)(.*)$/);
    if (quote) return { cls: 'quote', html: mk(esc(quote[1])) + inline(quote[2]) };

    // 分隔线要排在列表前面：`---` 和 `- - -` 都同时长得像无序列表项。
    if (/^\s*([-*_])\s*(\1\s*){2,}$/.test(src)) {
      return { cls: 'hr', html: mk(esc(src)) };
    }

    const item = src.match(/^(\s*)([-*+]|\d{1,9}[.)])(\s+)(.*)$/);
    if (item) {
      return {
        cls: 'li',
        html: esc(item[1]) + mk(item[2] + item[3]) + inline(item[4]),
      };
    }

    // 空行也要占一行高度，否则行盒塌了，光标没处放。
    return { cls: '', html: inline(src) || '<br>' };
  }

  /** 整篇渲染成一串 `<div class="ln">`，一行一个。 */
  function paint(text) {
    let inCode = false;
    return text
      .split('\n')
      .map((src) => {
        const r = line(src, inCode);
        if (r.fence) inCode = !inCode;
        return `<div class="ln${r.cls ? ' ' + r.cls : ''}">${r.html}</div>`;
      })
      .join('');
  }

  return { paint, safeUrl };
})();
