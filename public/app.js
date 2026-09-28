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
  data: { tasks: [] },
  currentId: null,
  query: '',
  view: 'toc',          // toc | stats
  statusFilter: '全部',
  editTaskId: null,
  editEntryId: null,
  pendingImport: null,  // 待确认的导入数据
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
  const res = await fetch(path, {
    headers: { 'Content-Type': 'application/json', 'X-Requested-With': 'manga-log' },
    ...opts,
  });
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
    (state.statusFilter === '全部' || t.status === state.statusFilter) && taskMatches(t, q));
}
function renderToc() {
  const q = state.query.trim().toLowerCase();
  const list = $('#taskList');
  const tasks = visibleTasks();
  const filtersActive = q || state.statusFilter !== '全部';
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
function renderTabs() {
  const stats = state.view === 'stats';
  $('#tabToc').classList.toggle('active', !stats);
  $('#tabStats').classList.toggle('active', stats);
  $('#tocEn').textContent = stats ? 'EXTRA' : 'CONTENTS';
  $('#statusChips').classList.toggle('hidden', stats);
  $('#search').classList.toggle('hidden', stats);
  $('#taskList').classList.toggle('hidden', stats);
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

function panelHTML(e, idx) {
  const sfx = KIND_SFX[e.kind] || '记';
  const badgeCls = e.result === '成功' ? 'ok' : e.result === '失败' ? 'bad' : 'pending';
  const badgeTxt = e.result === '成功' ? '成功' : e.result === '失败' ? '✗ 失败' : '待验证';
  const r = idx % 3;
  return `
  <article class="panel r${r + 1}" data-eid="${esc(e.id)}">
    <div class="p-no">${pad(idx + 1)}</div>
    <div class="stamp roughen" title="${esc(e.kind)}">${esc(sfx)}</div>
    <div class="e-head"><span class="e-kind">${esc(e.kind)}</span><span class="e-time mono">${esc(e.time || '')}</span></div>
    <h4 class="e-title">${esc(e.title)}</h4>
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
      <button class="btn-icon" data-action="edit-entry" data-id="${esc(e.id)}" title="修改此格">✎ 改</button>
      <button class="btn-icon" data-action="del-entry" data-id="${esc(e.id)}" title="撕掉此格">✕ 撕</button>
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
  const e = editing || { time: nowLocal().replace(' ', 'T'), kind: '配置', result: '待验证', title: '', detail: '' };
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

function renderStage() {
  const stage = $('#stage');
  if (state.view === 'stats') return renderStats(stage);
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
        ${(t.tags || []).map((g) => `<span class="chip tag">#${esc(g)}</span>`).join('')}
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
    if (!state.editEntryId) form.querySelector('input[name="e-title"]')?.focus({ preventScroll: true });
  }
  if (state.editEntryId) $('#entryFormPanel')?.scrollIntoView({ behavior: 'smooth', block: 'center' });
}

function renderAll() {
  renderTicker();
  renderTabs();
  renderChips();
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
    <section class="stat-block r2">
      <h3 class="stat-title">备份与搬家</h3>
      <p class="stat-note">全部记录都存在本地 data.json。导出 JSON 备份留底；导入备份会<b>覆盖</b>当前全部记录。注意：备份 JSON 不含图片文件，贴过的图都在 uploads/ 目录，搬家时把整个 uploads 目录一起拷走。</p>
      <div class="backup-row">
        <button class="btn rs" data-action="export-backup">⇩ 导出备份</button>
        <button class="btn rs" data-action="import-backup">⇒ 导入备份</button>
      </div>
    </section>
  </div>
  <div class="tsuzuku"><span class="jp">つづく</span><span class="en mono">TO BE CONTINUED · 未完待续</span></div>`;
}

/* ---------------- 附件：图片、文档与表格 ---------------- */
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
      toast('文档挂上了！', '挂！');
    } catch (e) { toast(e.message, '啊！'); }
  }
}

function openLightbox(url) {
  $('#lightboxImg').src = url;
  $('#lightbox').classList.remove('hidden');
}
function closeLightbox() {
  $('#lightbox').classList.add('hidden');
  $('#lightboxImg').src = '';
}

/* ---------------- 附件预览 ---------------- */
const PREVIEW_TEXT_EXTS = new Set(['txt', 'log', 'md', 'markdown', 'json', 'yaml', 'yml', 'xml', 'conf', 'cfg', 'ini', 'env', 'csv', 'tsv', 'sql', 'py', 'sh', 'bash', 'js', 'ts', 'css', 'html', 'htm', 'pem', 'key', 'csr', 'cnf', 'service']);
const PREVIEW_TABLE_EXTS = new Set(['csv', 'tsv']);
const PREVIEW_TEXT_CAP = 5 * 1024 * 1024;   // 文本预览上限
const PREVIEW_LINE_CAP = 2000;              // 最多预览行数

async function previewFile(url, name, size) {
  const ext = extOf(name);
  $('#pvName').textContent = name;
  $('#pvSize').textContent = fmtSize(size);
  const dl = $('#pvDownload');
  dl.href = url;
  dl.setAttribute('download', name);
  const body = $('#pvBody');
  body.innerHTML = '<div class="pv-note">读取中…</div>';
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
    body.innerHTML = `<div class="pv-count mono">${counter}</div><pre class="pv-text">${shown || '（空文件）'}</pre>`;
  } catch (e) {
    body.innerHTML = `<div class="pv-note">预览失败：${esc(e.message)}</div>`;
  }
}
function closePreview() {
  $('#filePreview').classList.add('hidden');
  $('#pvBody').innerHTML = '';
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
  const empty = !d.editId && !d.v.title && !d.v.detail && !d.images.length && !d.tables.length && !d.files.length;
  if (empty) return;
  form['e-time'].value = d.v.time;
  form['e-kind'].value = d.v.kind;
  form['e-result'].value = d.v.result;
  form['e-title'].value = d.v.title;
  form['e-detail'].value = d.v.detail;
  formImages = d.images;
  formTables = d.tables;
  formFiles = d.files;
  renderImgStrip();
  renderFileStrip();
  renderTableEditors();
}

async function refresh(opts = {}) {
  const fresh = await api('/api/data');
  const draft = opts.preserve ? captureEntryDraft() : null;
  state.data = fresh;
  if (!currentTask() && state.data.tasks.length) state.currentId = state.data.tasks[0].id;
  if (!state.data.tasks.length) state.currentId = null;
  renderAll();
  if (draft) restoreEntryDraft(draft);
  lastDataHash = JSON.stringify(state.data);
}

/* 自动同步：每 10 秒检测一次外部写入（如 import_history.py 录入），有变化就重绘并保留草稿 */
function isUserTypingInStage() {
  const el = document.activeElement;
  if (!el) return false;
  if (!/^(INPUT|TEXTAREA|SELECT)$/.test(el.tagName)) return false;
  const stage = $('#stage');
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
  $('#modal').classList.remove('hidden');
  f.title.focus();
}
function closeTaskModal() {
  $('#modal').classList.add('hidden');
  state.editTaskId = null;
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
    await refresh();
  } catch (e) { toast(e.message, '啊！'); }
}

function editEntry(btn) {
  state.editEntryId = btn.dataset.id;
  renderStage();
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
function buildMd(tasks) {
  const lines = [];
  const total = tasks.reduce((s, t) => s + t.entries.length, 0);
  lines.push('# 服务器改动 · 漫画志', '');
  lines.push(`> 导出于 ${nowLocal()} · 共 ${tasks.length} 话 / ${total} 格`, '');
  tasks.forEach((t) => {
    const idx = state.data.tasks.indexOf(t) + 1;
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
function exportMd() {
  download(`漫画志_全稿_${dayStr()}.md`, buildMd(state.data.tasks));
  toast('全稿导出齐了！', '齐！');
}
function exportTaskMd() {
  const t = currentTask();
  if (!t) return;
  download(`漫画志_${t.title.slice(0, 20)}_${dayStr()}.md`, buildMd([t]));
  toast('本话导出齐了！', '齐！');
}
function exportBackup() {
  download(`漫画志_备份_${dayStr()}.json`, JSON.stringify(state.data, null, 2), 'application/json');
  toast('备份导出齐了！', '齐！');
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
    if (!obj || !Array.isArray(obj.tasks)) { toast('缺少 tasks 字段，不是漫画志备份', '啊！'); return; }
    state.pendingImport = obj;
    const btn = document.querySelector('[data-action="import-backup"]');
    if (btn) {
      btn.classList.add('armed');
      btn.textContent = `⚠ 确认覆盖（${obj.tasks.length} 话）？`;
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
    toast(`已导入 ${res.tasks} 话，覆写完成！`, '齐！');
    await refresh();
  } catch (e) {
    resetImportBtns();
    toast(e.message, '啊！');
  }
}

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
  else if (act === 'del-task') deleteTask(el);
  else if (act === 'edit-entry') editEntry(el);
  else if (act === 'del-entry') deleteEntry(el);
  else if (act === 'cancel-edit-entry') { state.editEntryId = null; renderStage(); }
  else if (act === 'del-todo') deleteTodo(el);
  else if (act === 'filter-status') {
    state.statusFilter = el.dataset.status;
    renderChips(); renderToc(); renderStage();
  } else if (act === 'export-task') exportTaskMd();
  else if (act === 'export-backup') exportBackup();
  else if (act === 'import-backup') {
    if (state.pendingImport) confirmImport();
    else $('#importFile').click();
  } else if (act === 'view-img') openLightbox(el.dataset.url);
  else if (act === 'preview-file') {
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
  else if (act === 'del-img') {
    formImages = formImages.filter((x) => x.url !== el.dataset.url);
    renderImgStrip();
  } else if (act === 'del-file') {
    formFiles = formFiles.filter((x) => x.url !== el.dataset.url);
    renderFileStrip();
  } else if (act === 'add-table') {
    if (formTables.length >= 5) { toast('一格最多 5 张表', '满！'); return; }
    syncTablesFromDom();
    formTables.push({ id: genId('tb'), title: '', cols: ['项目', '数值'], rows: [['', '']] });
    renderTableEditors();
  } else if (act === 'del-table') {
    syncTablesFromDom();
    formTables.splice(+el.dataset.ti, 1);
    renderTableEditors();
  } else if (act === 'add-row') {
    syncTablesFromDom();
    const tb = formTables[+el.dataset.ti];
    if (!tb) return;
    if (tb.rows.length >= 100) { toast('最多 100 行', '满！'); return; }
    tb.rows.push(tb.cols.map(() => ''));
    renderTableEditors();
  } else if (act === 'add-col') {
    syncTablesFromDom();
    const tb = formTables[+el.dataset.ti];
    if (!tb) return;
    if (tb.cols.length >= 12) { toast('最多 12 列', '满！'); return; }
    tb.cols.push('');
    tb.rows.forEach((r) => r.push(''));
    renderTableEditors();
  } else if (act === 'del-col') {
    syncTablesFromDom();
    const tb = formTables[+el.dataset.ti];
    if (!tb || tb.cols.length <= 1) return;
    tb.cols.pop();
    tb.rows.forEach((r) => r.pop());
    renderTableEditors();
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
document.addEventListener('change', (ev) => {
  if (ev.target.classList && ev.target.classList.contains('todo-check')) toggleTodo(ev.target);
});
document.addEventListener('submit', (ev) => {
  if (ev.target.id === 'todoForm') { ev.preventDefault(); addTodo(ev.target); }
});

$('#taskForm').addEventListener('submit', onTaskSubmit);
$('#btnNewTask').addEventListener('click', () => openTaskModal(null));
$('#btnExport').addEventListener('click', exportMd);
$('#btnLogout').addEventListener('click', async () => {
  try { await fetch('/api/logout', { method: 'POST', headers: { 'X-Requested-With': 'manga-log' } }); } catch (_) { /* 忽略 */ }
  location.href = '/';
});
$('#modal').addEventListener('click', (ev) => { if (ev.target === ev.currentTarget) closeTaskModal(); });
$('#importFile').addEventListener('change', onImportFile);
$('#quickForm').addEventListener('submit', onQuickSubmit);
$('#quickInput').addEventListener('input', updateQuickKind);
$('#quickTask').addEventListener('change', (ev) => {
  state.currentId = ev.target.value;
  state.editEntryId = null;
  renderToc(); renderStage();
});
$('#tabToc').addEventListener('click', () => { state.view = 'toc'; renderTabs(); renderStage(); });
$('#tabStats').addEventListener('click', () => { state.view = 'stats'; renderTabs(); renderStage(); });
$('#search').addEventListener('input', (ev) => {
  state.query = ev.target.value;
  renderToc(); renderStage();
});

document.addEventListener('keydown', (ev) => {
  if (ev.key === 'Escape') {
    closeTaskModal();
    closeLightbox();
    closePreview();
    return;
  }
  if ((ev.ctrlKey || ev.metaKey) && ev.key === 'Enter' && !$('#modal').classList.contains('hidden')) {
    $('#taskForm').requestSubmit();
    return;
  }
  const inField = /^(INPUT|TEXTAREA|SELECT)$/.test(ev.target.tagName);
  if (inField || !$('#modal').classList.contains('hidden')) return;
  if (ev.key === 'n' || ev.key === 'N') { ev.preventDefault(); openTaskModal(null); }
  else if (ev.key === '/') {
    ev.preventDefault();
    state.view = 'toc';
    renderTabs(); renderStage();
    $('#search').focus();
  }
});

/* ---------------- 启动 ---------------- */
refresh().catch((e) => {
  $('#stage').innerHTML = `<div class="empty-hero"><div class="big">加载失败…</div><p>${esc(e.message)}</p></div>`;
});
