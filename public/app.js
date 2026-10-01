'use strict';
/* 服务器改动 · 漫画志 —— 前端逻辑（无框架） */

const $ = (sel, el = document) => el.querySelector(sel);
const KIND_SFX = { '配置': '改', '代码': '码', '部署': '上', '修复': '修', '排查': '查', '回滚': '撤', '其他': '记' };
const KINDS = Object.keys(KIND_SFX);
const CN_NUM = ['零', '一', '二', '三', '四', '五', '六', '七', '八', '九', '十'];
const STATUSES = ['全部', '进行中', '已完成', '搁置'];

/* 快记类型识别：按关键词猜类型（顺序即优先级） */
const KIND_HINTS = [
  ['回滚', ['回滚', '回退', 'rollback', 'revert', '撤销']],
  ['修复', ['修复', '修好', 'fix', 'hotfix', 'bug', '报错', '恢复了', '补上']],
  ['部署', ['部署', '发布', '上线', 'deploy', 'systemctl', 'restart', 'reload', 'docker', 'pm2', 'kubectl', 'nginx -s', 'supervisor', 'launch']],
  ['代码', ['代码', 'git', 'commit', 'push', 'merge', 'rebase', 'pr ', 'pull request', 'review', '分支', 'branch', '重构']],
  ['配置', ['配置', 'conf', 'nginx', 'env', 'yaml', 'yml', 'cron', '证书', 'cert', 'ssl', '防火墙', 'iptables', 'crontab', 'sed', 'chmod', 'chown', 'cp ', 'mv ', 'vim', 'vi ']],
  ['排查', ['排查', '定位', 'check', '日志', 'log', '监控', '告警', 'grep', 'tail', 'du ', 'df ', 'top', '内存', '磁盘', 'cpu', '慢查询']],
];

const state = {
  data: { tasks: [], mangas: [] },
  activeMangaId: localStorage.getItem('manga-log-active') || null,
  currentId: null,
  query: '',
  view: 'toc',          // toc | timeline | inbox | stats
  statusFilter: '全部',
  tagFilter: '',
  editTaskId: null,
  editMangaId: null,
  editEntryId: null,
  pendingImport: null,  // 待确认的导入数据
  snapshots: [],
  timelineFilters: { host: '', kind: '', result: '', from: '', to: '' },
  previewFile: null,
  comparisonCandidates: [],
  lightboxList: [],
  lightboxIndex: 0,
};

/* 条目表单内的附件草稿（提交时随格一起保存） */
let formImages = [];
let formTables = [];
let formFiles = [];
function genId(p) { return `${p}_${Date.now().toString(36)}${Math.random().toString(36).slice(2, 7)}`; }

/* 文档类型白名单（与服务端一致） */
const DOC_EXTS = new Set([
  'txt', 'log', 'md', 'markdown', 'csv', 'tsv', 'json', 'yaml', 'yml', 'xml', 'conf', 'cfg', 'ini', 'env',
  'pdf', 'doc', 'docx', 'xls', 'xlsx', 'ppt', 'pptx', 'odt', 'ods',
  'py', 'sh', 'bash', 'sql', 'js', 'ts', 'css', 'java', 'c', 'cpp', 'h', 'go', 'rs', 'ipynb',
  'zip', 'tar', 'gz', 'tgz', '7z', 'rar',
  'pem', 'key', 'csr', 'cnf',
]);
const DOC_MAX = 50 * 1024 * 1024;
function extOf(name) {
  const m = /\.([A-Za-z0-9]{1,10})$/.exec(String(name || ''));
  return m ? m[1].toLowerCase() : '';
}
function fmtSize(n) {
  if (!n) return '';
  if (n < 1024) return `${n}B`;
  if (n < 1048576) return `${(n / 1024).toFixed(1)}KB`;
  return `${(n / 1048576).toFixed(1)}MB`;
}

/* ---------------- 工具 ---------------- */
function esc(s) {
  return String(s ?? '')
    .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;').replace(/'/g, '&#39;');
}
function cnNum(n) {
  if (n <= 10) return CN_NUM[n];
  if (n < 20) return '十' + (n % 10 ? CN_NUM[n % 10] : '');
  if (n < 100) return CN_NUM[Math.floor(n / 10)] + '十' + (n % 10 ? CN_NUM[n % 10] : '');
  return String(n);
}
function pad(n) { return String(n).padStart(2, '0'); }
function nowLocal() {
  const d = new Date();
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}
function dayStr() {
  const d = new Date();
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}
function parseLocal(s) { return s ? new Date(String(s).replace(' ', 'T')) : null; }
function relDate(s) {
  const d = parseLocal(s);
  if (!d || isNaN(d)) return '';
  const diff = (Date.now() - d.getTime()) / 86400000;
  if (diff < 1 && d.getDate() === new Date().getDate()) return '今天';
  if (diff < 2) return '昨天';
  if (diff < 7) return `${Math.floor(diff)} 天前`;
  return `${pad(d.getMonth() + 1)}/${pad(d.getDate())}`;
}
function sortByTime(arr) {
  return [...arr].sort((a, b) => String(a.time).localeCompare(String(b.time)) || String(a.id).localeCompare(String(b.id)));
}
function weekStartTs(d) {
  const x = new Date(d);
  x.setHours(0, 0, 0, 0);
  x.setDate(x.getDate() - ((x.getDay() + 6) % 7));
  return x.getTime();
}
function detectKind(text) {
  const t = String(text).toLowerCase();
  for (const [kind, words] of KIND_HINTS) {
    if (words.some((w) => t.includes(w))) return kind;
  }
  return '其他';
}
function currentTask() { return state.data.tasks.find((t) => t.id === state.currentId) || null; }

async function api(path, opts = {}) {
  const route = path.split('?')[0];
  if (state.activeMangaId && (route === '/api/data' || route === '/api/import' || route === '/api/tasks' || route.startsWith('/api/tasks/'))) {
    const url = new URL(path, location.origin);
    if (!url.searchParams.has('mangaId')) url.searchParams.set('mangaId', state.activeMangaId);
    path = `${url.pathname}${url.search}`;
  }
  const res = await fetch(path, {
    headers: { 'Content-Type': 'application/json', 'X-Requested-With': 'manga-log' },
    ...opts,
  });
  if (res.status === 401) {
    /* 会话失效（多用户模式重启后必然发生）：回登录页重新认人 */
    location.href = '/login';
    throw new Error('未登录或会话已失效');
  }
  const body = await res.json().catch(() => ({}));
  if (!res.ok) throw new Error(body.error || `请求失败 (${res.status})`);
  return body;
}

let toastTimer = null;
function toast(text, sfx = '唰！') {
  const el = $('#toast');
  el.innerHTML = `<span class="sfx">${esc(sfx)}</span>${esc(text)}`;
  el.classList.remove('hidden');
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => el.classList.add('hidden'), 2000);
}

/* 剪贴板：安全上下文走异步 API，本地 http 场景退回 execCommand */
function copyText(text, msg = '已复制') {
  if (navigator.clipboard && window.isSecureContext) {
    navigator.clipboard.writeText(text).then(() => toast(msg, '拷！')).catch(() => fallbackCopy(text, msg));
  } else fallbackCopy(text, msg);
}
function fallbackCopy(text, msg) {
  const ta = document.createElement('textarea');
  ta.value = text; ta.style.position = 'fixed'; ta.style.opacity = '0';
  document.body.appendChild(ta); ta.select();
  try { document.execCommand('copy'); toast(msg, '拷！'); } catch (_) { /* 忽略 */ }
  ta.remove();
}

/* ---------------- 渲染：数据条 ---------------- */
function weekCount() {
  const cutoff = Date.now() - 7 * 86400000;
  let n = 0;
  for (const t of state.data.tasks)
    for (const e of t.entries) {
      const d = parseLocal(e.time);
      if (d && !isNaN(d) && d.getTime() >= cutoff) n++;
    }
  return n;
}
function renderTicker() {
  const panels = state.data.tasks.reduce((s, t) => s + t.entries.length, 0);
  const undone = state.data.tasks.reduce((s, t) => s + (t.todos || []).filter((x) => !x.done).length, 0);
  $('#statVol').textContent = `连载 ${state.data.tasks.length} 话`;
  $('#statPanel').textContent = `分格 ${panels} 格`;
  $('#statWeek').textContent = `本周改动 ${weekCount()} 次`;
  $('#statTodo').textContent = `遗留 ${undone} 件`;
}

/* ---------------- 渲染：目次 ---------------- */
function taskMatches(t, q) {
  if (!q) return true;
  const hay = [t.title, t.host, (t.tags || []).join(' '), t.note, ...t.entries.map((e) => `${e.title} ${e.detail} ${e.kind}`)].join(' ');
  return hay.toLowerCase().includes(q);
}
function visibleTasks() {
  const q = state.query.trim().toLowerCase();
  return state.data.tasks.filter((t) =>
    (state.statusFilter === '全部' || t.status === state.statusFilter)
    && (!state.tagFilter || (t.tags || []).includes(state.tagFilter))
    && taskMatches(t, q));
}
function renderToc() {
  const q = state.query.trim().toLowerCase();
  const list = $('#taskList');
  const tasks = visibleTasks();
  const filtersActive = q || state.statusFilter !== '全部' || state.tagFilter;
  if (filtersActive && tasks.length && !tasks.some((t) => t.id === state.currentId)) {
    state.currentId = tasks[0].id;
  }
  if (!tasks.length) {
    list.innerHTML = `<div class="toc-empty">${q || filtersActive ? '……没有符合条件的话。' : '还没有记录，点下面「开新话」！'}</div>`;
    return;
  }
  list.innerHTML = tasks.map((t) => {
    const idx = state.data.tasks.indexOf(t) + 1;
    const undone = (t.todos || []).filter((x) => !x.done).length;
    return `
    <div class="toc-item ${t.id === state.currentId ? 'active' : ''}" data-action="select-task" data-id="${esc(t.id)}">
      <div class="toc-line1"><span class="no">第${cnNum(idx)}话</span><span class="t">${esc(t.title)}</span></div>
      <div class="toc-line2">
        <span class="host">${esc(t.host || '—')}</span>
        <span class="leader"></span>
        <span class="meta">${t.entries.length} 格${undone ? ` · 待办${undone}` : ''} · ${esc(relDate(t.updatedAt))}</span>
      </div>
    </div>`;
  }).join('');
}
function renderChips() {
  const row = $('#statusChips');
  row.innerHTML = STATUSES.map((s) =>
    `<button type="button" class="fchip ${state.statusFilter === s ? 'active' : ''}" data-action="filter-status" data-status="${s}">${s}</button>`
  ).join('');
}
/* 目次的标签筛选行：只列当前漫画出现过的标签，附话数 */
function tagCounts() {
  const counts = new Map();
  for (const t of state.data.tasks) for (const g of t.tags || []) counts.set(g, (counts.get(g) || 0) + 1);
  return [...counts.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0], 'zh-CN'));
}
function renderTagChips() {
  const row = $('#tagChips');
  if (!row) return;
  const tags = tagCounts();
  row.classList.toggle('hidden', !tags.length);
  row.innerHTML = !tags.length ? '' :
    `<button type="button" class="fchip ${state.tagFilter === '' ? 'active' : ''}" data-action="filter-tag" data-tag="">全部</button>`
    + tags.map(([g, n]) => `<button type="button" class="fchip ${state.tagFilter === g ? 'active' : ''}" data-action="filter-tag" data-tag="${esc(g)}" title="${n} 话带此标签">#${esc(g)} · ${n}</button>`).join('');
}
function renderTabs() {
  const labels = { toc: ['tabToc', 'CONTENTS'], timeline: ['tabTimeline', 'TIMELINE'], inbox: ['tabInbox', 'INBOX'], stats: ['tabStats', 'EXTRA'] };
  for (const id of ['tabToc', 'tabTimeline', 'tabInbox', 'tabStats']) $(`#${id}`).classList.toggle('active', id === labels[state.view]?.[0]);
  $('#tocEn').textContent = labels[state.view]?.[1] || 'CONTENTS';
  const isToc = state.view === 'toc';
  $('#statusChips').classList.toggle('hidden', !isToc);
  $('#search').classList.toggle('hidden', !isToc);
  $('#taskList').classList.toggle('hidden', !isToc);
  $('#btnNewTask').classList.toggle('hidden', !isToc);
}

function renderMangaPicker() {
  const select = $('#mangaSelect');
  if (!select) return;
  const mangas = state.data.mangas || [];
  select.innerHTML = mangas.map((m) => `<option value="${esc(m.id)}">${esc(m.title)}${m.taskCount ? ` · ${m.taskCount} 话` : ' · 未开篇'}</option>`).join('');
  select.value = state.activeMangaId || mangas[0]?.id || '';
  select.disabled = mangas.length < 2;
  $('#btnRenameManga').disabled = !mangas.length;
  syncComicSelect(select);
}
function renderQuickSelect() {
  const sel = $('#quickTask');
  const tasks = state.data.tasks;
  if (!tasks.length) {
    sel.innerHTML = '<option>先开新话</option>';
    sel.disabled = true;
    return;
  }
  sel.disabled = false;
  sel.innerHTML = tasks.map((t, i) => {
    const short = t.title.length > 14 ? t.title.slice(0, 14) + '…' : t.title;
    return `<option value="${esc(t.id)}">记到 第${cnNum(i + 1)}话 · ${esc(short)}</option>`;
  }).join('');
  const ids = tasks.map((t) => t.id);
  sel.value = ids.includes(state.currentId) ? state.currentId : ids[0];
}

/* ---------------- 渲染：分格 ---------------- */
/* ---------------- 细节文本装饰：路径 / 文件名 / 命令 / 链接自动区分 ---------------- */
const DETAIL_FILE_EXT = 'conf|cfg|ini|env|yaml|yml|json|log|txt|md|markdown|csv|tsv|sql|xml|html|htm|css|js|mjs|ts|tsx|jsx|py|sh|bash|ipynb|pth|pt|ckpt|npz|npy|pem|key|cnf|service|gz|zip|tgz|7z|rar|pdf';
/* 单次扫描：链接 > 绝对路径/波浪线路径 > 带已知后缀的文件名，互不嵌套 */
const DETAIL_INLINE_RE = new RegExp(
  '(https?:\\/\\/[^\\s"\'<>（）【】，。；、！？]+)'
  + '|((?<=^|[\\s（(「【：:，,=])\\/(?:[\\w.@+\\-*]+\\/)*[\\w.@+\\-*]+(?:\\{[^}\\s]*\\})?\\/?)'
  + '|((?<=^|[\\s（(「【：:，,=])~\\/(?:[\\w.@+\\-]+\\/)*[\\w.@+\\-]+)'
  + '|(\\b[\\w.@+\\-*]+(?:\\/[\\w.@+\\-*]+)+\\.(?:' + DETAIL_FILE_EXT + ')\\b)'
  + '|(\\b[A-Za-z0-9_][\\w.-]*\\.(?:' + DETAIL_FILE_EXT + ')\\b)',
  'gi'
);
const DETAIL_CMD_RE = /^\s*(?:sudo|cd|cp|mv|rm|rmdir|mkdir|chmod|chown|git|npm|npx|node|pip3?|apt|apt-get|yum|systemctl|service|docker|kubectl|nginx|certbot|ssh|scp|rsync|find|grep|egrep|tail|head|du|df|tar|unzip|zip|curl|wget|vim|vi|sed|awk|echo|export|source|python3?|pkill|kill|nohup|crontab|ln|ls|ll|cat|conda|make|cmake|pytest|gzip)\b/;
const DETAIL_HAS_CJK = /[\u4e00-\u9fff]/;

function detailInline(escapedLine) {
  const re = new RegExp(DETAIL_INLINE_RE.source, 'gi');
  let out = '', last = 0, m;
  while ((m = re.exec(escapedLine))) {
    out += escapedLine.slice(last, m.index);
    const tok = m[0];
    if (m[1]) out += `<a class="t-url" href="${tok}" target="_blank" rel="noopener">${tok}</a>`;
    else if (m[2] || m[3] || m[4]) out += `<span class="t-path">${tok}</span>`;
    else out += `<span class="t-file">${tok}</span>`;
    last = m.index + tok.length;
  }
  return out + escapedLine.slice(last);
}

/* 细节 → 结构化 HTML：``` 围栏与纯命令行渲染为命令块，其余走行内装饰 */
function renderDetail(raw) {
  const escaped = esc(raw);
  const lines = escaped.split('\n');
  const parts = [];
  let buf = [];
  let fence = [];
  let inFence = false;
  const flush = () => { if (buf.length) { parts.push({ block: false, html: buf.join('\n') }); buf = []; } };
  for (const line of lines) {
    if (/^\s*```/.test(line)) {
      if (inFence) {
        flush();
        parts.push({ block: true, html: `<div class="t-cmd"><span class="cmd-prompt">⌨ </span>${fence.join('\n')}</div>` });
        fence = [];
      }
      inFence = !inFence;
      continue;
    }
    if (inFence) { fence.push(line); continue; }
    const bare = line.replace(/^\s*\$\s?/, '');
    const looksCmd = !DETAIL_HAS_CJK.test(line) && DETAIL_CMD_RE.test(bare) && /[\s/.\\-]/.test(bare);
    if (/^\s*\$\s*\S/.test(line) || /^\s*#/.test(line) || looksCmd) {
      flush();
      parts.push({ block: true, html: `<div class="t-cmd"><span class="cmd-prompt">$ </span>${detailInline(bare)}</div>` });
      continue;
    }
    buf.push(detailInline(line));
  }
  if (fence.length) {
    flush();
    parts.push({ block: true, html: `<div class="t-cmd"><span class="cmd-prompt">⌨ </span>${fence.join('\n')}</div>` });
  }
  flush();
  let html = '', prevBlock = false;
  for (const p of parts) {
    if (!html) html = p.html;
    else if (prevBlock || p.block) html += p.html;
    else html += '\n' + p.html;
    prevBlock = p.block;
  }
  return html;
}

function renderTable(tb) {
  if (!tb.cols.length && !tb.rows.length) return '';
  return `
  <div class="e-table-wrap">
    ${tb.title ? `<div class="e-table-title">⊞ ${esc(tb.title)}</div>` : ''}
    <table class="e-table">
      <thead><tr>${tb.cols.map((c) => `<th>${esc(c)}</th>`).join('')}</tr></thead>
      <tbody>${tb.rows.map((r) => `<tr>${tb.cols.map((c, i) => `<td>${esc(r[i] ?? '')}</td>`).join('')}</tr>`).join('')}</tbody>
    </table>
  </div>`;
}

function relationTarget(relation, tasks = state.data.tasks) {
  if (!relation) return null;
  const task = tasks.find((t) => t.id === relation.taskId);
  const entry = task?.entries.find((x) => x.id === relation.entryId);
  return task && entry ? { task, entry } : null;
}
function panelHTML(e, idx, taskId = state.currentId) {
  const sfx = KIND_SFX[e.kind] || '记';
  const badgeCls = e.result === '成功' ? 'ok' : e.result === '失败' ? 'bad' : 'pending';
  const badgeTxt = e.result === '成功' ? '成功' : e.result === '失败' ? '✗ 失败' : '待验证';
  const r = idx % 3;
  return `
  <article class="panel r${r + 1}" data-eid="${esc(e.id)}" data-tid="${esc(taskId || '')}">
    <div class="p-no">${pad(idx + 1)}</div>
    <div class="stamp roughen" title="${esc(e.kind)}">${esc(sfx)}</div>
    <div class="e-head"><span class="e-kind">${esc(e.kind)}</span><span class="e-time mono">${esc(e.time || '')}</span></div>
    <h4 class="e-title">${esc(e.title)}</h4>
    ${e.reviewAt ? `<div class="entry-review mono">⌁ 复核时间：${esc(e.reviewAt)}</div>` : ''}
    ${(() => { const target = relationTarget(e.relation); return target ? `<div class="entry-relation">${esc(e.relation.type || '相关')} → <button type="button" class="text-link" data-action="open-related" data-tid="${esc(target.task.id)}" data-eid="${esc(target.entry.id)}">${esc(target.entry.title)}</button><span class="mono"> · ${esc(target.task.host || target.task.title)}</span></div>` : ''; })()}
    ${e.detail ? `<div class="bubble">${renderDetail(e.detail)}</div>` : ''}
    ${(e.images && e.images.length) ? `
    <div class="e-imgs">
      ${e.images.map((im) => `
      <figure class="e-img" data-action="view-img" data-url="${esc(im.url)}" title="点击放大：${esc(im.name)}">
        <img src="${esc(im.url)}" alt="${esc(im.name)}" loading="lazy">
      </figure>`).join('')}
    </div>` : ''}
    ${(e.tables && e.tables.length) ? e.tables.map((tb) => renderTable(tb)).join('') : ''}
    ${(e.files && e.files.length) ? `
    <div class="e-files">
      ${e.files.map((fl) => `
      <a class="e-file rs" href="${esc(fl.url)}" download="${esc(fl.name)}"
         data-action="preview-file" data-url="${esc(fl.url)}" data-name="${esc(fl.name)}" data-size="${fl.size || 0}"
         title="点击预览：${esc(fl.name)}">
        <span class="ef-icon">📄</span>
        <span class="ef-name">${esc(fl.name)}</span>
        ${fl.size ? `<span class="ef-size mono">${esc(fmtSize(fl.size))}</span>` : ''}
      </a>`).join('')}
    </div>` : ''}
    <div class="e-foot">
      <span class="badge ${badgeCls}">${esc(badgeTxt)}</span>
      <span class="spacer"></span>
      <button class="btn-icon" data-action="copy-entry" data-id="${esc(e.id)}" data-tid="${esc(taskId || '')}" title="复制本格为 Markdown">⧉ 复</button>
      <button class="btn-icon" data-action="edit-entry" data-id="${esc(e.id)}" data-tid="${esc(taskId || '')}" title="修改此格">✎ 改</button>
      <button class="btn-icon" data-action="del-entry" data-id="${esc(e.id)}" data-tid="${esc(taskId || '')}" title="撕掉此格">✕ 撕</button>
    </div>
  </article>`;
}

function renderImgStrip() {
  const el = $('#imgStrip');
  if (!el) return;
  el.classList.toggle('hidden', !formImages.length);
  el.innerHTML = formImages.map((im) => `
    <span class="img-thumb">
      <img src="${esc(im.url)}" alt="${esc(im.name)}" title="${esc(im.name)}">
      <button type="button" class="thumb-del" data-form="del-img" data-url="${esc(im.url)}" title="移除这张图">✕</button>
    </span>`).join('');
}

function syncTablesFromDom() {
  document.querySelectorAll('[data-tf]').forEach((el) => {
    const tb = formTables[+el.dataset.ti];
    if (!tb) return;
    if (el.dataset.tf === 'title') tb.title = el.value;
    else if (el.dataset.tf === 'col') tb.cols[+el.dataset.ci] = el.value;
    else if (el.dataset.tf === 'cell') {
      const ri = +el.dataset.ri;
      tb.rows[ri] = tb.rows[ri] || [];
      tb.rows[ri][+el.dataset.ci] = el.value;
    }
  });
}

function renderTableEditors() {
  const el = $('#tableEditors');
  if (!el) return;
  el.innerHTML = formTables.map((tb, ti) => `
    <div class="tb-editor" data-ti="${ti}">
      <div class="tb-head">
        <input class="tb-title" data-tf="title" data-ti="${ti}" placeholder="表格标题（可选）" maxlength="120" value="${esc(tb.title)}">
        <button type="button" class="btn-icon" data-form="del-table" data-ti="${ti}" title="删除整张表">✕ 删表</button>
      </div>
      <div class="tb-scroll"><table class="tb-grid">
        <thead><tr>
          <th class="tb-rowno"></th>
          ${tb.cols.map((c, ci) => `<th><input data-tf="col" data-ti="${ti}" data-ci="${ci}" maxlength="40" placeholder="列${ci + 1}" value="${esc(c)}"></th>`).join('')}
          <th class="tb-op"><button type="button" data-form="del-col" data-ti="${ti}" title="删最后一列">−列</button></th>
        </tr></thead>
        <tbody>
          ${tb.rows.map((row, ri) => `<tr>
            <td class="tb-rowno mono">${ri + 1}</td>
            ${tb.cols.map((c, ci) => `<td><input data-tf="cell" data-ti="${ti}" data-ri="${ri}" data-ci="${ci}" maxlength="200" value="${esc(row[ci] || '')}"></td>`).join('')}
            <td class="tb-op"></td>
          </tr>`).join('')}
        </tbody>
      </table></div>
      <div class="tb-ops">
        <button type="button" class="btn-mini rs" data-form="add-row" data-ti="${ti}">＋ 行</button>
        <button type="button" class="btn-mini rs" data-form="add-col" data-ti="${ti}">＋ 列</button>
        <span class="hint mono">${tb.rows.length} 行 × ${tb.cols.length} 列</span>
      </div>
    </div>`).join('');
}

function renderFileStrip() {
  const el = $('#fileStrip');
  if (!el) return;
  el.classList.toggle('hidden', !formFiles.length);
  el.innerHTML = formFiles.map((fl) => `
    <span class="file-chip rs" data-action="preview-file" data-url="${esc(fl.url)}" data-name="${esc(fl.name)}" data-size="${fl.size || 0}" title="点击预览：${esc(fl.name)}">
      <span class="fc-icon">📄</span>
      <span class="fc-name">${esc(fl.name)}</span>
      ${fl.size ? `<span class="fc-size mono">${esc(fmtSize(fl.size))}</span>` : ''}
      <button type="button" class="fc-del" data-form="del-file" data-url="${esc(fl.url)}" title="移除">✕</button>
    </span>`).join('');
}

function entryFormHTML(t, nextNo) {
  const editing = state.editEntryId ? t.entries.find((e) => e.id === state.editEntryId) : null;
  const e = editing || { time: nowLocal().replace(' ', 'T'), kind: '配置', result: '待验证', title: '', detail: '', reviewAt: '', relation: null };
  formImages = editing ? (editing.images || []).map((x) => ({ ...x })) : [];
  formTables = editing ? (editing.tables || []).map((tb) => ({ ...tb, cols: [...tb.cols], rows: tb.rows.map((r) => [...r]) })) : [];
  formFiles = editing ? (editing.files || []).map((x) => ({ ...x })) : [];
  return `
  <section class="panel blank" id="entryFormPanel">
    <div class="p-no">${pad(nextNo)}</div>
    <form id="entryForm">
      <div class="e-head"><span class="e-kind">${editing ? '✎ 修改此格' : '下一笔记录'}</span></div>
      <div class="form-grid">
        <label class="lb">时间
          <input class="ipt" type="datetime-local" name="e-time" value="${esc(e.time ? String(e.time).replace(' ', 'T') : '')}">
        </label>
        <label class="lb">类型
          <select class="ipt" name="e-kind">
            ${KINDS.map((k) => `<option ${k === e.kind ? 'selected' : ''}>${k}</option>`).join('')}
          </select>
        </label>
        <label class="lb">结果
          <select class="ipt" name="e-result">
            ${['成功', '待验证', '失败'].map((r) => `<option ${r === e.result ? 'selected' : ''}>${r}</option>`).join('')}
          </select>
        </label>
      </div>
      <div class="form-grid relation-grid">
        <label class="lb">复核时间（可选）
          <input class="ipt" type="datetime-local" name="e-review" value="${esc(e.reviewAt ? String(e.reviewAt).replace(' ', 'T') : '')}">
        </label>
        <label class="lb relation-choice">关联记录
          <select class="ipt" name="e-related">
            <option value="">不关联</option>
            ${state.data.tasks.flatMap((task) => task.entries.map((entry) => ({ task, entry })))
              .filter(({ entry }) => entry.id !== editing?.id)
              .map(({ task, entry }) => `<option value="${esc(task.id)}|${esc(entry.id)}" ${e.relation?.taskId === task.id && e.relation?.entryId === entry.id ? 'selected' : ''}>${esc(task.host || task.title)} · ${esc(entry.title)}</option>`).join('')}
          </select>
        </label>
        <label class="lb">关系
          <select class="ipt" name="e-rel-type">
            ${['相关', '修复', '回滚', '验证'].map((kind) => `<option ${kind === (e.relation?.type || '相关') ? 'selected' : ''}>${kind}</option>`).join('')}
          </select>
        </label>
      </div>
      <label class="lb">改动了什么 <em>*</em>
        <input class="ipt" type="text" name="e-title" maxlength="120" placeholder="一句话概括这次改动" value="${esc(e.title)}" required>
      </label>
      <label class="lb">细节（命令、原因、影响…）
        <textarea class="ipt" name="e-detail" rows="3" maxlength="5000" placeholder="贴上关键命令 / 改了哪些文件 / 踩了什么坑（可直接 Ctrl+V 粘贴截图或把图片拖进来）">${esc(e.detail)}</textarea>
      </label>
      <div class="attach-row">
        <input type="file" id="imgFile" class="hidden" accept="image/png,image/jpeg,image/gif,image/webp" multiple>
        <input type="file" id="docFile" class="hidden" multiple>
        <button type="button" class="btn-mini rs" data-form="pick-img">📎 贴图</button>
        <button type="button" class="btn-mini rs" data-form="pick-doc">📄 文档</button>
        <button type="button" class="btn-mini rs" data-form="add-table">⊞ 表格</button>
        <button type="button" class="btn-mini rs" data-form="pick-server">🖥 服务器文件</button>
        <span class="hint mono">截图可 Ctrl+V 粘贴或拖进细节框；日志/配置/PDF 等文档也可挂进格子</span>
      </div>
      <div id="imgStrip" class="img-strip hidden"></div>
      <div id="fileStrip" class="file-strip hidden"></div>
      <div id="tableEditors" class="table-editors"></div>
      <div class="form-foot">
        ${editing ? '<button type="button" class="btn rs" data-action="cancel-edit-entry">不改了</button>' : ''}
        <span class="hint mono">Ctrl+Enter 记下</span>
        <button type="submit" class="btn primary rs roughen">${editing ? '保存修改' : '✎ 记下这一格！'}</button>
      </div>
    </form>
  </section>`;
}

function todosHTML(t) {
  const todos = t.todos || [];
  return `
  <div class="todos">
    <div class="todos-head">
      <span class="todos-title">遗留事项</span>
      <span class="mono todo-count">${todos.filter((x) => !x.done).length} 件待办</span>
    </div>
    <ul class="todo-list">
      ${todos.map((td) => `
        <li class="todo-item ${td.done ? 'done' : ''}">
          <input type="checkbox" class="todo-check" data-tid="${esc(td.id)}" ${td.done ? 'checked' : ''} title="标记完成">
          <span class="todo-text">${esc(td.text)}</span>
          <button class="btn-icon" data-action="del-todo" data-tid="${esc(td.id)}" title="删掉这条">✕</button>
        </li>`).join('')}
    </ul>
    <form id="todoForm" class="todo-form">
      <input class="ipt rs" type="text" maxlength="200" autocomplete="off" placeholder="＋ 还有啥没做完的？回车添加">
    </form>
  </div>`;
}

function renderTimeline(stage) {
  const all = state.data.tasks.flatMap((task) => task.entries.map((entry, index) => ({ task, entry, index })));
  const hosts = [...new Set(state.data.tasks.map((t) => t.host).filter(Boolean))].sort();
  const f = state.timelineFilters;
  const filtered = all.filter(({ task, entry }) => {
    const date = String(entry.time || '').slice(0, 10);
    return (!f.host || (f.host === '__empty__' ? !task.host : task.host === f.host)) && (!f.kind || entry.kind === f.kind) && (!f.result || entry.result === f.result)
      && (!f.from || date >= f.from) && (!f.to || date <= f.to);
  }).sort((a, b) => String(b.entry.time).localeCompare(String(a.entry.time)));
  stage.innerHTML = `
    <div class="ep-head global-head"><div class="vol-badge"><span class="num">時</span><span class="hua">全局记录</span></div>
      <div class="ep-info"><h2 class="ep-title">全局时间线</h2><div class="ep-meta"><span class="chip">${filtered.length} / ${all.length} 格</span><span class="chip">跨 ${state.data.tasks.length} 话</span></div></div>
    </div>
    <div class="timeline-filters">
      <label>主机<select class="ipt rs" data-timeline-filter="host"><option value="">全部主机</option><option value="__empty__" ${f.host === '__empty__' ? 'selected' : ''}>未填主机</option>${hosts.map((h) => `<option ${f.host === h ? 'selected' : ''}>${esc(h)}</option>`).join('')}</select></label>
      <label>类型<select class="ipt rs" data-timeline-filter="kind"><option value="">全部类型</option>${KINDS.map((k) => `<option ${f.kind === k ? 'selected' : ''}>${k}</option>`).join('')}</select></label>
      <label>结果<select class="ipt rs" data-timeline-filter="result"><option value="">全部结果</option>${['成功', '待验证', '失败'].map((r) => `<option ${f.result === r ? 'selected' : ''}>${r}</option>`).join('')}</select></label>
      <label>从<input class="ipt rs" type="date" data-timeline-filter="from" value="${esc(f.from)}"></label>
      <label>至<input class="ipt rs" type="date" data-timeline-filter="to" value="${esc(f.to)}"></label>
      <button class="btn rs" data-action="clear-timeline-filters">清空筛选</button>
    </div>
    <div class="divider" aria-hidden="true"></div>
    <div class="timeline-task-label mono">按时间倒序 · 点「编辑」可回到原任务</div>
    <div class="panels">${filtered.length ? filtered.map(({ task, entry, index }) => `<div class="timeline-item"><div class="timeline-context"><span>第${cnNum(state.data.tasks.indexOf(task) + 1)}话</span><strong>${esc(task.title)}</strong>${task.host ? `<span class="chip">${esc(task.host)}</span>` : ''}</div>${panelHTML(entry, index, task.id)}</div>`).join('') : '<div class="toc-empty">没有符合条件的记录。</div>'}</div>`;
}

function renderInbox(stage) {
  const pendingEntries = [];
  const pendingTodos = [];
  for (const task of state.data.tasks) {
    for (const entry of task.entries) if (entry.result === '待验证') pendingEntries.push({ task, entry });
    for (const todo of task.todos || []) if (!todo.done) pendingTodos.push({ task, todo });
  }
  pendingEntries.sort((a, b) => String(a.entry.reviewAt || a.entry.time).localeCompare(String(b.entry.reviewAt || b.entry.time)));
  stage.innerHTML = `
    <div class="ep-head global-head"><div class="vol-badge"><span class="num">?</span><span class="hua">复核队列</span></div>
      <div class="ep-info"><h2 class="ep-title">待验证收件箱</h2><div class="ep-meta"><span class="chip tag">${pendingEntries.length} 格待验证</span><span class="chip">${pendingTodos.length} 件未完成</span></div></div>
    </div>
    <p class="inbox-intro">集中查看尚未确认结果的改动与遗留事项，点开一项即可回到原话。</p>
    <div class="inbox-columns">
      <section><h3 class="stat-title">待验证改动</h3><div class="panels">${pendingEntries.length ? pendingEntries.map(({ task, entry }) => `<div class="inbox-record"><div class="timeline-context"><strong>${esc(task.title)}</strong>${task.host ? `<span class="chip">${esc(task.host)}</span>` : ''}<span class="mono">${esc(entry.reviewAt ? `复核 ${entry.reviewAt}` : entry.time || '')}</span><button class="btn-mini rs" data-action="open-inbox-entry" data-tid="${esc(task.id)}" data-eid="${esc(entry.id)}">打开原格 →</button></div>${panelHTML(entry, task.entries.indexOf(entry), task.id)}</div>`).join('') : '<div class="toc-empty">当前没有待验证改动。</div>'}</div></section>
      <section><h3 class="stat-title">未完成事项</h3><div class="inbox-todos">${pendingTodos.length ? pendingTodos.map(({ task, todo }) => `<article class="inbox-todo"><span class="todo-mark">□</span><div><strong>${esc(todo.text)}</strong><div class="mono">${esc(task.host || '未填主机')} · ${esc(task.title)}</div></div><button class="btn-mini rs" data-action="open-inbox-todo" data-tid="${esc(task.id)}">回到本话 →</button></article>`).join('') : '<div class="toc-empty">没有遗留事项。</div>'}</div></section>
    </div>`;
}

function renderStage() {
  const stage = $('#stage');
  if (state.view === 'stats') return renderStats(stage);
  if (state.view === 'timeline') return renderTimeline(stage);
  if (state.view === 'inbox') return renderInbox(stage);
  const t = currentTask();
  if (!t) {
    stage.innerHTML = `
    <div class="empty-hero">
      <div class="big">创刊号，虚位以待！</div>
      <p>把你在服务器上做的每一件事记成「一话」，<br>其中的每一次改动就是「一格」漫画。</p>
      <button class="btn primary rs roughen" data-action="new-task">＋ 开新话</button>
    </div>`;
    return;
  }

  const idx = state.data.tasks.indexOf(t) + 1;
  try { localStorage.setItem(`manga-log-current-${state.activeMangaId || ''}`, t.id); } catch (_) {}
  const entries = sortByTime(t.entries);
  const q = state.query.trim().toLowerCase();
  const shown = q ? entries.filter((e) => `${e.title} ${e.detail} ${e.kind} ${e.time}`.toLowerCase().includes(q)) : entries;
  const statusCls = t.status === '已完成' ? 's-done' : t.status === '搁置' ? 's-hold' : '';

  stage.innerHTML = `
  <div class="ep-head">
    <div class="vol-badge"><span class="num">${pad(idx)}</span><span class="hua">第${cnNum(idx)}话</span></div>
    <div class="ep-info">
      <h2 class="ep-title">${esc(t.title)}</h2>
      <div class="ep-meta">
        ${t.host ? `<span class="chip">▣ ${esc(t.host)}</span>` : ''}
        <span class="status-stamp ${statusCls}">${esc(t.status)}</span>
        ${(t.tags || []).map((g) => `<button type="button" class="chip tag" data-action="filter-tag" data-tag="${esc(g)}" title="目次里筛选带此标签的话">#${esc(g)}</button>`).join('')}
      </div>
    </div>
    <div class="ep-actions">
      <button class="btn rs" data-action="edit-task">✎ 编辑本话</button>
      <button class="btn rs" data-action="export-task" title="只导出这一话为 Markdown">⇩ 本话</button>
      <button class="btn danger rs" data-action="del-task" id="btnDelTask">✕ 整话撕掉</button>
    </div>
  </div>
  ${t.note ? `<p class="ep-note">${esc(t.note)}</p>` : ''}
  ${(t.todos || []).length || true ? todosHTML(t) : ''}
  <div class="divider" aria-hidden="true"></div>

  <div class="panels">
    ${shown.map((e, i) => panelHTML(e, entries.indexOf(e))).join('') ||
      `<div class="toc-empty" style="padding:20px 0">${q ? '本话没有匹配的格子。' : ''}</div>`}
    ${q ? '' : entryFormHTML(t, entries.length + 1)}
  </div>
  <div class="tsuzuku"><span class="jp">つづく</span><span class="en mono">TO BE CONTINUED · 未完待续</span></div>`;

  const form = $('#entryForm');
  if (form) {
    form.addEventListener('submit', onEntrySubmit);
    renderImgStrip();
    renderFileStrip();
    renderTableEditors();
    /* 表格输入即写回内存态。原先只在提交时 syncTablesFromDom，
     * 导致自动同步重绘 DOM 时，正在输入的单元格内容会被旧值覆盖。 */
    const tbEl = $('#tableEditors');
    if (tbEl) tbEl.addEventListener('input', (ev) => { if (ev.target.dataset.tf) syncTablesFromDom(); });
    /* 草稿自动暂存：打字即防抖写 localStorage，误关页面/刷新后回到原话原格还在 */
    form.addEventListener('input', scheduleDraftSave);
    restoreSavedDraft();
    if (!state.editEntryId) form.querySelector('input[name="e-title"]')?.focus({ preventScroll: true });
  }
  if (state.editEntryId) $('#entryFormPanel')?.scrollIntoView({ behavior: 'smooth', block: 'center' });
}

function renderAll() {
  renderTicker();
  renderMangaPicker();
  renderTabs();
  renderChips();
  renderTagChips();
  renderToc();
  renderQuickSelect();
  renderStage();
}

/* ---------------- 卷末统计 ---------------- */
function hatchRows(pairs, total) {
  const rows = pairs.filter(([k, n]) => n > 0);
  if (!rows.length || !total) return '<p class="stat-empty">暂无记录。</p>';
  const max = Math.max(...rows.map(([, n]) => n));
  return rows.map(([k, n]) => `
    <div class="hrow">
      <span class="hlabel" title="${esc(k)}">${esc(k)}</span>
      <div class="hbar-wrap"><div class="hbar ${n === max ? 'max' : ''}" style="width:${Math.max(3, Math.round((n / max) * 100))}%"></div></div>
      <span class="hval">${n}</span>
    </div>`).join('');
}

function renderStats(stage) {
  const tasks = state.data.tasks;
  const entries = [];
  tasks.forEach((t, i) => t.entries.forEach((e) => entries.push({ ...e, vol: i + 1 })));
  const total = entries.length;

  const weeks = [];
  const nowW = weekStartTs(new Date());
  for (let k = 11; k >= 0; k--) weeks.push({ start: nowW - k * 604800000, count: 0 });
  entries.forEach((e) => {
    const d = parseLocal(e.time);
    if (!d || isNaN(d)) return;
    const wk = weekStartTs(d);
    const bucket = weeks.find((w) => w.start === wk);
    if (bucket) bucket.count++;
  });
  const maxW = Math.max(1, ...weeks.map((w) => w.count));

  const byKind = {}; KINDS.forEach((k) => { byKind[k] = 0; });
  const byHost = {}; const byResult = { '成功': 0, '待验证': 0, '失败': 0 };
  entries.forEach((e) => {
    byKind[e.kind] = (byKind[e.kind] || 0) + 1;
    byResult[e.result] = (byResult[e.result] || 0) + 1;
  });
  // 主机归属要从话上取
  tasks.forEach((t) => {
    const h = t.host || '(未填主机)';
    byHost[h] = (byHost[h] || 0) + t.entries.length;
  });
  const kindPairs = KINDS.map((k) => [k, byKind[k]]);
  const hostPairs = Object.entries(byHost).sort((a, b) => b[1] - a[1]).slice(0, 6);
  const topTasks = [...tasks].sort((a, b) => b.entries.length - a.entries.length).slice(0, 3).filter((t) => t.entries.length);
  const undone = tasks.reduce((s, t) => s + (t.todos || []).filter((x) => !x.done).length, 0);

  stage.innerHTML = `
  <div class="ep-head">
    <div class="vol-badge"><span class="num">EX</span><span class="hua">卷末特典</span></div>
    <div class="ep-info">
      <h2 class="ep-title">卷末统计</h2>
      <div class="ep-meta">
        <span class="chip">共 ${tasks.length} 话 · ${total} 格</span>
        ${undone ? `<span class="chip tag">遗留 ${undone} 件</span>` : ''}
      </div>
    </div>
  </div>
  <div class="divider" aria-hidden="true"></div>
  <div class="stat-grid">
    <section class="stat-block r1">
      <h3 class="stat-title">近十二周改动节奏</h3>
      <div class="wchart">
        ${weeks.map((w) => {
          const d = new Date(w.start);
          return `<div class="wcol" title="${w.start} 起：${w.count} 次">
            <span class="wnum">${w.count || ''}</span>
            <div class="wbar ${w.count && w.count === maxW ? 'max' : ''}" style="height:${w.count ? Math.max(3, Math.round((w.count / maxW) * 88)) : 2}px"></div>
            <span class="wlabel">${d.getMonth() + 1}/${d.getDate()}</span>
          </div>`;
        }).join('')}
      </div>
    </section>
    <section class="stat-block r2">
      <h3 class="stat-title">改动类型分布</h3>
      ${hatchRows(kindPairs, total)}
    </section>
    <section class="stat-block r2">
      <h3 class="stat-title">服务器分布</h3>
      ${hatchRows(hostPairs, total)}
    </section>
    <section class="stat-block r1">
      <h3 class="stat-title">战果统计</h3>
      <div class="bignum-row">
        <div class="bignum"><div class="n">${byResult['成功']}</div><span class="badge ok">成功</span></div>
        <div class="bignum"><div class="n">${byResult['待验证']}</div><span class="badge pending">待验证</span></div>
        <div class="bignum"><div class="n">${byResult['失败']}</div><span class="badge bad">✗ 失败</span></div>
        <div class="bignum"><div class="n">${total ? Math.round((byResult['成功'] / total) * 100) : 0}<i>%</i></div><span class="pct mono">完结率</span></div>
      </div>
    </section>
    <section class="stat-block r3">
      <h3 class="stat-title">最勤快的话 TOP3</h3>
      ${topTasks.length ? topTasks.map((t) => {
        const idx = tasks.indexOf(t) + 1;
        const max = topTasks[0].entries.length || 1;
        return `<div class="hrow">
          <span class="hlabel" title="${esc(t.title)}">第${cnNum(idx)}话</span>
          <div class="hbar-wrap"><div class="hbar" style="width:${Math.max(3, Math.round((t.entries.length / max) * 100))}%"></div></div>
          <span class="hval">${t.entries.length} 格</span>
        </div>`;
      }).join('') : '<p class="stat-empty">暂无记录。</p>'}
    </section>
    <section class="stat-block r1" id="serverStatusBlock">
      <h3 class="stat-title">服务器状态</h3>
      <div id="serverStatusBody"><div class="pv-note">读取中…</div></div>
    </section>
    <section class="stat-block r2">
      <h3 class="stat-title">备份与搬家</h3>
      <p class="stat-note">每天首次改动会留快照；删除附件、导入覆盖和恢复前也会留一份。最多保留 40 份，自动快照连同记录引用的附件一起保存。手动 JSON 备份不含附件，搬家时需另拷 uploads/。</p>
      <div class="backup-row">
        <button class="btn rs" data-action="export-backup">⇩ 导出备份</button>
        <button class="btn rs" data-action="import-backup">⇒ 导入备份</button>
      </div>
      <div class="snapshot-list">
        <h4>自动快照 <span class="mono">${state.snapshots.length} / 40</span></h4>
        ${state.snapshots.length ? state.snapshots.map((snap) => `<div class="snapshot-row"><span class="mono">${esc(snap.createdAt.replace('T', ' ').slice(0, 16))}</span><span>${esc(({ daily: '每日', 'before-import': '导入前', 'before-restore': '恢复前', 'before-delete': '删除前', 'external-edit': '外部改动' })[snap.reason] || '自动')} · ${snap.tasks} 话</span><button class="btn-mini rs" data-action="restore-snapshot" data-id="${esc(snap.id)}">恢复</button></div>`).join('') : '<p class="stat-empty">第一次数据改动后会生成每日快照。</p>'}
      </div>
    </section>
  </div>
  <div class="tsuzuku"><span class="jp">つづく</span><span class="en mono">TO BE CONTINUED · 未完待续</span></div>`;
  fillServerStatus();
}

/* ---------------- 服务器状态面板（磁盘 / GPU） ---------------- */
async function fillServerStatus() {
  const body = $('#serverStatusBody');
  if (!body) return;
  try {
    const st = await api('/api/server-status');
    const bar = (pct, warn) => `<div class="ss-bar-wrap"><div class="ss-bar ${warn ? 'ss-bar-warn' : ''}" style="width:${Math.min(100, pct)}%"></div></div>`;
    const gpus = st.gpus.length ? st.gpus.map((g) => `
      <div class="ss-row"><span class="mono ss-key">GPU${g.index}</span>
        <div class="ss-bar-wrap">${bar(g.util)}</div>
        <span class="mono ss-val">${g.util}% · ${g.memUsed}/${g.memTotal}MB</span></div>
      <div class="ss-sub">${esc(g.name)}</div>`).join('') : '<div class="ss-sub">未检测到 NVIDIA GPU（或无驱动）</div>';
    const disks = st.disks.map((d) => {
      const warn = d.usePct >= 90;
      return `<div class="ss-row ${warn ? 'ss-warn' : ''}"><span class="mono ss-key">${esc(d.mount)}</span>
        ${bar(d.usePct, warn)}
        <span class="mono ss-val">${d.used} / ${d.size}（${d.usePct}%）</span></div>`;
    }).join('');
    body.innerHTML = `${st.gpus.length ? `<div class="ss-label">GPU</div>${gpus}` : ''}<div class="ss-label">磁盘</div>${disks || '<div class="ss-sub">（无磁盘信息）</div>'}`;
  } catch (e) {
    body.innerHTML = `<div class="pv-note">读取失败：${esc(e.message)}</div>`;
  }
}
async function handleFiles(files) {
  for (const file of Array.from(files)) {
    if (!/^image\/(png|jpe?g|gif|webp)$/.test(file.type)) { toast('仅支持 png / jpg / gif / webp', '啊！'); continue; }
    if (file.size > 6 * 1024 * 1024) { toast('图片需在 6MB 以内', '啊！'); continue; }
    if (formImages.length >= 9) { toast('一格最多 9 张图', '满！'); break; }
    try {
      const dataUrl = await new Promise((res, rej) => {
        const r = new FileReader();
        r.onload = () => res(r.result);
        r.onerror = () => rej(new Error('读取图片失败'));
        r.readAsDataURL(file);
      });
      const out = await api('/api/upload', { method: 'POST', body: JSON.stringify({ name: file.name, dataUrl }) });
      formImages.push({ id: genId('im'), url: out.url, name: out.name });
      renderImgStrip();
      scheduleDraftSave();
      toast('图贴上了！', '贴！');
    } catch (e) { toast(e.message, '啊！'); }
  }
}

async function handleDocFiles(files) {
  for (const file of Array.from(files)) {
    if (!DOC_EXTS.has(extOf(file.name))) { toast(`不支持 .${extOf(file.name) || '(无后缀)'} 类型`, '啊！'); continue; }
    if (file.size > DOC_MAX) { toast('文档需在 50MB 以内', '啊！'); continue; }
    if (formFiles.length >= 10) { toast('一格最多 10 个文档', '满！'); break; }
    try {
      const buf = await file.arrayBuffer();
      const res = await fetch(`/api/upload/doc?name=${encodeURIComponent(file.name)}`, { method: 'POST', headers: { 'X-Requested-With': 'manga-log' }, body: buf });
      const body = await res.json().catch(() => ({}));
      if (!res.ok) throw new Error(body.error || `上传失败 (${res.status})`);
      formFiles.push({ id: genId('f'), url: body.url, name: body.name, size: body.size });
      renderFileStrip();
      scheduleDraftSave();
      toast('文档挂上了！', '挂！');
    } catch (e) { toast(e.message, '啊！'); }
  }
}

function openLightbox(url, list) {
  state.lightboxList = Array.isArray(list) && list.length ? list : [url];
  state.lightboxIndex = Math.max(0, state.lightboxList.indexOf(url));
  const many = state.lightboxList.length > 1;
  $('#lbNav').classList.toggle('hidden', !many);
  $('#lbPrev').classList.toggle('hidden', !many);
  $('#lbNext').classList.toggle('hidden', !many);
  renderLightbox();
  $('#lightbox').classList.remove('hidden');
}
function renderLightbox() {
  $('#lightboxImg').src = state.lightboxList[state.lightboxIndex] || '';
  $('#lbCounter').textContent = `${state.lightboxIndex + 1} / ${state.lightboxList.length}`;
}
function lightboxStep(step) {
  if (!state.lightboxList.length) return;
  state.lightboxIndex = (state.lightboxIndex + step + state.lightboxList.length) % state.lightboxList.length;
  renderLightbox();
}
function closeLightbox() {
  $('#lightbox').classList.add('hidden');
  $('#lightboxImg').src = '';
  state.lightboxList = [];
}

/* ---------------- 附件预览 ---------------- */
const PREVIEW_TEXT_EXTS = new Set(['txt', 'log', 'md', 'markdown', 'json', 'yaml', 'yml', 'xml', 'conf', 'cfg', 'ini', 'env', 'csv', 'tsv', 'sql', 'py', 'sh', 'bash', 'js', 'ts', 'css', 'html', 'htm', 'pem', 'key', 'csr', 'cnf', 'service']);
const PREVIEW_TABLE_EXTS = new Set(['csv', 'tsv']);
const PREVIEW_TEXT_CAP = 5 * 1024 * 1024;   // 文本预览上限
const PREVIEW_LINE_CAP = 2000;              // 最多预览行数

async function previewFile(url, name, size) {
  const ext = extOf(name);
  state.previewFile = { url, name, size, ext };
  $('#pvName').textContent = name;
  $('#pvSize').textContent = fmtSize(size);
  const dl = $('#pvDownload');
  dl.href = url;
  dl.setAttribute('download', name);
  const body = $('#pvBody');
  body.innerHTML = '<div class="pv-note">读取中…</div>';
  $('#pvCompare').classList.toggle('hidden', !PREVIEW_TEXT_EXTS.has(ext) || size > PREVIEW_TEXT_CAP);
  $('#pvCompareFile').innerHTML = '';
  $('#pvCompareRun').disabled = true;
  if (PREVIEW_TEXT_EXTS.has(ext) && size <= PREVIEW_TEXT_CAP) {
    state.comparisonCandidates = state.data.tasks.flatMap((task) => task.entries.flatMap((entry) => (entry.files || [])
      .filter((file) => file.url !== url && PREVIEW_TEXT_EXTS.has(extOf(file.name)) && file.size <= PREVIEW_TEXT_CAP)
      .map((file) => ({ ...file, taskTitle: task.title, time: entry.time }))));
    $('#pvCompareFile').innerHTML = state.comparisonCandidates.length
      ? state.comparisonCandidates.map((file, i) => `<option value="${i}">${esc(file.name)} · ${esc(file.taskTitle)} · ${esc(file.time || '')}</option>`).join('')
      : '<option value="">没有其他可比较的文本附件</option>';
    $('#pvCompareRun').disabled = !state.comparisonCandidates.length;
  }
  /* 配对汇总：当前文件是 json（逐例 metrics / 汇总）时，提供与其他 json 的数值配对 */
  const metricsCandidates = ext === 'json' && size <= PREVIEW_TEXT_CAP
    ? state.data.tasks.flatMap((task) => task.entries.flatMap((entry) => (entry.files || [])
        .filter((file) => file.url !== url && extOf(file.name) === 'json' && file.size <= PREVIEW_TEXT_CAP)
        .map((file) => ({ ...file, taskTitle: task.title }))))
    : [];
  state.pairCandidates = metricsCandidates;
  $('#pvPair').classList.toggle('hidden', !metricsCandidates.length);
  $('#pvPairFile').innerHTML = metricsCandidates.map((file, i) => `<option value="${i}">${esc(file.name)} · ${esc(file.taskTitle)}</option>`).join('');
  $('#pvPairRun').disabled = !metricsCandidates.length;
  $('#filePreview').classList.remove('hidden');
  try {
    if (ext === 'pdf') {
      const r = await fetch(url);
      if (!r.ok) throw new Error(`读取失败 (${r.status})`);
      const blobUrl = URL.createObjectURL(await r.blob());
      body.innerHTML = `<embed class="pv-pdf" src="${blobUrl}" type="application/pdf">`;
      return;
    }
    if (!PREVIEW_TEXT_EXTS.has(ext)) {
      body.innerHTML = '<div class="pv-note">该类型暂不支持在线预览，请点「⇩ 下载」查看。</div>';
      return;
    }
    if (size > PREVIEW_TEXT_CAP) {
      body.innerHTML = `<div class="pv-note">文件 ${fmtSize(size)} 超过 5MB，请点「⇩ 下载」查看。</div>`;
      return;
    }
    const text = await (await fetch(url)).text();
    if (PREVIEW_TABLE_EXTS.has(ext)) {
      const rows = text.replace(/\r/g, '').split('\n').filter((l) => l.trim().length).slice(0, 500);
      if (!rows.length) { body.innerHTML = '<div class="pv-note">（空文件）</div>'; return; }
      const sep = ext === 'tsv' ? '\t' : ',';
      const parse = (line) => line.split(sep).map((c) => c.replace(/^"(.*)"$/, '$1'));
      const head = parse(rows[0]);
      const bodyRows = rows.slice(1).map(parse);
      body.innerHTML = `<div class="pv-count mono">共 ${bodyRows.length} 行 × ${head.length} 列${rows.length >= 500 ? '（仅预览前 500 行）' : ''}</div>`
        + `<table class="e-table"><thead><tr>${head.map((h) => `<th>${esc(h)}</th>`).join('')}</tr></thead>`
        + `<tbody>${bodyRows.map((r2) => `<tr>${head.map((_, i) => `<td>${esc(r2[i] ?? '')}</td>`).join('')}</tr>`).join('')}</tbody></table>`;
      return;
    }
    const lines = text.replace(/\r/g, '').split('\n');
    const shown = lines.slice(0, PREVIEW_LINE_CAP).map(esc).join('\n');
    const counter = lines.length > PREVIEW_LINE_CAP
      ? `前 ${PREVIEW_LINE_CAP} 行 / 共 ${lines.length} 行`
      : `共 ${lines.length} 行`;
    /* 训练日志自动解析（epoch x/y loss z 格式） */
    const parsedLog = /\bepoch\s+\d+\s*\//.test(text) ? parseTrainingLog(text) : null;
    let parseCard = '';
    if (parsedLog && parsedLog.found) {
      const rows = [
        ['已完成轮数', `${parsedLog.done} / ${parsedLog.planned}`],
        ['best loss', parsedLog.best ? `${parsedLog.best.loss} @ epoch ${parsedLog.best.epoch}` : '—'],
        ['最终 loss', parsedLog.final ? `${parsedLog.final.loss} @ epoch ${parsedLog.final.epoch}` : '—'],
        ['训练完成', parsedLog.complete ? '✓ 是' : '✗ 未完成'],
      ];
      if (parsedLog.outDir) rows.push(['产物目录', parsedLog.outDir]);
      const table = { id: genId('tb'), title: `训练日志解析：${name.slice(0, 30)}`, cols: ['项目', '数值'], rows };
      parseCard = `<div class="pv-parse-card"><div class="pv-parse-head">📈 识别到训练日志</div>`
        + `<table class="e-table">${rows.map((r2) => `<tr><th>${esc(r2[0])}</th><td>${esc(r2[1])}</td></tr>`).join('')}</table>`
        + ($('#entryForm') ? `<button id="pvInsertLog" class="btn primary rs" type="button">⬇ 插入为本格表格</button>` : '')
        + `</div>`;
      state.pendingParsedTable = table;
    }
    body.innerHTML = (parseCard || '')
      + `<div class="pv-count mono">${counter}</div><pre class="pv-text">${shown || '（空文件）'}</pre>`;
    $('#pvInsertLog')?.addEventListener('click', () => {
      const t2 = state.pendingParsedTable;
      if (!t2) return;
      formTables.push({ ...t2, cols: [...t2.cols], rows: t2.rows.map((r) => [...r]) });
      renderTableEditors();
      closePreview();
      $('#tableEditors')?.scrollIntoView({ behavior: 'smooth', block: 'center' });
      toast('解析表已插入！', '成！');
    });
  } catch (e) {
    body.innerHTML = `<div class="pv-note">预览失败：${esc(e.message)}</div>`;
  }
}
function closePreview() {
  $('#filePreview').classList.add('hidden');
  $('#pvBody').innerHTML = '';
  $('#pvCompare').classList.add('hidden');
  state.previewFile = null;
}

function diffRows(before, after) {
  const a = before.replace(/\r/g, '').split('\n').slice(0, PREVIEW_LINE_CAP);
  const b = after.replace(/\r/g, '').split('\n').slice(0, PREVIEW_LINE_CAP);
  if (a.length * b.length > 1500000) {
    const n = Math.max(a.length, b.length);
    return Array.from({ length: n }, (_, i) => {
      if (a[i] === b[i]) return { a: a[i] ?? '', b: b[i] ?? '', an: i < a.length ? i + 1 : '', bn: i < b.length ? i + 1 : '', sa: 'same', sb: 'same' };
      return { a: a[i] ?? '', b: b[i] ?? '', an: i < a.length ? i + 1 : '', bn: i < b.length ? i + 1 : '', sa: i >= a.length ? 'empty' : 'remove', sb: i >= b.length ? 'empty' : 'add' };
    });
  }
  const width = b.length + 1;
  const dp = new Uint32Array((a.length + 1) * width);
  for (let i = a.length - 1; i >= 0; i--) for (let j = b.length - 1; j >= 0; j--) {
    const at = i * width + j;
    dp[at] = a[i] === b[j] ? dp[(i + 1) * width + j + 1] + 1 : Math.max(dp[(i + 1) * width + j], dp[i * width + j + 1]);
  }
  const rows = []; let i = 0, j = 0, oldNo = 1, newNo = 1;
  while (i < a.length || j < b.length) {
    if (i < a.length && j < b.length && a[i] === b[j]) {
      rows.push({ a: a[i], b: b[j], an: oldNo++, bn: newNo++, sa: 'same', sb: 'same' }); i++; j++;
    } else if (i < a.length && (j >= b.length || dp[(i + 1) * width + j] >= dp[i * width + j + 1])) {
      rows.push({ a: a[i], b: '', an: oldNo++, bn: '', sa: 'remove', sb: 'empty' }); i++;
    } else {
      rows.push({ a: '', b: b[j], an: '', bn: newNo++, sa: 'empty', sb: 'add' }); j++;
    }
  }
  return rows;
}
async function comparePreviewFiles() {
  const current = state.previewFile;
  const other = state.comparisonCandidates[+$('#pvCompareFile').value];
  if (!current || !other) return;
  const body = $('#pvBody');
  body.innerHTML = '<div class="pv-note">正在对照两个版本…</div>';
  try {
    const [a, b] = await Promise.all([fetch(current.url), fetch(other.url)]);
    if (!a.ok || !b.ok) throw new Error('有一个文件读取失败');
    const [before, after] = await Promise.all([a.text(), b.text()]);
    const rows = diffRows(before, after);
    body.innerHTML = `<div class="pv-diff-labels"><strong>当前：${esc(current.name)}</strong><strong>对照：${esc(other.name)}</strong></div><div class="pv-diff">${rows.map((row) => `<div class="pv-diff-row"><div class="pv-diff-cell ${row.sa}"><span class="pv-ln">${row.an ?? ''}</span><code>${esc(row.a)}</code></div><div class="pv-diff-cell ${row.sb}"><span class="pv-ln">${row.bn ?? ''}</span><code>${esc(row.b)}</code></div></div>`).join('')}</div>${before.split('\n').length > PREVIEW_LINE_CAP || after.split('\n').length > PREVIEW_LINE_CAP ? '<p class="pv-note">差异仅展示前 2000 行。</p>' : ''}`;
  } catch (e) { body.innerHTML = `<div class="pv-note">比较失败：${esc(e.message)}</div>`; }
}

/* ---------------- 配对汇总：两份逐例 metrics / 汇总的数值配对 ---------------- */
function parseNumericJson(json) {
  if (Array.isArray(json)) {
    const rows = json.filter((r) => r && typeof r === 'object' && !Array.isArray(r));
    if (!rows.length) return null;
    const keys = new Set();
    for (const r of rows) for (const [k, v] of Object.entries(r)) if (typeof v === 'number' && Number.isFinite(v)) keys.add(k);
    if (!keys.size) return null;
    const nameKey = rows.some((r) => typeof r.name === 'string' && r.name) ? 'name'
      : (rows.some((r) => typeof r.case === 'string') ? 'case' : null);
    return { type: 'cases', rows, keys: [...keys], nameKey };
  }
  if (json && typeof json === 'object') {
    const metrics = Object.entries(json)
      .filter(([k, v]) => v && typeof v === 'object' && Number.isFinite(v.mean))
      .map(([k, v]) => ({ key: k, mean: v.mean, std: Number.isFinite(v.std) ? v.std : 0 }));
    return metrics.length ? { type: 'summary', metrics } : null;
  }
  return null;
}
function metricStats(values) {
  const n = values.length;
  const mean = values.reduce((s, v) => s + v, 0) / n;
  const std = Math.sqrt(values.reduce((s, v) => s + (v - mean) ** 2, 0) / n); // 总体标准差（同 np.std 默认）
  return { mean, std, n };
}
async function runMetricsPair() {
  const current = state.previewFile;
  const other = state.pairCandidates[+$('#pvPairFile').value];
  if (!current || !other) return;
  const body = $('#pvBody');
  body.innerHTML = '<div class="pv-note">正在读取并配对…</div>';
  try {
    const [ta, tb] = await Promise.all([(await fetch(current.url)).text(), (await fetch(other.url)).text()]);
    const pa = parseNumericJson(JSON.parse(ta));
    const pb = parseNumericJson(JSON.parse(tb));
    if (!pa || !pb) throw new Error('有一份文件不是可识别的数值表（需要逐例数组或 mean/std 汇总对象）');
    const LOWER_BETTER = /^(rmse|time|loss|err|error|mse|mae)/i;
    const fmtMs = (mean, std) => `${mean.toFixed(4)} ± ${std.toFixed(4)}`;
    let cols, rows, note;
    if (pa.type === 'cases' && pb.type === 'cases') {
      const keys = pa.keys.filter((k) => pb.keys.includes(k));
      if (!keys.length) throw new Error('两份文件没有共同的数值字段');
      const nameKey = pa.nameKey && pb.nameKey ? 'name' : null;
      const mapB = new Map(pb.rows.map((r, i) => [nameKey ? String(r[pb.nameKey]) : String(i), r]));
      const deltas = {}; keys.forEach((k) => { deltas[k] = []; });
      let matched = 0, skipped = 0;
      for (const r of pa.rows) {
        const b = mapB.get(nameKey ? String(r[pa.nameKey]) : String(pa.rows.indexOf(r)));
        if (!b) { skipped++; continue; }
        matched++;
        for (const k of keys) {
          const dv = Number(b[k]) - Number(r[k]);
          if (Number.isFinite(dv)) deltas[k].push(dv);
        }
      }
      if (!matched) throw new Error('两份文件没有可配对的病例（name 对不上）');
      cols = ['指标', 'A 均值±标准差', 'B 均值±标准差', '配对差 (B−A)', 'B 改善 / 配对'];
      rows = keys.map((k) => {
        const aStats = metricStats(pa.rows.map((r) => Number(r[k])).filter(Number.isFinite));
        const bStats = metricStats(pb.rows.map((r) => Number(r[k])).filter(Number.isFinite));
        const ds = deltas[k];
        const dMean = ds.length ? ds.reduce((s, v) => s + v, 0) / ds.length : NaN;
        const lower = LOWER_BETTER.test(k);
        const improved = ds.filter((v) => (lower ? v < 0 : v > 0)).length;
        return [k + (lower ? '（越低越好）' : ''), fmtMs(aStats.mean, aStats.std), fmtMs(bStats.mean, bStats.std),
          Number.isFinite(dMean) ? (dMean >= 0 ? '+' : '') + dMean.toFixed(4) : '—',
          `${improved} / ${matched}`];
      });
      note = `配对 ${matched} 例${skipped ? `，跳过 ${skipped} 例（只在一侧出现）` : ''}`;
      if (!nameKey) note += '（文件无 name 字段，按顺序配对）';
    } else {
      const toSummary = (p) => p.type === 'summary' ? p.metrics
        : p.keys.map((k) => { const st = metricStats(p.rows.map((r) => Number(r[k])).filter(Number.isFinite)); return { key: k, mean: st.mean, std: st.std }; });
      const ma = toSummary(pa); const mb = toSummary(pb);
      const keys = ma.filter((m) => mb.some((x) => x.key === m.key)).map((m) => m.key);
      if (!keys.length) throw new Error('两份文件没有共同的数值字段');
      const get = (list, k) => list.find((x) => x.key === k);
      cols = ['指标', 'A 均值±标准差', 'B 均值±标准差', '均值差 (B−A)'];
      rows = keys.map((k) => {
        const x = get(ma, k); const y = get(mb, k);
        const d = y.mean - x.mean;
        const lower = LOWER_BETTER.test(k);
        return [k + (lower ? '（越低越好）' : ''), fmtMs(x.mean, x.std), fmtMs(y.mean, y.std), (d >= 0 ? '+' : '') + d.toFixed(4)];
      });
      note = '汇总级对比（至少一侧是 summary，无逐例改善计数）';
    }
    const table = { id: genId('tb'), title: `配对汇总：${current.name.slice(0, 24)} vs ${other.name.slice(0, 24)}`, cols, rows };
    body.innerHTML = `<div class="pv-count mono">${esc(note)}</div>`
      + `<table class="e-table"><thead><tr>${cols.map((c) => `<th>${esc(c)}</th>`).join('')}</tr></thead>`
      + `<tbody>${rows.map((r2) => `<tr>${r2.map((c) => `<td>${esc(c)}</td>`).join('')}</tr>`).join('')}</tbody></table>`
      + `<div class="form-foot">${$('#entryForm') ? '<button id="pvInsertTable" class="btn primary rs" type="button">⬇ 插入为本格表格</button>' : '<span class="pv-note">在本格表单里打开预览，才能把表插进格子</span>'}</div>`;
    $('#pvInsertTable')?.addEventListener('click', () => {
      formTables.push({ ...table, cols: [...table.cols], rows: table.rows.map((r) => [...r]) });
      renderTableEditors();
      closePreview();
      $('#tableEditors')?.scrollIntoView({ behavior: 'smooth', block: 'center' });
      toast('对比表已插入，可继续修改！', '成！');
    });
  } catch (e) {
    body.innerHTML = `<div class="pv-note">配对失败：${esc(e.message)}</div>`;
  }
}

/* ---------------- 服务器文件直取 ---------------- */
function openServerPicker() {
  const pathInput = $('#spPath');
  if (!pathInput.value) pathInput.value = localStorage.getItem('manga-sp-path') || '/data2/lee';
  $('#serverPicker').classList.remove('hidden');
  listServerDir();
}
async function listServerDir() {
  const p = $('#spPath').value.trim();
  const list = $('#spList');
  const absolute = p.startsWith('/') || /^[A-Za-z]:[\/\\]/.test(p);
  if (!absolute) { list.innerHTML = '<div class="pv-note">需要绝对路径（/… 或 C:/…）</div>'; return; }
  list.innerHTML = '<div class="pv-note">读取中…</div>';
  try {
    const res = await api('/api/server-list', { method: 'POST', body: JSON.stringify({ path: p }) });
    localStorage.setItem('manga-sp-path', res.path);
    state.spDir = res.path;
    state.spItems = res.items;
    state.spSelected = new Set();
    renderServerList();
  } catch (e) { list.innerHTML = `<div class="pv-note">${esc(e.message)}</div>`; }
}
function renderServerList() {
  const items = state.spItems || [];
  const list = $('#spList');
  if (!items.length) { list.innerHTML = '<div class="pv-note">（空目录）</div>'; $('#spCount').textContent = ''; return; }
  list.innerHTML = items.map((item) => {
    const full = (state.spDir === '/' ? '' : state.spDir) + '/' + item.name;
    if (item.dir) {
      return `<div class="sp-row sp-dir" data-action="sp-cd" data-path="${esc(full)}" title="进入目录"><span>📁</span><span class="sp-name">${esc(item.name)}</span></div>`;
    }
    return `<label class="sp-row"><input type="checkbox" class="sp-check" data-path="${esc(full)}" data-name="${esc(item.name)}" data-size="${item.size || 0}">
      <span>📄</span><span class="sp-name">${esc(item.name)}</span><span class="fc-size mono">${fmtSize(item.size)}</span></label>`;
  }).join('');
  $('#spCount').textContent = `${items.filter((x) => x.dir).length} 个目录 / ${items.filter((x) => !x.dir).length} 个文件`;
}
async function attachSelectedServerFiles() {
  const picks = [...document.querySelectorAll('.sp-check:checked')].map((c) => ({ path: c.dataset.path, name: c.dataset.name }));
  if (!picks.length) { toast('先勾选要挂的文件', '嗯？'); return; }
  try {
    const res = await api('/api/server-attach', { method: 'POST', body: JSON.stringify({ paths: picks.map((p) => p.path) }) });
    let img = 0, doc = 0;
    const errs = [];
    for (const file of res.files) {
      if (file.error) { errs.push(`${file.name}：${file.error}`); continue; }
      if (file.image && formImages.length < 9) { formImages.push({ id: genId('im'), url: file.url, name: file.name }); img++; }
      else if (!file.image && formFiles.length < 10) { formFiles.push({ id: genId('f'), url: file.url, name: file.name, size: file.size }); doc++; }
      else errs.push(`${file.name}：数量已达上限`);
    }
    renderImgStrip();
    renderFileStrip();
    scheduleDraftSave();
    closeServerPicker();
    toast(`已挂上：图 ${img} · 文档 ${doc}` + (errs.length ? `（${errs.length} 个跳过）` : ''), '挂！');
  } catch (e) { toast(e.message, '啊！'); }
}
function closeServerPicker() { $('#serverPicker').classList.add('hidden'); }

/* ---------------- 训练日志解析 ---------------- */
function parseTrainingLog(text) {
  const epochs = [];
  const re = /epoch\s+(\d+)\s*\/\s*(\d+)\s*[,:：]?\s*loss\s+([\d.]+(?:[eE][+-]?\d+)?)/gi;
  let m;
  while ((m = re.exec(text))) epochs.push({ epoch: +m[1], total: +m[2], loss: +m[3] });
  const complete = /training complete[:：]?\s*(\S*)/i.exec(text);
  let best = null;
  for (const e of epochs) if (!best || e.loss < best.loss) best = e;
  const last = epochs[epochs.length - 1] || null;
  return {
    found: epochs.length > 0,
    count: epochs.length,
    done: last ? last.epoch : 0,
    planned: last ? last.total : 0,
    best: best ? { epoch: best.epoch, loss: best.loss } : null,
    final: last ? { epoch: last.epoch, loss: last.loss } : null,
    complete: Boolean(complete),
    outDir: complete ? complete[1] : '',
  };
}

/* ---------------- 交互：刷新（支持草稿保护） ---------------- */
let lastDataHash = '';
function captureEntryDraft() {
  const form = $('#entryForm');
  if (!form) return null;
  return {
    taskId: state.currentId,
    editId: state.editEntryId,
    v: {
      time: form['e-time'].value,
      kind: form['e-kind'].value,
      result: form['e-result'].value,
      title: form['e-title'].value,
      detail: form['e-detail'].value,
      reviewAt: form['e-review'].value,
      relation: form['e-related'].value,
      relationType: form['e-rel-type'].value,
    },
    images: formImages,
    tables: formTables,
    files: formFiles,
  };
}
function restoreEntryDraft(d) {
  if (!d) return;
  const form = $('#entryForm');
  if (!form || state.currentId !== d.taskId || state.editEntryId !== d.editId) return;
  const empty = !d.editId && !d.v.title && !d.v.detail && !d.v.reviewAt && !d.v.relation && !d.images.length && !d.tables.length && !d.files.length;
  if (empty) return;
  form['e-time'].value = d.v.time;
  form['e-kind'].value = d.v.kind;
  form['e-result'].value = d.v.result;
  form['e-title'].value = d.v.title;
  form['e-detail'].value = d.v.detail;
  form['e-review'].value = d.v.reviewAt;
  form['e-related'].value = d.v.relation;
  form['e-rel-type'].value = d.v.relationType;
  formImages = d.images;
  formTables = d.tables;
  formFiles = d.files;
  renderImgStrip();
  renderFileStrip();
  renderTableEditors();
}

/* ---------------- 草稿本地暂存 ----------------
 * 与 refresh({ preserve }) 的内存草稿互补：那套只挡 10 秒轮询重绘，
 * 这套挡的是刷新页面/误关标签页——重新打开同一话同一格时自动回填。
 * 单个 key 只存最近一份草稿，切到别的话不覆盖（上下文对不上就不回填）。 */
const DRAFT_KEY = 'manga-log-entry-draft';
let draftSaveTimer = null;
function scheduleDraftSave() {
  clearTimeout(draftSaveTimer);
  draftSaveTimer = setTimeout(saveEntryDraft, 400);
}
function saveEntryDraft() {
  const form = $('#entryForm');
  if (!form) return;
  const d = captureEntryDraft();
  if (!d) return;
  d.mangaId = state.activeMangaId || '';
  d.savedAt = Date.now();
  try { localStorage.setItem(DRAFT_KEY, JSON.stringify(d)); } catch (_) { /* 隐私模式等存不进就算了 */ }
}
function clearEntryDraft() {
  clearTimeout(draftSaveTimer);
  try { localStorage.removeItem(DRAFT_KEY); } catch (_) {}
}
function restoreSavedDraft() {
  const form = $('#entryForm');
  if (!form) return;
  let d;
  try { d = JSON.parse(localStorage.getItem(DRAFT_KEY) || 'null'); } catch (_) { return; }
  if (!d) return;
  if (d.mangaId !== (state.activeMangaId || '') || d.taskId !== state.currentId || (d.editId || '') !== (state.editEntryId || '')) return;
  const v = d.v || {};
  const images = d.images || [];
  const tables = d.tables || [];
  const files = d.files || [];
  const empty = !d.editId && !v.title && !v.detail && !v.reviewAt && !v.relation && !images.length && !tables.length && !files.length;
  if (empty) return;
  form['e-time'].value = v.time || '';
  form['e-kind'].value = v.kind || '配置';
  form['e-result'].value = v.result || '待验证';
  form['e-title'].value = v.title || '';
  form['e-detail'].value = v.detail || '';
  form['e-review'].value = v.reviewAt || '';
  form['e-related'].value = v.relation || '';
  form['e-rel-type'].value = v.relationType || '相关';
  formImages = images;
  formTables = tables;
  formFiles = files;
  renderImgStrip();
  renderFileStrip();
  renderTableEditors();
}

async function refresh(opts = {}) {
  const [fresh, backupInfo] = await Promise.all([api('/api/data'), api('/api/snapshots')]);
  const draft = opts.preserve ? captureEntryDraft() : null;
  state.data = fresh;
  state.activeMangaId = fresh.activeMangaId || fresh.mangas?.[0]?.id || null;
  if (state.activeMangaId) localStorage.setItem('manga-log-active', state.activeMangaId);
  state.snapshots = backupInfo.snapshots || [];
  if (!currentTask() && state.data.tasks.length) {
    /* 回到上次读到的话（按漫画分别记）：误关页面/刷新后草稿才能跟着回到原格 */
    let saved = '';
    try { saved = localStorage.getItem(`manga-log-current-${state.activeMangaId || ''}`) || ''; } catch (_) {}
    state.currentId = state.data.tasks.some((t) => t.id === saved) ? saved : state.data.tasks[0].id;
  }
  if (!state.data.tasks.length) state.currentId = null;
  renderAll();
  if (draft) restoreEntryDraft(draft);
  lastDataHash = JSON.stringify(state.data);
}

/* 自动同步：每 10 秒检测一次外部写入（如 import_history.py 录入），有变化就重绘并保留草稿 */
function isUserTypingInStage() {
  const el = document.activeElement;
  if (!el) return false;
  const stage = $('#stage');
  if (stage && stage.contains(el) && el.closest?.('.comic-date.open, .comic-select.open')) return true;
  if (!/^(INPUT|TEXTAREA|SELECT)$/.test(el.tagName)) return false;
  return !!(stage && stage.contains(el));
}
setInterval(async () => {
  try {
    /* 用户正在格子里打字时，本轮跳过重绘：
     * renderStage 会整块重建 #stage 的 DOM，任何未被 captureEntryDraft
     * 覆盖的控件（尤其是表格单元格）都会丢输入。 */
    if (isUserTypingInStage()) return;
    const fresh = await api('/api/data');
    if (JSON.stringify(fresh) === lastDataHash) return;
    await refresh({ preserve: true });
    toast('记录有更新，已同步', '新！');
  } catch (_) { /* 服务暂时不可达就等下一轮 */ }
}, 10000);

/* ---------------- 交互：任务弹窗 ---------------- */
function openTaskModal(task) {
  state.editTaskId = task ? task.id : null;
  $('#modalTitle').textContent = task ? '改这一话' : '新的一话';
  const f = $('#taskForm');
  f.title.value = task ? task.title : '';
  f.host.value = task ? task.host : '';
  f.status.value = task ? task.status : '进行中';
  f.tags.value = task ? (task.tags || []).join(', ') : '';
  f.note.value = task ? task.note : '';
  $('.template-field').classList.toggle('hidden', !!task);
  f.template.value = 'custom';
  syncComicSelect(f.template);
  syncComicSelect(f.status);
  f.title.dataset.templateTitle = '';
  updateTemplateHint(f.template.value, f);
  $('#modal').classList.remove('hidden');
  f.title.focus();
}

const WORKFLOW_TEMPLATES = {
  release: { title: '应用版本发布', label: '版本发布', todos: ['确认变更范围与审批', '确认备份和回滚方案', '灰度发布并观察指标', '全量发布后复核告警'] },
  certificate: { title: 'TLS 证书续期', label: '证书续期', todos: ['核对域名与到期时间', '备份现有证书和配置', '续期并执行配置校验', 'reload 后验证证书链'] },
  database: { title: '数据库变更', label: '数据库变更', todos: ['确认变更 SQL 与影响范围', '检查备份和恢复方案', '低峰执行并观察锁与耗时', '核对数据结果与慢查询'] },
  incident: { title: '线上故障排查', label: '故障排查', todos: ['记录告警时间与影响范围', '收集日志、指标和近期变更', '执行止损或回滚并验证恢复', '补充根因与后续改进项'] },
};
function updateTemplateHint(value, form = $('#taskForm')) {
  const hint = $('#templateHint');
  if (!hint || !form) return;
  const template = WORKFLOW_TEMPLATES[value];
  hint.textContent = template ? `${template.label}检查清单：${template.todos.join(' · ')}` : '选择模板会预填检查清单，可按实际情况修改。';
}
function closeTaskModal() {
  $('#modal').classList.add('hidden');
  state.editTaskId = null;
}

function openMangaModal(manga = null) {
  state.editMangaId = manga?.id || null;
  $('#mangaModalTitle').textContent = manga ? '改漫画名' : '新的一部漫画';
  const form = $('#mangaForm');
  form.title.value = manga?.title || '';
  form.querySelector('[type="submit"]').textContent = manga ? '保存题名' : '收进书架';
  $('#mangaModal').classList.remove('hidden');
  form.title.focus();
}
function closeMangaModal() {
  $('#mangaModal').classList.add('hidden');
  state.editMangaId = null;
}
async function onMangaSubmit(ev) {
  ev.preventDefault();
  const title = ev.target.title.value.trim();
  if (!title) return;
  try {
    if (state.editMangaId) {
      await api(`/api/mangas/${state.editMangaId}`, { method: 'PATCH', body: JSON.stringify({ title }) });
      toast('漫画名改好了！', '改！');
    } else {
      const res = await api('/api/mangas', { method: 'POST', body: JSON.stringify({ title }) });
      state.activeMangaId = res.manga.id;
      localStorage.setItem('manga-log-active', state.activeMangaId);
      state.currentId = null;
      state.view = 'toc';
      state.query = '';
      state.statusFilter = '全部';
      state.tagFilter = '';
      state.timelineFilters = { host: '', kind: '', result: '', from: '', to: '' };
      $('#search').value = '';
      toast('新漫画，开篇！', '开！');
    }
    closeMangaModal();
    await refresh();
  } catch (e) { toast(e.message, '啊！'); }
}
async function switchManga(id) {
  if (!id || id === state.activeMangaId) return;
  const previous = state.activeMangaId;
  state.activeMangaId = id;
  state.currentId = null;
  state.editEntryId = null;
  state.query = '';
  state.statusFilter = '全部';
  state.tagFilter = '';
  $('#search').value = '';
  localStorage.setItem('manga-log-active', id);
  try {
    await refresh();
    toast(`切到「${state.data.activeMangaTitle}」`, '切！');
  } catch (e) {
    state.activeMangaId = previous;
    if (previous) localStorage.setItem('manga-log-active', previous);
    renderMangaPicker();
    toast(e.message, '啊！');
  }
}

async function onTaskSubmit(ev) {
  ev.preventDefault();
  const f = ev.target;
  const payload = {
    title: f.title.value,
    host: f.host.value,
    status: f.status.value,
    tags: f.tags.value.split(/[,，、]/).map((s) => s.trim()).filter(Boolean),
    note: f.note.value,
  };
  const template = !state.editTaskId && WORKFLOW_TEMPLATES[f.template.value];
  if (template) payload.todos = template.todos.map((text) => ({ id: genId('td'), text, done: false }));
  try {
    if (state.editTaskId) {
      await api(`/api/tasks/${state.editTaskId}`, { method: 'PATCH', body: JSON.stringify(payload) });
      toast('这一话改好了！', '改！');
    } else {
      const res = await api('/api/tasks', { method: 'POST', body: JSON.stringify(payload) });
      state.currentId = res.task.id;
      state.view = 'toc';
      toast('新一话，开演！', '开！');
    }
    closeTaskModal();
    await refresh();
  } catch (e) { toast(e.message, '啊！'); }
}

async function deleteTask(btn) {
  if (!btn.dataset.armed) {
    btn.dataset.armed = '1';
    btn.classList.add('armed');
    btn.textContent = '真的撕？！';
    setTimeout(() => {
      btn.dataset.armed = '';
      btn.classList.remove('armed');
      btn.textContent = '✕ 整话撕掉';
    }, 2600);
    return;
  }
  try {
    await api(`/api/tasks/${state.currentId}`, { method: 'DELETE' });
    state.currentId = null;
    toast('这一话撕掉了…', '撕！');
    await refresh();
  } catch (e) { toast(e.message, '啊！'); }
}

/* ---------------- 交互：分格 ---------------- */
async function onEntrySubmit(ev) {
  ev.preventDefault();
  const f = ev.target;
  syncTablesFromDom();
  const payload = {
    time: (f['e-time'].value || nowLocal()).replace('T', ' '),
    kind: f['e-kind'].value,
    result: f['e-result'].value,
    title: f['e-title'].value,
    detail: f['e-detail'].value,
    reviewAt: (f['e-review'].value || '').replace('T', ' '),
    relation: (() => {
      const [taskId, entryId] = (f['e-related'].value || '').split('|');
      return taskId && entryId ? { taskId, entryId, type: f['e-rel-type'].value } : null;
    })(),
    images: formImages,
    tables: formTables,
    files: formFiles,
  };
  const t = currentTask();
  if (!t) return;
  try {
    if (state.editEntryId) {
      await api(`/api/tasks/${t.id}/entries/${state.editEntryId}`, { method: 'PATCH', body: JSON.stringify(payload) });
      state.editEntryId = null;
      toast('这一格改好了！', '改！');
    } else {
      await api(`/api/tasks/${t.id}/entries`, { method: 'POST', body: JSON.stringify(payload) });
      toast('记下这一格！', '唰！');
    }
    formImages = [];
    formTables = [];
    formFiles = [];
    clearEntryDraft();
    await refresh();
  } catch (e) { toast(e.message, '啊！'); }
}

function editEntry(btn) {
  if (btn.dataset.tid) state.currentId = btn.dataset.tid;
  state.view = 'toc';
  state.editEntryId = btn.dataset.id;
  renderTabs(); renderToc(); renderQuickSelect(); renderStage();
}

async function deleteEntry(btn) {
  if (!btn.dataset.armed) {
    btn.dataset.armed = '1';
    btn.classList.add('armed');
    btn.textContent = '✕ 确认撕？';
    setTimeout(() => {
      btn.dataset.armed = '';
      btn.classList.remove('armed');
      btn.textContent = '✕ 撕';
    }, 2600);
    return;
  }
  if (btn.dataset.tid) state.currentId = btn.dataset.tid;
  const t = currentTask();
  if (!t) return;
  try {
    await api(`/api/tasks/${t.id}/entries/${btn.dataset.id}`, { method: 'DELETE' });
    toast('这一格撕掉了…', '撕！');
    await refresh();
  } catch (e) { toast(e.message, '啊！'); }
}

/* ---------------- 交互：快记条 ---------------- */
function updateQuickKind() {
  const text = $('#quickInput').value.trim();
  const kind = detectKind(text);
  const el = $('#quickKind');
  el.textContent = text ? `将记为：${kind}` : '将记为：其他';
}
async function onQuickSubmit(ev) {
  ev.preventDefault();
  const text = $('#quickInput').value.trim();
  if (!text) return;
  const taskId = $('#quickTask').value;
  if (!taskId || !state.data.tasks.some((t) => t.id === taskId)) { toast('先在左栏「开新话」！', '啊！'); return; }
  try {
    await api(`/api/tasks/${taskId}/entries`, {
      method: 'POST',
      body: JSON.stringify({ title: text, kind: detectKind(text), result: '待验证', time: nowLocal() }),
    });
    $('#quickInput').value = '';
    updateQuickKind();
    state.view = 'toc';
    state.currentId = taskId;
    await refresh();
    toast('快记完成，已入格！', '唰！');
  } catch (e) { toast(e.message, '啊！'); }
}

/* ---------------- 交互：遗留事项 ---------------- */
async function patchTodos(t, todos) {
  try {
    await api(`/api/tasks/${t.id}`, { method: 'PATCH', body: JSON.stringify({ todos }) });
    await refresh();
  } catch (e) { toast(e.message, '啊！'); }
}
function toggleTodo(checkbox) {
  const t = currentTask();
  if (!t) return;
  patchTodos(t, (t.todos || []).map((x) => x.id === checkbox.dataset.tid ? { ...x, done: checkbox.checked } : x));
}
function addTodo(form) {
  const t = currentTask();
  if (!t) return;
  const text = form.querySelector('input').value.trim();
  if (!text) return;
  const id = `td_${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`;
  patchTodos(t, [...(t.todos || []), { id, text, done: false }]);
}
function deleteTodo(btn) {
  const t = currentTask();
  if (!t) return;
  patchTodos(t, (t.todos || []).filter((x) => x.id !== btn.dataset.tid));
}

/* ---------------- 导出 ---------------- */
/* 单格复制为 Markdown：周报 / 聊天里直接贴 */
function entryToMd(e, task, tasks) {
  const lines = [];
  if (task) lines.push(`> 第${cnNum(tasks.indexOf(task) + 1)}话 · ${task.title}${task.host ? ` · ${task.host}` : ''}`, '');
  lines.push(`### ${e.time || '—'} ｜ ${e.kind} ｜ ${e.result}`);
  lines.push(`**${e.title}**`, '');
  if (e.detail) lines.push(e.detail, '');
  if (e.reviewAt) lines.push(`- 复核时间：${e.reviewAt}`);
  const related = relationTarget(e.relation, tasks);
  if (related) lines.push(`- ${e.relation.type || '相关'}：第${cnNum(tasks.indexOf(related.task) + 1)}话 · ${related.entry.title}`);
  (e.images || []).forEach((im) => lines.push(`![${im.name}](${im.url})`));
  (e.files || []).forEach((fl) => lines.push(`📄 [${fl.name}](${fl.url})${fl.size ? `（${fmtSize(fl.size)}）` : ''}`));
  (e.tables || []).forEach((tb) => {
    if (tb.title) lines.push(`**${tb.title}**`, '');
    if (tb.cols.length) {
      lines.push(`| ${tb.cols.join(' | ')} |`, `| ${tb.cols.map(() => '---').join(' | ')} |`);
      (tb.rows || []).forEach((r) => lines.push(`| ${tb.cols.map((c, i) => (r[i] ?? '')).join(' | ')} |`));
      lines.push('');
    }
  });
  return lines.join('\n');
}
function copyEntryMd(btn) {
  const task = state.data.tasks.find((t) => t.id === btn.dataset.tid);
  const entry = task?.entries.find((e) => e.id === btn.dataset.id);
  if (!task || !entry) return;
  copyText(entryToMd(entry, task, state.data.tasks), '本格 Markdown 已复制');
}
function buildMd(tasks, mangaTitle = state.data.activeMangaTitle || '服务器改动', mangaTasks = tasks) {
  const lines = [];
  const total = tasks.reduce((s, t) => s + t.entries.length, 0);
  lines.push(`# ${mangaTitle} · 漫画志`, '');
  lines.push(`> 导出于 ${nowLocal()} · 共 ${tasks.length} 话 / ${total} 格`, '');
  tasks.forEach((t) => {
    const idx = mangaTasks.indexOf(t) + 1;
    lines.push(`## 第${cnNum(idx)}话 · ${t.title}`, '');
    const metas = [`状态：${t.status}`];
    if (t.host) metas.push(`主机：${t.host}`);
    if ((t.tags || []).length) metas.push(`标签：${t.tags.join('、')}`);
    lines.push(`- ${metas.join(' ｜ ')}`);
    if (t.note) lines.push(`- 备注：${t.note}`);
    lines.push('');
    const todos = t.todos || [];
    if (todos.length) {
      lines.push('**遗留事项**', '');
      todos.forEach((td) => lines.push(`- [${td.done ? 'x' : ' '}] ${td.text}`));
      lines.push('');
    }
    const entries = sortByTime(t.entries);
    if (!entries.length) lines.push('（本话暂无分格记录）', '');
    entries.forEach((e, j) => {
      lines.push(`### 格 ${pad(j + 1)} ｜ ${e.time || '—'} ｜ ${e.kind} ｜ ${e.result}`);
      lines.push(`**${e.title}**`, '');
      if (e.detail) lines.push(e.detail, '');
      if (e.reviewAt) lines.push(`- 复核时间：${e.reviewAt}`);
      const related = relationTarget(e.relation, mangaTasks);
      if (related) lines.push(`- ${e.relation.type || '相关'}：第${cnNum(mangaTasks.indexOf(related.task) + 1)}话 · ${related.entry.title}`);
      if (e.reviewAt || related) lines.push('');
      (e.images || []).forEach((im) => lines.push(`![${im.name}](${im.url})`));
      if ((e.images || []).length) lines.push('');
      (e.files || []).forEach((fl) => lines.push(`📄 [${fl.name}](${fl.url})${fl.size ? `（${fmtSize(fl.size)}）` : ''}`));
      if ((e.files || []).length) lines.push('');
      (e.tables || []).forEach((tb) => {
        if (tb.title) lines.push(`**${tb.title}**`, '');
        if (tb.cols.length) {
          lines.push(`| ${tb.cols.join(' | ')} |`);
          lines.push(`| ${tb.cols.map(() => '---').join(' | ')} |`);
          (tb.rows || []).forEach((r) => lines.push(`| ${tb.cols.map((c, i) => (r[i] ?? '')).join(' | ')} |`));
          lines.push('');
        }
      });
    });
  });
  return lines.join('\n');
}
function download(filename, text, mime = 'text/markdown;charset=utf-8') {
  const blob = new Blob([text], { type: mime });
  const a = document.createElement('a');
  a.href = URL.createObjectURL(blob);
  a.download = filename;
  a.click();
  URL.revokeObjectURL(a.href);
}
async function exportMd() {
  try {
    const full = await api('/api/data?all=1');
    const docs = full.mangas.map((manga) => buildMd(manga.tasks, manga.title, manga.tasks));
    download(`漫画志_全稿_${dayStr()}.md`, docs.join('\n\n---\n\n'));
    toast(`全稿导出齐了 · ${full.mangas.length} 部漫画`, '齐！');
  } catch (e) { toast(e.message, '啊！'); }
}
function exportTaskMd() {
  const t = currentTask();
  if (!t) return;
  download(`漫画志_${t.title.slice(0, 20)}_${dayStr()}.md`, buildMd([t], state.data.activeMangaTitle, state.data.tasks));
  toast('本话导出齐了！', '齐！');
}
function exportMangaMd() {
  const tasks = state.data.tasks;
  if (!tasks.length) { toast('这一部还没有话，先开新话吧', '嗯？'); return; }
  download(`漫画志_${(state.data.activeMangaTitle || '本部').slice(0, 30)}_${dayStr()}.md`, buildMd(tasks, state.data.activeMangaTitle, tasks));
  toast('本部导出齐了！', '齐！');
}
async function exportBackup() {
  try {
    const full = await api('/api/data?all=1');
    download(`漫画志_备份_${dayStr()}.json`, JSON.stringify(full, null, 2), 'application/json');
    toast(`整套备份导出齐了 · ${full.mangas.length} 部漫画`, '齐！');
  } catch (e) { toast(e.message, '啊！'); }
}
function resetImportBtns() {
  state.pendingImport = null;
  document.querySelectorAll('[data-action="import-backup"]').forEach((b) => {
    b.classList.remove('armed');
    b.textContent = '⇒ 导入备份';
  });
}
function onImportFile(ev) {
  const file = ev.target.files[0];
  ev.target.value = '';
  if (!file) return;
  file.text().then((txt) => {
    let obj;
    try { obj = JSON.parse(txt); } catch (_) { toast('文件不是合法 JSON', '啊！'); return; }
    if (!obj || (!Array.isArray(obj.tasks) && !Array.isArray(obj.mangas))) { toast('缺少 mangas / tasks 字段，不是漫画志备份', '啊！'); return; }
    state.pendingImport = obj;
    const btn = document.querySelector('[data-action="import-backup"]');
    if (btn) {
      btn.classList.add('armed');
      const count = Array.isArray(obj.mangas)
        ? `${obj.mangas.length} 部漫画 / ${obj.mangas.reduce((sum, manga) => sum + (manga.tasks || []).length, 0)} 话`
        : `当前漫画的 ${obj.tasks.length} 话`;
      btn.textContent = `⚠ 确认覆盖（${count}）？`;
    }
    toast('再点一次「导入备份」确认覆盖', '⚠');
  });
}
async function confirmImport() {
  const obj = state.pendingImport;
  if (!obj) return;
  try {
    const res = await api('/api/import', { method: 'POST', body: JSON.stringify(obj) });
    resetImportBtns();
    toast(`已导入 ${res.mangas} 部漫画 · ${res.tasks} 话`, '齐！');
    await refresh();
  } catch (e) {
    resetImportBtns();
    toast(e.message, '啊！');
  }
}
async function restoreSnapshot(btn) {
  try {
    const result = await api('/api/snapshots/restore', { method: 'POST', body: JSON.stringify({ id: btn.dataset.id }) });
    state.view = 'toc';
    toast(`快照已恢复 · ${result.tasks} 话${result.missingAssets ? ` · 缺 ${result.missingAssets} 个附件` : ''}`, '復！');
    await refresh();
  } catch (e) { toast(e.message, '啊！'); }
}
function jumpToTask(taskId, entryId = '') {
  state.currentId = taskId;
  state.editEntryId = null;
  state.view = 'toc';
  renderAll();
  requestAnimationFrame(() => {
    const target = entryId
      ? [...document.querySelectorAll('[data-eid]')].find((el) => el.dataset.eid === entryId && el.dataset.tid === taskId)
      : $('.todos');
    target?.scrollIntoView({ behavior: 'smooth', block: 'center' });
  });
}

/* 日期控件保留原生 input 作为表单数据源，日历面板由页面绘制。 */
function comicDateInputLabel(input) {
  if (input.getAttribute('aria-label')) return input.getAttribute('aria-label');
  const label = input.closest('label');
  if (label) return [...label.childNodes].filter((node) => node.nodeType === Node.TEXT_NODE).map((node) => node.textContent.trim()).filter(Boolean).join(' ') || '日期';
  return input.type === 'date' ? '选择日期' : '选择日期和时间';
}
function comicDateParts(input) {
  const match = /^(\d{4})-(\d{2})-(\d{2})(?:T(\d{2}):(\d{2}))?$/.exec(input.value);
  if (!match) return null;
  return { year: +match[1], month: +match[2] - 1, day: +match[3], hour: +(match[4] || 0), minute: +(match[5] || 0) };
}
function comicDateISO(year, month, day) {
  return `${year}-${String(month + 1).padStart(2, '0')}-${String(day).padStart(2, '0')}`;
}
function comicDateToday() {
  const now = new Date();
  return { year: now.getFullYear(), month: now.getMonth(), day: now.getDate(), hour: now.getHours(), minute: now.getMinutes() };
}
function syncComicDate(wrapper) {
  const input = wrapper?.querySelector('.comic-date-native');
  const trigger = wrapper?.querySelector('.cd-trigger');
  const value = wrapper?.querySelector('.cd-value');
  if (!input || !trigger || !value) return;
  const date = comicDateParts(input);
  if (!date) {
    const placeholder = input.type === 'date' ? '选择日期' : '选择日期和时间';
    value.textContent = placeholder;
    trigger.setAttribute('aria-label', `${trigger.dataset.label}：${placeholder}`);
    trigger.disabled = input.disabled;
    return;
  }
  const dateLabel = `${date.year}年${String(date.month + 1).padStart(2, '0')}月${String(date.day).padStart(2, '0')}日`;
  const fullLabel = input.type === 'date' ? dateLabel : `${dateLabel} · ${String(date.hour).padStart(2, '0')}:${String(date.minute).padStart(2, '0')}`;
  value.textContent = fullLabel;
  trigger.setAttribute('aria-label', `${trigger.dataset.label}：${fullLabel}`);
  trigger.disabled = input.disabled;
}
function setComicDateValue(input, value) {
  input.value = value;
  const wrapper = input.closest('.comic-date');
  syncComicDate(wrapper);
  input.dispatchEvent(new Event('input', { bubbles: true }));
  input.dispatchEvent(new Event('change', { bubbles: true }));
}
function renderComicCalendar(wrapper) {
  const input = wrapper.querySelector('.comic-date-native');
  const popup = wrapper.querySelector('.cd-popup');
  const selected = comicDateParts(input);
  const now = comicDateToday();
  const year = +(wrapper.dataset.viewYear || selected?.year || now.year);
  const month = +(wrapper.dataset.viewMonth ?? selected?.month ?? now.month);
  wrapper.dataset.viewYear = String(year);
  wrapper.dataset.viewMonth = String(month);
  const monthDate = new Date(year, month, 1);
  const firstOffset = (monthDate.getDay() + 6) % 7;
  const gridStart = new Date(year, month, 1 - firstOffset);
  const todayISO = comicDateISO(now.year, now.month, now.day);
  const selectedISO = selected ? comicDateISO(selected.year, selected.month, selected.day) : '';
  const days = Array.from({ length: 42 }, (_, index) => {
    const day = new Date(gridStart.getFullYear(), gridStart.getMonth(), gridStart.getDate() + index);
    const iso = comicDateISO(day.getFullYear(), day.getMonth(), day.getDate());
    const classes = ['cd-day'];
    if (day.getMonth() !== month) classes.push('outside');
    if (iso === todayISO) classes.push('today');
    if (iso === selectedISO) classes.push('selected');
    return `<button type="button" class="${classes.join(' ')}" data-calendar="day" data-date="${iso}" aria-label="${day.getFullYear()}年${day.getMonth() + 1}月${day.getDate()}日" aria-pressed="${iso === selectedISO}" tabindex="-1">${day.getDate()}</button>`;
  }).join('');
  const time = selected || now;
  const timeEditor = input.type === 'datetime-local' ? `
    <div class="cd-time-row">
      <span class="cd-time-caption">時間</span>
      <label class="cd-time-field">时<input class="cd-time-part cd-hour" type="number" min="0" max="23" inputmode="numeric" value="${String(time.hour).padStart(2, '0')}" aria-label="小时"></label>
      <b>:</b>
      <label class="cd-time-field">分<input class="cd-time-part cd-minute" type="number" min="0" max="59" inputmode="numeric" value="${String(time.minute).padStart(2, '0')}" aria-label="分钟"></label>
    </div>` : '';
  popup.innerHTML = `
    <div class="cd-head">
      <button type="button" class="cd-nav" data-calendar="month" data-step="-1" aria-label="上个月">‹</button>
      <strong>${year}年${String(month + 1).padStart(2, '0')}月</strong>
      <button type="button" class="cd-nav" data-calendar="month" data-step="1" aria-label="下个月">›</button>
    </div>
    <div class="cd-weekdays" aria-hidden="true"><span>一</span><span>二</span><span>三</span><span>四</span><span>五</span><span>六</span><span>日</span></div>
    <div class="cd-grid" role="group" aria-label="日期">${days}</div>
    ${timeEditor}
    <div class="cd-footer">
      <button type="button" class="cd-footer-btn" data-calendar="today">今天</button>
      <button type="button" class="cd-footer-btn" data-calendar="clear">清除</button>
      <button type="button" class="cd-footer-btn cd-done" data-calendar="done">完成</button>
    </div>`;
}
function enhanceComicDate(input) {
  if (!(input instanceof HTMLInputElement) || !['date', 'datetime-local'].includes(input.type) || input.dataset.comicDateEnhanced) return;
  const label = comicDateInputLabel(input);
  const wrapper = document.createElement('div');
  wrapper.className = 'comic-date';
  const trigger = document.createElement('button');
  trigger.type = 'button';
  trigger.className = 'cd-trigger';
  trigger.setAttribute('aria-haspopup', 'dialog');
  trigger.setAttribute('aria-expanded', 'false');
  trigger.setAttribute('aria-label', label);
  trigger.dataset.label = label;
  trigger.setAttribute('aria-controls', genId('calendar'));
  const value = document.createElement('span');
  value.className = 'cd-value';
  const icon = document.createElement('span');
  icon.className = 'cd-icon';
  icon.setAttribute('aria-hidden', 'true');
  icon.textContent = '▦';
  trigger.append(value, icon);
  const popup = document.createElement('div');
  popup.className = 'cd-popup';
  popup.id = trigger.getAttribute('aria-controls');
  popup.setAttribute('role', 'dialog');
  popup.setAttribute('aria-label', `${label}日历`);
  popup.hidden = true;
  input.dataset.comicDateEnhanced = '1';
  input.classList.add('comic-date-native');
  input.tabIndex = -1;
  input.setAttribute('aria-hidden', 'true');
  input.parentNode.insertBefore(wrapper, input);
  wrapper.append(trigger, popup, input);
  syncComicDate(wrapper);
}
function closeComicDate(wrapper, returnFocus = false) {
  if (!wrapper) return;
  wrapper.classList.remove('open', 'opens-up');
  const trigger = wrapper.querySelector('.cd-trigger');
  const popup = wrapper.querySelector('.cd-popup');
  trigger?.setAttribute('aria-expanded', 'false');
  if (popup) { popup.hidden = true; popup.style.left = ''; popup.style.maxHeight = ''; }
  if (returnFocus) trigger?.focus();
}
function openComicDate(wrapper) {
  document.querySelectorAll('.comic-date.open').forEach((other) => { if (other !== wrapper) closeComicDate(other); });
  document.querySelectorAll('.comic-select.open').forEach((other) => closeComicSelect(other));
  const input = wrapper.querySelector('.comic-date-native');
  const popup = wrapper.querySelector('.cd-popup');
  const selected = comicDateParts(input) || comicDateToday();
  wrapper.dataset.viewYear = String(selected.year);
  wrapper.dataset.viewMonth = String(selected.month);
  renderComicCalendar(wrapper);
  wrapper.classList.add('open');
  wrapper.querySelector('.cd-trigger')?.setAttribute('aria-expanded', 'true');
  popup.hidden = false;
  wrapper.classList.remove('opens-up');
  const wrapperBounds = wrapper.getBoundingClientRect();
  const popupBounds = popup.getBoundingClientRect();
  const left = Math.max(8, Math.min(popupBounds.left, window.innerWidth - popupBounds.width - 8));
  popup.style.left = `${left - wrapperBounds.left}px`;
  const bounds = wrapper.getBoundingClientRect();
  const popupHeight = Math.min(popup.scrollHeight, 430, window.innerHeight * 0.7);
  const spaceAbove = Math.max(120, bounds.top - 21);
  const spaceBelow = Math.max(120, window.innerHeight - bounds.bottom - 21);
  const opensUp = spaceBelow < popupHeight && spaceAbove > spaceBelow;
  popup.style.maxHeight = `${Math.min(popupHeight, opensUp ? spaceAbove : spaceBelow)}px`;
  if (opensUp) wrapper.classList.add('opens-up');
  popup.querySelector('.cd-day.selected, .cd-day.today')?.focus();
}

/* 保留原生 select 作为表单数据源，用页面内菜单替代浏览器原生弹层。 */
function comicSelectLabel(select) {
  if (select.getAttribute('aria-label')) return select.getAttribute('aria-label');
  if (select.title) return select.title;
  const label = select.closest('label');
  if (label) return [...label.childNodes].filter((node) => node.nodeType === Node.TEXT_NODE).map((node) => node.textContent.trim()).filter(Boolean).join(' ') || '选择';
  return '选择';
}
function closeComicSelect(wrapper, returnFocus = false) {
  if (!wrapper) return;
  wrapper.classList.remove('open');
  const trigger = wrapper.querySelector('.cs-trigger');
  const menu = wrapper.querySelector('.cs-menu');
  trigger?.setAttribute('aria-expanded', 'false');
  if (menu) menu.hidden = true;
  if (returnFocus) trigger?.focus();
}
function syncComicSelect(select) {
  if (!select || !select.dataset.comicEnhanced) return;
  const wrapper = select.closest('.comic-select');
  const trigger = wrapper?.querySelector('.cs-trigger');
  const menu = wrapper?.querySelector('.cs-menu');
  if (!wrapper || !trigger || !menu) return;
  const selected = select.options[select.selectedIndex];
  trigger.querySelector('.cs-value').textContent = selected?.textContent || '—';
  trigger.disabled = select.disabled;
  menu.replaceChildren();
  [...select.options].forEach((option, index) => {
    const item = document.createElement('button');
    item.type = 'button';
    item.className = 'cs-option';
    item.setAttribute('role', 'option');
    item.setAttribute('aria-selected', String(index === select.selectedIndex));
    item.disabled = option.disabled;
    item.tabIndex = -1;
    item.dataset.index = String(index);
    item.textContent = option.textContent;
    menu.appendChild(item);
  });
}
function enhanceComicSelect(select) {
  if (!(select instanceof HTMLSelectElement) || select.dataset.comicEnhanced) return;
  const label = comicSelectLabel(select);
  const wrapper = document.createElement('div');
  wrapper.className = `comic-select${select.id === 'quickTask' ? ' comic-quick' : ''}`;
  const trigger = document.createElement('button');
  trigger.type = 'button';
  trigger.className = 'cs-trigger';
  trigger.setAttribute('role', 'combobox');
  trigger.setAttribute('aria-haspopup', 'listbox');
  trigger.setAttribute('aria-expanded', 'false');
  trigger.setAttribute('aria-label', label);
  trigger.setAttribute('aria-controls', genId('menu'));
  const value = document.createElement('span');
  value.className = 'cs-value';
  const arrow = document.createElement('span');
  arrow.className = 'cs-arrow';
  arrow.setAttribute('aria-hidden', 'true');
  arrow.textContent = '⌄';
  trigger.append(value, arrow);
  const menu = document.createElement('div');
  menu.className = 'cs-menu';
  menu.id = trigger.getAttribute('aria-controls');
  menu.setAttribute('role', 'listbox');
  menu.setAttribute('aria-label', `${label}选项`);
  menu.hidden = true;
  select.dataset.comicEnhanced = '1';
  select.classList.add('comic-native');
  select.tabIndex = -1;
  select.setAttribute('aria-hidden', 'true');
  select.parentNode.insertBefore(wrapper, select);
  wrapper.append(trigger, menu, select);
  syncComicSelect(select);
}
function enhanceComicControls(root = document) {
  if (root instanceof HTMLSelectElement) enhanceComicSelect(root);
  if (root instanceof HTMLInputElement) enhanceComicDate(root);
  root.querySelectorAll?.('select').forEach(enhanceComicSelect);
  root.querySelectorAll?.('input[type="date"], input[type="datetime-local"]').forEach(enhanceComicDate);
}
function openComicSelect(wrapper) {
  document.querySelectorAll('.comic-select.open').forEach((other) => { if (other !== wrapper) closeComicSelect(other); });
  wrapper.classList.add('open');
  wrapper.querySelector('.cs-trigger')?.setAttribute('aria-expanded', 'true');
  const menu = wrapper.querySelector('.cs-menu');
  menu.hidden = false;
  wrapper.classList.remove('opens-up');
  const bounds = wrapper.getBoundingClientRect();
  const menuHeight = Math.min(menu.scrollHeight, window.innerHeight * 0.48);
  if (window.innerHeight - bounds.bottom < menuHeight + 12 && bounds.top > window.innerHeight - bounds.bottom) {
    wrapper.classList.add('opens-up');
  }
  const select = wrapper.querySelector('select');
  const selected = menu.querySelector(`[data-index="${select?.selectedIndex ?? 0}"]`) || menu.querySelector('.cs-option:not(:disabled)');
  selected?.focus();
}
document.addEventListener('click', (ev) => {
  const trigger = ev.target.closest('.cs-trigger');
  if (trigger) {
    const wrapper = trigger.closest('.comic-select');
    if (wrapper.classList.contains('open')) closeComicSelect(wrapper);
    else openComicSelect(wrapper);
    return;
  }
  const option = ev.target.closest('.cs-option');
  if (option) {
    const wrapper = option.closest('.comic-select');
    const select = wrapper?.querySelector('select');
    if (!select || option.disabled) return;
    select.selectedIndex = +option.dataset.index;
    syncComicSelect(select);
    closeComicSelect(wrapper, true);
    select.dispatchEvent(new Event('change', { bubbles: true }));
    return;
  }
  if (!ev.target.closest('.comic-select')) document.querySelectorAll('.comic-select.open').forEach((wrapper) => closeComicSelect(wrapper));
});
document.addEventListener('click', (ev) => {
  const trigger = ev.target.closest('.cd-trigger');
  if (trigger) {
    const wrapper = trigger.closest('.comic-date');
    if (wrapper.classList.contains('open')) closeComicDate(wrapper);
    else openComicDate(wrapper);
    return;
  }
  const action = ev.target.closest('[data-calendar]');
  if (action) {
    const wrapper = action.closest('.comic-date');
    if (!wrapper) return;
    const input = wrapper.querySelector('.comic-date-native');
    const date = comicDateParts(input) || comicDateToday();
    if (action.dataset.calendar === 'month') {
      const month = new Date(+wrapper.dataset.viewYear, +wrapper.dataset.viewMonth + +action.dataset.step, 1);
      wrapper.dataset.viewYear = String(month.getFullYear());
      wrapper.dataset.viewMonth = String(month.getMonth());
      renderComicCalendar(wrapper);
      wrapper.querySelector(`.cd-nav[data-step="${action.dataset.step}"]`)?.focus();
    } else if (action.dataset.calendar === 'day') {
      const [year, month, day] = action.dataset.date.split('-').map(Number);
      const value = comicDateISO(year, month - 1, day);
      if (input.type === 'date') {
        setComicDateValue(input, value);
        closeComicDate(wrapper, true);
      } else {
        setComicDateValue(input, `${value}T${String(date.hour).padStart(2, '0')}:${String(date.minute).padStart(2, '0')}`);
        wrapper.dataset.viewYear = String(year);
        wrapper.dataset.viewMonth = String(month - 1);
        renderComicCalendar(wrapper);
        wrapper.querySelector(`.cd-day[data-date="${value}"]`)?.focus();
      }
    } else if (action.dataset.calendar === 'today') {
      const today = comicDateToday();
      const value = comicDateISO(today.year, today.month, today.day);
      setComicDateValue(input, input.type === 'date' ? value : `${value}T${String(today.hour).padStart(2, '0')}:${String(today.minute).padStart(2, '0')}`);
      closeComicDate(wrapper, true);
    } else if (action.dataset.calendar === 'clear') {
      setComicDateValue(input, '');
      closeComicDate(wrapper, true);
    } else if (action.dataset.calendar === 'done') closeComicDate(wrapper, true);
    return;
  }
  if (ev.target.closest('.cd-popup')) return;
  document.querySelectorAll('.comic-date.open').forEach((wrapper) => closeComicDate(wrapper));
});
function updateComicDateTime(wrapper) {
  const input = wrapper?.querySelector('.comic-date-native');
  if (!input || input.type !== 'datetime-local') return;
  const current = comicDateParts(input) || comicDateToday();
  const hourField = wrapper.querySelector('.cd-hour');
  const minuteField = wrapper.querySelector('.cd-minute');
  if (!hourField || !minuteField || hourField.value === '' || minuteField.value === '') return;
  const hour = Math.max(0, Math.min(23, +hourField.value));
  const minute = Math.max(0, Math.min(59, +minuteField.value));
  hourField.value = String(hour).padStart(2, '0');
  minuteField.value = String(minute).padStart(2, '0');
  const value = `${comicDateISO(current.year, current.month, current.day)}T${String(hour).padStart(2, '0')}:${String(minute).padStart(2, '0')}`;
  setComicDateValue(input, value);
}
document.addEventListener('input', (ev) => {
  const field = ev.target.closest?.('.cd-time-part');
  if (field) updateComicDateTime(field.closest('.comic-date'));
});
document.addEventListener('change', (ev) => {
  const field = ev.target.closest?.('.cd-time-part');
  if (field) updateComicDateTime(field.closest('.comic-date'));
});
document.addEventListener('keydown', (ev) => {
  const day = ev.target.closest?.('.cd-day');
  const popup = ev.target.closest?.('.cd-popup');
  const wrapper = (day || popup)?.closest('.comic-date');
  if (!wrapper) return;
  if (ev.key === 'Escape') {
    ev.preventDefault(); closeComicDate(wrapper, true); return;
  }
  if (ev.key === 'Tab') { closeComicDate(wrapper, true); return; }
  if (!day) return;
  const delta = { ArrowLeft: -1, ArrowRight: 1, ArrowUp: -7, ArrowDown: 7 }[ev.key];
  const current = new Date(`${day.dataset.date}T12:00:00`);
  let next;
  if (delta) next = new Date(current.getFullYear(), current.getMonth(), current.getDate() + delta);
  else if (ev.key === 'Home' || ev.key === 'End') {
    const offset = (current.getDay() + 6) % 7;
    next = new Date(current.getFullYear(), current.getMonth(), current.getDate() + (ev.key === 'Home' ? -offset : 6 - offset));
  } else return;
  ev.preventDefault();
  wrapper.dataset.viewYear = String(next.getFullYear());
  wrapper.dataset.viewMonth = String(next.getMonth());
  renderComicCalendar(wrapper);
  const iso = comicDateISO(next.getFullYear(), next.getMonth(), next.getDate());
  wrapper.querySelector(`.cd-day[data-date="${iso}"]`)?.focus();
});
document.addEventListener('keydown', (ev) => {
  const trigger = ev.target.closest?.('.cs-trigger');
  const option = ev.target.closest?.('.cs-option');
  const wrapper = (trigger || option)?.closest('.comic-select');
  if (!wrapper) return;
  const options = [...wrapper.querySelectorAll('.cs-option:not(:disabled)')];
  if (trigger && ['ArrowDown', 'ArrowUp', 'Enter', ' '].includes(ev.key)) {
    if (!wrapper.classList.contains('open')) { ev.preventDefault(); openComicSelect(wrapper); }
    return;
  }
  if (!option) return;
  const at = options.indexOf(option);
  if (ev.key === 'ArrowDown' || ev.key === 'ArrowUp') {
    ev.preventDefault();
    options[Math.max(0, Math.min(options.length - 1, at + (ev.key === 'ArrowDown' ? 1 : -1)))]?.focus();
  } else if (ev.key === 'Home' || ev.key === 'End') {
    ev.preventDefault();
    (ev.key === 'Home' ? options[0] : options.at(-1))?.focus();
  } else if (ev.key === 'Escape') {
    ev.preventDefault(); closeComicSelect(wrapper, true);
  } else if (ev.key === 'Tab') {
    closeComicSelect(wrapper, true);
  }
});
const comicSelectObserver = new MutationObserver((records) => {
  for (const record of records) {
    if (record.target instanceof HTMLSelectElement) syncComicSelect(record.target);
    for (const node of record.addedNodes) {
      if (node.nodeType !== Node.ELEMENT_NODE) continue;
      enhanceComicControls(node);
    }
  }
});
enhanceComicControls();
comicSelectObserver.observe(document.body, { childList: true, subtree: true });

/* ---------------- 事件绑定 ---------------- */
document.addEventListener('click', (ev) => {
  const el = ev.target.closest('[data-action]');
  if (!el) return;
  const act = el.dataset.action;
  if (act === 'select-task') {
    state.currentId = el.dataset.id;
    state.editEntryId = null;
    state.view = 'toc';
    renderTabs(); renderToc(); renderQuickSelect(); renderStage();
  } else if (act === 'new-task') openTaskModal(null);
  else if (act === 'edit-task') openTaskModal(currentTask());
  else if (act === 'close-modal') closeTaskModal();
  else if (act === 'close-manga-modal') closeMangaModal();
  else if (act === 'del-task') deleteTask(el);
  else if (act === 'edit-entry') editEntry(el);
  else if (act === 'del-entry') deleteEntry(el);
  else if (act === 'open-related' || act === 'open-inbox-entry') jumpToTask(el.dataset.tid, el.dataset.eid);
  else if (act === 'open-inbox-todo') jumpToTask(el.dataset.tid);
  else if (act === 'restore-snapshot') restoreSnapshot(el);
  else if (act === 'clear-timeline-filters') { state.timelineFilters = { host: '', kind: '', result: '', from: '', to: '' }; renderStage(); }
  else if (act === 'cancel-edit-entry') { state.editEntryId = null; clearEntryDraft(); renderStage(); }
  else if (act === 'del-todo') deleteTodo(el);
  else if (act === 'filter-status') {
    state.statusFilter = el.dataset.status;
    renderChips(); renderToc(); renderStage();
  } else if (act === 'filter-tag') {
    state.tagFilter = el.dataset.tag || '';
    state.view = 'toc';
    renderTabs(); renderTagChips(); renderToc(); renderStage();
  } else if (act === 'copy-entry') copyEntryMd(el); else if (act === 'export-task') exportTaskMd();
  else if (act === 'export-backup') exportBackup();
  else if (act === 'import-backup') {
    if (state.pendingImport) confirmImport();
    else $('#importFile').click();
  } else if (act === 'view-img') {
    const urls = [...(el.closest('.e-imgs')?.querySelectorAll('.e-img') || [])].map((x) => x.dataset.url);
    openLightbox(el.dataset.url, urls.length ? urls : [el.dataset.url]);
  } else if (act === 'preview-file') {
    ev.preventDefault();
    previewFile(el.dataset.url, el.dataset.name, +el.dataset.size || 0);
  }
});
/* 条目表单里的附件按钮（data-form） */
document.addEventListener('click', (ev) => {
  const el = ev.target.closest('[data-form]');
  if (!el) return;
  const act = el.dataset.form;
  if (act === 'pick-img') $('#imgFile')?.click();
  else if (act === 'pick-doc') $('#docFile')?.click();
  else if (act === 'pick-server') openServerPicker();
  else if (act === 'del-img') {
    formImages = formImages.filter((x) => x.url !== el.dataset.url);
    renderImgStrip();
    scheduleDraftSave();
  } else if (act === 'del-file') {
    formFiles = formFiles.filter((x) => x.url !== el.dataset.url);
    renderFileStrip();
    scheduleDraftSave();
  } else if (act === 'sp-cd') {
    $('#spPath').value = el.dataset.path;
    listServerDir();
  } else if (act === 'add-table') {
    if (formTables.length >= 5) { toast('一格最多 5 张表', '满！'); return; }
    syncTablesFromDom();
    formTables.push({ id: genId('tb'), title: '', cols: ['项目', '数值'], rows: [['', '']] });
    renderTableEditors();
    scheduleDraftSave();
  } else if (act === 'del-table') {
    syncTablesFromDom();
    formTables.splice(+el.dataset.ti, 1);
    renderTableEditors();
    scheduleDraftSave();
  } else if (act === 'add-row') {
    syncTablesFromDom();
    const tb = formTables[+el.dataset.ti];
    if (!tb) return;
    if (tb.rows.length >= 100) { toast('最多 100 行', '满！'); return; }
    tb.rows.push(tb.cols.map(() => ''));
    renderTableEditors();
    scheduleDraftSave();
  } else if (act === 'add-col') {
    syncTablesFromDom();
    const tb = formTables[+el.dataset.ti];
    if (!tb) return;
    if (tb.cols.length >= 12) { toast('最多 12 列', '满！'); return; }
    tb.cols.push('');
    tb.rows.forEach((r) => r.push(''));
    renderTableEditors();
    scheduleDraftSave();
  } else if (act === 'del-col') {
    syncTablesFromDom();
    const tb = formTables[+el.dataset.ti];
    if (!tb || tb.cols.length <= 1) return;
    tb.cols.pop();
    tb.rows.forEach((r) => r.pop());
    renderTableEditors();
    scheduleDraftSave();
  }
});
/* 表格编辑器输入同步 */
document.addEventListener('input', (ev) => {
  const el = ev.target;
  if (el.dataset && el.dataset.tf) syncTablesFromDom();
});
/* 选择图片 / 文档文件 */
document.addEventListener('change', (ev) => {
  if (ev.target.classList && ev.target.classList.contains('todo-check')) toggleTodo(ev.target);
  if (ev.target.dataset && ev.target.dataset.timelineFilter) {
    state.timelineFilters[ev.target.dataset.timelineFilter] = ev.target.value;
    renderStage();
  }
  if (ev.target.id === 'imgFile' && ev.target.files && ev.target.files.length) {
    handleFiles(ev.target.files);
    ev.target.value = '';
  }
  if (ev.target.id === 'docFile' && ev.target.files && ev.target.files.length) {
    handleDocFiles(ev.target.files);
    ev.target.value = '';
  }
});
/* 细节框里直接粘贴 / 拖入截图 */
document.addEventListener('paste', (ev) => {
  const files = ev.clipboardData && ev.clipboardData.files;
  if (files && files.length && ev.target && ev.target.name === 'e-detail') {
    ev.preventDefault();
    handleFiles(files);
  }
});
document.addEventListener('dragover', (ev) => {
  if (ev.target && ev.target.name === 'e-detail') ev.preventDefault();
});
document.addEventListener('drop', (ev) => {
  const files = ev.dataTransfer && ev.dataTransfer.files;
  if (files && files.length && ev.target && ev.target.name === 'e-detail') {
    ev.preventDefault();
    handleFiles(files);
  }
});
$('#lightbox').addEventListener('click', closeLightbox);
$('#filePreview').addEventListener('click', (ev) => { if (ev.target === ev.currentTarget) closePreview(); });
$('#pvClose').addEventListener('click', closePreview);
$('#pvCompareRun').addEventListener('click', comparePreviewFiles);
$('#pvPairRun').addEventListener('click', runMetricsPair);
$('#spGo').addEventListener('click', listServerDir);
$('#spPath').addEventListener('keydown', (ev) => { if (ev.key === 'Enter') { ev.preventDefault(); listServerDir(); } });
$('#spAttach').addEventListener('click', attachSelectedServerFiles);
$('#spClose').addEventListener('click', closeServerPicker);
$('#serverPicker').addEventListener('click', (ev) => { if (ev.target === ev.currentTarget) closeServerPicker(); });
$('#lbPrev').addEventListener('click', (ev) => { ev.stopPropagation(); lightboxStep(-1); });
$('#lbNext').addEventListener('click', (ev) => { ev.stopPropagation(); lightboxStep(1); });
/* 命令块一键复制（选中文本时不打扰）；围栏块的提示符是 ⌨，命令行块是 $ */
document.addEventListener('click', (ev) => {
  const block = ev.target.closest('.t-cmd');
  if (!block || ev.target.closest('a') || String(window.getSelection() || '').trim()) return;
  copyText(block.textContent.replace(/^[$⌨]\s?/, '').replace(/\s+$/, ''), '命令已复制');
});
document.addEventListener('submit', (ev) => {
  if (ev.target.id === 'todoForm') { ev.preventDefault(); addTodo(ev.target); }
});

$('#taskForm').addEventListener('submit', onTaskSubmit);
$('#taskForm').elements.template.addEventListener('change', (ev) => {
  const form = $('#taskForm');
  const oldTitle = form.title.dataset.templateTitle || '';
  const template = WORKFLOW_TEMPLATES[ev.target.value];
  if (template && (!form.title.value.trim() || form.title.value === oldTitle)) form.title.value = template.title;
  if (!template && form.title.value === oldTitle) form.title.value = '';
  form.title.dataset.templateTitle = template?.title || '';
  updateTemplateHint(ev.target.value, form);
});
$('#btnNewTask').addEventListener('click', () => openTaskModal(null));
$('#btnNewManga').addEventListener('click', () => openMangaModal());
$('#btnRenameManga').addEventListener('click', () => {
  const manga = (state.data.mangas || []).find((item) => item.id === state.activeMangaId);
  if (manga) openMangaModal(manga);
});
$('#mangaSelect').addEventListener('change', (ev) => switchManga(ev.target.value));
$('#mangaForm').addEventListener('submit', onMangaSubmit);
$('#btnExport').addEventListener('click', exportMd);
$('#btnExportManga').addEventListener('click', exportMangaMd);
$('#btnLogout').addEventListener('click', async () => {
  try { await fetch('/api/logout', { method: 'POST', headers: { 'X-Requested-With': 'manga-log' } }); } catch (_) { /* 忽略 */ }
  location.href = '/';
});
$('#modal').addEventListener('click', (ev) => { if (ev.target === ev.currentTarget) closeTaskModal(); });
$('#mangaModal').addEventListener('click', (ev) => { if (ev.target === ev.currentTarget) closeMangaModal(); });
$('#importFile').addEventListener('change', onImportFile);
$('#quickForm').addEventListener('submit', onQuickSubmit);
$('#quickInput').addEventListener('input', updateQuickKind);
$('#quickTask').addEventListener('change', (ev) => {
  state.currentId = ev.target.value;
  state.editEntryId = null;
  renderToc(); renderStage();
});
$('#tabToc').addEventListener('click', () => { state.view = 'toc'; renderTabs(); renderStage(); });
$('#tabTimeline').addEventListener('click', () => { state.view = 'timeline'; renderTabs(); renderStage(); });
$('#tabInbox').addEventListener('click', () => { state.view = 'inbox'; renderTabs(); renderStage(); });
$('#tabStats').addEventListener('click', () => { state.view = 'stats'; renderTabs(); renderStage(); });
$('#search').addEventListener('input', (ev) => {
  state.query = ev.target.value;
  renderToc(); renderStage();
});

document.addEventListener('keydown', (ev) => {
  if (ev.key === 'Escape') {
    closeTaskModal();
    closeMangaModal();
    closeLightbox();
    closePreview();
    closeServerPicker();
    return;
  }
  if (!$('#lightbox').classList.contains('hidden')) {
    if (ev.key === 'ArrowLeft') { lightboxStep(-1); return; }
    if (ev.key === 'ArrowRight') { lightboxStep(1); return; }
  }
  if ((ev.ctrlKey || ev.metaKey) && ev.key === 'Enter' && !$('#modal').classList.contains('hidden')) {
    $('#taskForm').requestSubmit();
    return;
  }
  if ((ev.ctrlKey || ev.metaKey) && ev.key === 'Enter' && !$('#mangaModal').classList.contains('hidden')) {
    $('#mangaForm').requestSubmit();
    return;
  }
  const inField = /^(INPUT|TEXTAREA|SELECT)$/.test(ev.target.tagName);
  if (inField || !$('#modal').classList.contains('hidden') || !$('#mangaModal').classList.contains('hidden')) return;
  if (ev.key === 'n' || ev.key === 'N') { ev.preventDefault(); openTaskModal(null); }
  else if (ev.key === '/') {
    ev.preventDefault();
    state.view = 'toc';
    renderTabs(); renderStage();
    $('#search').focus();
  }
});

/* ---------------- 启动 ---------------- */
/* 「退场」按钮与用户名默认隐藏（单用户模式本就不需要，避免首屏闪一下），
 * 只有 whoami 表明是多用户模式时才显示。 */
api('/api/whoami').then((w) => {
  if (!w.multi) return;
  const chip = $('#whoamiChip');
  const logout = $('#btnLogout');
  if (chip) { chip.textContent = `👤 ${w.user}`; chip.classList.remove('hidden'); }
  if (logout) logout.classList.remove('hidden');
}).catch(() => { /* 拿不到就保持隐藏，不影响主流程 */ });

refresh().catch((e) => {
  $('#stage').innerHTML = `<div class="empty-hero"><div class="big">加载失败…</div><p>${esc(e.message)}</p></div>`;
});
