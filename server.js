'use strict';
/*
 * 服务器改动 · 漫画志 —— 本地手稿风改动记录
 * 零依赖：只用 Node 内置模块，`node server.js` 即可启动。
 * 数据保存在同目录 data.json，绑定 127.0.0.1，仅本机可访问。
 */
const http = require('http');
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
const { spawn } = require('child_process');

const ROOT = __dirname;
const PUBLIC_DIR = path.join(ROOT, 'public');
const DATA_FILE = path.join(ROOT, 'data.json');
const UPLOAD_DIR = path.join(ROOT, 'uploads');

/* 可选 config.json：{ "accessCode": "口令", "host": "0.0.0.0", "port": 4780 }（环境变量优先） */
function loadConfig() {
  try { return JSON.parse(fs.readFileSync(path.join(ROOT, 'config.json'), 'utf8')); }
  catch (_) { return {}; }
}
const CFG = loadConfig();
const ACCESS_CODE = String(process.env.ACCESS_CODE ?? CFG.accessCode ?? '').trim();
const BIND_HOST = process.env.HOST || CFG.host || '127.0.0.1';
const BASE_PORT = Number(process.env.PORT) || Number(CFG.port) || 4780;

const KINDS = ['配置', '代码', '部署', '修复', '排查', '回滚', '其他'];
const RESULTS = ['成功', '失败', '待验证'];
const STATUSES = ['进行中', '已完成', '搁置'];

/* ---------------- 数据层 ---------------- */
function pad(n) { return String(n).padStart(2, '0'); }
function dayStr(offsetDays) {
  const d = new Date();
  d.setDate(d.getDate() + (offsetDays || 0));
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}
function stampLocal(offsetDays = 0, hhmm = '00:00') {
  return `${dayStr(offsetDays)} ${hhmm}`;
}
function uid(prefix) {
  return `${prefix}_${Date.now().toString(36)}${Math.random().toString(36).slice(2, 7)}`;
}

function seedData() {
  const t = (offset, hhmm) => stampLocal(offset, hhmm);
  return {
    version: 1,
    seededAt: new Date().toISOString(),
    tasks: [
      {
        id: uid('t'), title: 'Nginx HTTPS 证书更换', host: 'prod-web-01',
        status: '进行中', tags: ['运维', '安全'], note: '证书 10-08 到期，续期后留意子域。',
        createdAt: t(-2, '10:00'), updatedAt: t(-2, '10:43'),
        entries: [
          { id: uid('e'), time: t(-2, '10:12'), kind: '配置', title: '备份 nginx.conf 并核对到期日', detail: 'cp /etc/nginx/nginx.conf{,.bak-0926}\nopenssl x509 -enddate 确认当前证书 10-08 到期。', result: '成功' },
          { id: uid('e'), time: t(-2, '10:40'), kind: '部署', title: 'certbot 续期并 reload', detail: 'certbot renew 通过，nginx -t 校验后 reload；curl -I 验证证书链 OK。', result: '成功' },
          { id: uid('e'), time: t(-2, '10:43'), kind: '排查', title: 'assets 子域证书未更新', detail: 'assets.example.com 使用独立 server 块，漏了一张证书，待补签。', result: '待验证' },
        ],
      },
      {
        id: uid('t'), title: '数据盘告警清理', host: 'db-master',
        status: '已完成', tags: ['运维', '告警'], note: 'WAL 堆积导致磁盘 92%，已回落；计划加水位巡检。',
        todos: [{ id: uid('td'), text: '加磁盘水位巡检脚本', done: false }],
        createdAt: t(-4, '02:20'), updatedAt: t(-4, '03:05'),
        entries: [
          { id: uid('e'), time: t(-4, '02:31'), kind: '排查', title: '磁盘 92% 告警定位', detail: 'du 逐层排查到 /var/lib/postgresql，WAL 归档堆积为主因。', result: '成功' },
          { id: uid('e'), time: t(-4, '03:02'), kind: '配置', title: '归档保留 7 天 → 3 天', detail: '调整归档清理脚本保留窗口，先观察一周。', result: '成功' },
          { id: uid('e'), time: t(-4, '03:05'), kind: '修复', title: '清理后磁盘回落到 61%', detail: '告警恢复。遗留事项：加磁盘水位巡检脚本。', result: '成功' },
        ],
      },
      {
        id: uid('t'), title: '订单服务灰度发布 v2.3.0', host: 'app-cluster',
        status: '进行中', tags: ['部署', '发布'], note: '退款重试逻辑上线，灰度 10% 中。',
        createdAt: t(-1, '16:00'), updatedAt: t(0, '09:30'),
        entries: [
          { id: uid('e'), time: t(-1, '16:00'), kind: '代码', title: '合并 feature/refund-retry', detail: 'PR #412 review 通过，CI 全绿。', result: '成功' },
          { id: uid('e'), time: t(-1, '18:20'), kind: '部署', title: '灰度 10% 实例', detail: '错误率 0.02%，耗时无异常，观察中。', result: '成功' },
          { id: uid('e'), time: t(0, '09:30'), kind: '排查', title: '慢日志出现 2 条慢查询', detail: '集中在 orders_refund 表，考虑补 (user_id, created_at) 索引。', result: '待验证' },
        ],
      },
    ],
  };
}

function loadData() {
  try {
    const parsed = JSON.parse(fs.readFileSync(DATA_FILE, 'utf8'));
    if (parsed && Array.isArray(parsed.tasks)) return parsed;
    throw new Error('bad shape');
  } catch (e) {
    if (fs.existsSync(DATA_FILE)) {
      try { fs.renameSync(DATA_FILE, `${DATA_FILE}.bak-${Date.now()}`); } catch (_) { /* 忽略备份失败 */ }
      console.warn('[!] data.json 无法解析，已备份为 .bak 文件并重建示例数据。');
    }
    const fresh = seedData();
    try { saveData(fresh); } catch (_) { /* 首次写盘失败不致命 */ }
    return fresh;
  }
}
function saveData(d) {
  const tmp = `${DATA_FILE}.tmp`;
  fs.writeFileSync(tmp, JSON.stringify(d, null, 2), 'utf8');
  fs.renameSync(tmp, DATA_FILE);
}

let data = loadData();

/* ---------------- 工具 ---------------- */
const S = (v, max) => (typeof v === 'string' ? v.trim().slice(0, max) : '');
const findTask = (id) => data.tasks.find((t) => t.id === id);

function normalizeTags(v) {
  if (!Array.isArray(v)) return [];
  return v.map((x) => S(x, 20)).filter(Boolean).slice(0, 8);
}
function normalizeTodos(v) {
  if (!Array.isArray(v)) return [];
  return v.slice(0, 50).map((x) => ({
    id: (x && typeof x.id === 'string' && /^[\w-]+$/.test(x.id)) ? x.id : uid('td'),
    text: S(x && x.text, 200),
    done: !!(x && x.done),
  })).filter((t) => t.text);
}

const IMG_TYPES = { 'image/png': 'png', 'image/jpeg': 'jpg', 'image/gif': 'gif', 'image/webp': 'webp' };
const DOC_EXTS = new Set([
  'txt', 'log', 'md', 'markdown', 'csv', 'tsv', 'json', 'yaml', 'yml', 'xml', 'conf', 'cfg', 'ini', 'env',
  'pdf', 'doc', 'docx', 'xls', 'xlsx', 'ppt', 'pptx', 'odt', 'ods',
  'py', 'sh', 'bash', 'sql', 'js', 'ts', 'css', 'java', 'c', 'cpp', 'h', 'go', 'rs', 'ipynb',
  'zip', 'tar', 'gz', 'tgz', '7z', 'rar',
  'pem', 'key', 'csr', 'cnf',
]);
const DOC_MIME = {
  pdf: 'application/pdf', zip: 'application/zip', gz: 'application/gzip', tgz: 'application/gzip',
  '7z': 'application/x-7z-compressed', rar: 'application/vnd.rar', tar: 'application/x-tar',
  doc: 'application/msword', docx: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
  xls: 'application/vnd.ms-excel', xlsx: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
  ppt: 'application/vnd.ms-powerpoint', pptx: 'application/vnd.openxmlformats-officedocument.presentationml.presentation',
  json: 'application/json', xml: 'application/xml', csv: 'text/csv',
};
const DOC_MAX = 50 * 1024 * 1024; // 单个文档 50MB

function normalizeImages(v) {
  if (!Array.isArray(v)) return [];
  return v.slice(0, 9).map((x) => ({
    id: (x && typeof x.id === 'string' && /^[\w-]+$/.test(x.id)) ? x.id : uid('im'),
    url: (x && typeof x.url === 'string' && /^\/uploads\/[\w.-]+$/.test(x.url)) ? x.url : '',
    name: S(x && x.name, 120) || '图片',
  })).filter((x) => x.url);
}
function normalizeFiles(v) {
  if (!Array.isArray(v)) return [];
  return v.slice(0, 10).map((x) => ({
    id: (x && typeof x.id === 'string' && /^[\w-]+$/.test(x.id)) ? x.id : uid('f'),
    url: (x && typeof x.url === 'string' && /^\/uploads\/[\w.-]+$/.test(x.url)) ? x.url : '',
    name: S(x && x.name, 200) || '附件',
    size: (x && Number.isFinite(+x.size)) ? Math.max(0, Math.min(Math.round(+x.size), 2147483647)) : 0,
  })).filter((x) => x.url);
}
function normalizeTables(v) {
  if (!Array.isArray(v)) return [];
  return v.slice(0, 5).map((x) => {
    const cols = Array.isArray(x && x.cols) ? x.cols.slice(0, 12).map((c) => S(c, 40)) : [];
    const rows = Array.isArray(x && x.rows)
      ? x.rows.slice(0, 200).map((r) => (Array.isArray(r) ? r.slice(0, 12).map((c) => S(c, 200)) : []))
      : [];
    return {
      id: (x && typeof x.id === 'string' && /^[\w-]+$/.test(x.id)) ? x.id : uid('tb'),
      title: S(x && x.title, 120),
      cols,
      rows,
    };
  }).filter((x) => x.cols.length || x.rows.length);
}
function removeUploads(urls) {
  for (const u of urls || []) {
    if (!/^\/uploads\/[\w.-]+$/.test(u)) continue;
    try { fs.unlinkSync(path.join(UPLOAD_DIR, path.basename(u))); } catch (_) { /* 文件可能已不在 */ }
  }
}
function normalizeEntry(body) {
  const title = S(body.title, 120);
  if (!title) return { error: '改动标题不能为空' };
  return {
    value: {
      id: body.id || uid('e'),
      time: S(body.time, 16) || stampLocal(0, `${pad(new Date().getHours())}:${pad(new Date().getMinutes())}`),
      kind: KINDS.includes(body.kind) ? body.kind : '其他',
      result: RESULTS.includes(body.result) ? body.result : '待验证',
      title,
      detail: S(body.detail, 5000),
      images: normalizeImages(body.images),
      tables: normalizeTables(body.tables),
      files: normalizeFiles(body.files),
    },
  };
}

/* ---------------- 口令锁（设置了 ACCESS_CODE 才启用） ---------------- */
const AUTH_TOKEN = ACCESS_CODE
  ? crypto.createHash('sha256').update(`manga-log::${ACCESS_CODE}`).digest('hex')
  : '';
function getCookie(req, key) {
  for (const part of String(req.headers.cookie || '').split(';')) {
    const [k, ...v] = part.trim().split('=');
    if (k === key) return v.join('=');
  }
  return '';
}
function isAuthed(req) {
  if (!ACCESS_CODE) return true;
  return getCookie(req, 'auth') === AUTH_TOKEN;
}

/* 内网免口令：直连的内网/本机来源直接放行；经隧道来的公网请求必须口令。
 * 判定时以 Cloudflare 注入的 CF-Connecting-IP（真实访客 IP）优先——
 * 因为隧道请求的 socket 地址恒为 127.0.0.1，只看来源 IP 会把公网口令锁整个架空。 */
function isPrivateIp(ip) {
  if (!ip) return false;
  let s = String(ip).toLowerCase().trim();
  if (s.startsWith('::ffff:')) s = s.slice(7); // IPv4-mapped IPv6
  if (s === '::1') return true;
  if (/^127\./.test(s)) return true;                 // loopback
  if (/^10\./.test(s)) return true;                  // 10/8
  if (/^192\.168\./.test(s)) return true;            // 192.168/16
  if (/^169\.254\./.test(s)) return true;            // link-local
  if (/^172\.(1[6-9]|2\d|3[01])\./.test(s)) return true; // 172.16/12
  if (/^100\.(6[4-9]|[7-9]\d|1[01]\d|12[0-7])\./.test(s)) return true; // 100.64/10（Tailscale 等）
  if (/^f[cd][0-9a-f]{2}:/.test(s)) return true;     // fc00::/7 IPv6 内网
  if (/^fe[89ab][0-9a-f]{2}:/.test(s)) return true;  // fe80::/10 IPv6 链路本地
  return false;
}
function isLanRequest(req) {
  const cf = req.headers['cf-connecting-ip'];
  const ip = cf ? String(cf).trim() : (req.socket.remoteAddress || '');
  return isPrivateIp(ip);
}
const loginFails = new Map();
function failRecord(ip) {
  const now = Date.now();
  let rec = loginFails.get(ip);
  if (!rec || now > rec.reset) { rec = { n: 0, reset: now + 5 * 60 * 1000 }; loginFails.set(ip, rec); }
  return rec;
}
function serveLogin(res) {
  res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8', 'Cache-Control': 'no-store' });
  res.end(`<!DOCTYPE html><html lang="zh-CN"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>验明正身 · 漫画志</title></head>
<body style="margin:0;min-height:100vh;display:flex;align-items:center;justify-content:center;background:#efe9da;background-image:radial-gradient(circle,rgba(22,21,19,.08) 1px,transparent 1.4px);background-size:7px 7px;font-family:KaiTi,'楷体',STKaiti,serif;color:#161513;">
<div style="background:#f6f2e8;border:2.5px solid #161513;border-radius:255px 15px 225px 15px/15px 225px 15px 255px;box-shadow:8px 8px 0 rgba(22,21,19,.85);padding:38px 44px;max-width:420px;transform:rotate(-.6deg);text-align:center;">
<div style="font-family:KaiTi,serif;font-size:34px;letter-spacing:6px;">验明正身</div>
<div style="font-size:13px;color:rgba(22,21,19,.55);letter-spacing:3px;margin:4px 0 20px;">SERVER MANGA LOG · 口令确认后开演</div>
<form id="f" style="display:flex;gap:10px;">
<input id="code" type="password" autofocus placeholder="输入口令…" style="flex:1;font-size:16px;padding:9px 12px;border:2px solid #161513;background:#fbf8f0;border-radius:14px 5px 16px 6px/6px 16px 5px 14px;outline:none;font-family:inherit;">
<button style="border:2px solid #161513;background:#161513;color:#f6f2e8;font-weight:700;font-size:15px;letter-spacing:2px;padding:8px 16px;border-radius:14px 5px 16px 6px/6px 16px 5px 14px;box-shadow:3px 3px 0 rgba(22,21,19,.4);cursor:pointer;font-family:inherit;">开演</button>
</form>
<div id="msg" style="margin-top:14px;font-size:13.5px;min-height:20px;color:#161513;"></div>
</div>
<script>
document.getElementById('f').addEventListener('submit', async (ev) => {
  ev.preventDefault();
  const msg = document.getElementById('msg');
  try {
    const r = await fetch('/api/login', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ code: document.getElementById('code').value }) });
    const b = await r.json().catch(() => ({}));
    if (r.ok) { location.href = '/'; return; }
    msg.textContent = b.error || '口令不对';
  } catch (e) { msg.textContent = '网络异常，稍后再试'; }
});
</script>
</body></html>`);
}

/* ---------------- API ---------------- */
const routes = [
  ['GET', /^\/api\/data$/, (req, res) => json(res, 200, data)],

  ['POST', /^\/api\/tasks$/, (req, res, m, body) => {
    const title = S(body.title, 120);
    if (!title) return json(res, 400, { error: '话数标题不能为空' });
    const now = stampLocal(0, `${pad(new Date().getHours())}:${pad(new Date().getMinutes())}`);
    const task = {
      id: uid('t'), title,
      host: S(body.host, 80),
      status: STATUSES.includes(body.status) ? body.status : '进行中',
      tags: normalizeTags(body.tags),
      note: S(body.note, 1000),
      todos: normalizeTodos(body.todos),
      createdAt: now, updatedAt: now, entries: [],
    };
    data.tasks.unshift(task);
    saveData(data);
    return json(res, 200, { ok: true, task });
  }],

  ['PATCH', /^\/api\/tasks\/([A-Za-z0-9_-]+)$/, (req, res, m, body) => {
    const task = findTask(m[1]);
    if (!task) return json(res, 404, { error: '没有这一话' });
    if (body.title !== undefined) {
      const title = S(body.title, 120);
      if (!title) return json(res, 400, { error: '话数标题不能为空' });
      task.title = title;
    }
    if (body.host !== undefined) task.host = S(body.host, 80);
    if (body.status !== undefined && STATUSES.includes(body.status)) task.status = body.status;
    if (body.tags !== undefined) task.tags = normalizeTags(body.tags);
    if (body.note !== undefined) task.note = S(body.note, 1000);
    if (body.todos !== undefined) task.todos = normalizeTodos(body.todos);
    task.updatedAt = stampLocal(0, `${pad(new Date().getHours())}:${pad(new Date().getMinutes())}`);
    saveData(data);
    return json(res, 200, { ok: true, task });
  }],

  ['DELETE', /^\/api\/tasks\/([A-Za-z0-9_-]+)$/, (req, res, m) => {
    const task = findTask(m[1]);
    if (!task) return json(res, 404, { error: '没有这一话' });
    data.tasks = data.tasks.filter((t) => t.id !== m[1]);
    (task.entries || []).forEach((e) => {
      removeUploads((e.images || []).map((x) => x.url));
      removeUploads((e.files || []).map((x) => x.url));
    });
    saveData(data);
    return json(res, 200, { ok: true });
  }],

  ['POST', /^\/api\/upload$/, (req, res, m, body) => {
    const m2 = typeof body.dataUrl === 'string'
      ? body.dataUrl.match(/^data:(image\/(?:png|jpeg|gif|webp));base64,([A-Za-z0-9+/=\s]+)$/)
      : null;
    if (!m2) return json(res, 400, { error: '仅支持 png / jpg / gif / webp 图片' });
    const buf = Buffer.from(m2[2].replace(/\s+/g, ''), 'base64');
    if (!buf.length) return json(res, 400, { error: '图片内容为空' });
    if (buf.length > 8 * 1024 * 1024) return json(res, 400, { error: '图片需在 8MB 以内' });
    const fname = `${uid('u')}.${IMG_TYPES[m2[1]]}`;
    try {
      fs.mkdirSync(UPLOAD_DIR, { recursive: true });
      fs.writeFileSync(path.join(UPLOAD_DIR, fname), buf);
    } catch (_) {
      return json(res, 500, { error: '图片保存失败' });
    }
    return json(res, 200, { ok: true, url: `/uploads/${fname}`, name: S(body.name, 120) || '图片' });
  }],

  ['POST', /^\/api\/tasks\/([A-Za-z0-9_-]+)\/entries$/, (req, res, m, body) => {
    const task = findTask(m[1]);
    if (!task) return json(res, 404, { error: '没有这一话' });
    const ne = normalizeEntry(body);
    if (ne.error) return json(res, 400, { error: ne.error });
    task.entries.push(ne.value);
    task.updatedAt = ne.value.time;
    saveData(data);
    return json(res, 200, { ok: true, task });
  }],

  ['PATCH', /^\/api\/tasks\/([A-Za-z0-9_-]+)\/entries\/([A-Za-z0-9_-]+)$/, (req, res, m, body) => {
    const task = findTask(m[1]);
    if (!task) return json(res, 404, { error: '没有这一话' });
    const entry = task.entries.find((e) => e.id === m[2]);
    if (!entry) return json(res, 404, { error: '没有这一格' });
    if (body.title !== undefined) {
      const title = S(body.title, 120);
      if (!title) return json(res, 400, { error: '改动标题不能为空' });
      entry.title = title;
    }
    if (body.time !== undefined) entry.time = S(body.time, 16) || entry.time;
    if (body.kind !== undefined && KINDS.includes(body.kind)) entry.kind = body.kind;
    if (body.result !== undefined && RESULTS.includes(body.result)) entry.result = body.result;
    if (body.detail !== undefined) entry.detail = S(body.detail, 5000);
    if (body.images !== undefined) entry.images = normalizeImages(body.images);
    if (body.tables !== undefined) entry.tables = normalizeTables(body.tables);
    if (body.files !== undefined) entry.files = normalizeFiles(body.files);
    task.updatedAt = stampLocal(0, `${pad(new Date().getHours())}:${pad(new Date().getMinutes())}`);
    saveData(data);
    return json(res, 200, { ok: true, task });
  }],

  ['POST', /^\/api\/import$/, (req, res, m, body) => {
    if (!Array.isArray(body.tasks)) return json(res, 400, { error: '缺少 tasks 数组' });
    const cleaned = [];
    for (const t of body.tasks.slice(0, 500)) {
      const title = S(t && t.title, 120);
      if (!title) continue;
      const now = stampLocal(0, `${pad(new Date().getHours())}:${pad(new Date().getMinutes())}`);
      cleaned.push({
        id: (typeof t.id === 'string' && /^t_[\w-]+$/.test(t.id)) ? t.id : uid('t'),
        title,
        host: S(t.host, 80),
        status: STATUSES.includes(t.status) ? t.status : '进行中',
        tags: normalizeTags(t.tags),
        note: S(t.note, 1000),
        todos: normalizeTodos(t.todos),
        createdAt: S(t.createdAt, 16) || now,
        updatedAt: S(t.updatedAt, 16) || now,
        entries: (Array.isArray(t.entries) ? t.entries : []).slice(0, 2000).map((e) => ({
          id: (e && typeof e.id === 'string' && /^e_[\w-]+$/.test(e.id)) ? e.id : uid('e'),
          time: S(e && e.time, 16),
          kind: KINDS.includes(e && e.kind) ? e.kind : '其他',
          result: RESULTS.includes(e && e.result) ? e.result : '待验证',
          title: S(e && e.title, 120),
          detail: S(e && e.detail, 5000),
          images: normalizeImages(e && e.images),
          tables: normalizeTables(e && e.tables),
          files: normalizeFiles(e && e.files),
        })).filter((e) => e.title),
      });
    }
    data = { version: 1, seededAt: data.seededAt, tasks: cleaned };
    saveData(data);
    return json(res, 200, { ok: true, tasks: cleaned.length });
  }],

  ['DELETE', /^\/api\/tasks\/([A-Za-z0-9_-]+)\/entries\/([A-Za-z0-9_-]+)$/, (req, res, m) => {
    const task = findTask(m[1]);
    if (!task) return json(res, 404, { error: '没有这一话' });
    const removed = task.entries.find((e) => e.id === m[2]);
    if (!removed) return json(res, 404, { error: '没有这一格' });
    task.entries = task.entries.filter((e) => e.id !== m[2]);
    removeUploads((removed.images || []).map((x) => x.url));
    removeUploads((removed.files || []).map((x) => x.url));
    saveData(data);
    return json(res, 200, { ok: true, task });
  }],
];

function json(res, code, obj) {
  const payload = JSON.stringify(obj);
  res.writeHead(code, { 'Content-Type': 'application/json; charset=utf-8', 'Cache-Control': 'no-store' });
  res.end(payload);
}

function readBody(req, limit = 1024 * 1024) {
  return new Promise((resolve, reject) => {
    let size = 0;
    const chunks = [];
    req.on('data', (c) => {
      size += c.length;
      if (size > limit) { reject(new Error('payload too large')); req.destroy(); return; }
      chunks.push(c);
    });
    req.on('end', () => {
      if (!chunks.length) return resolve({});
      try { resolve(JSON.parse(Buffer.concat(chunks).toString('utf8'))); }
      catch (_) { reject(new Error('bad json')); }
    });
    req.on('error', reject);
  });
}

function readRaw(req, limit) {
  return new Promise((resolve, reject) => {
    let size = 0;
    const chunks = [];
    req.on('data', (c) => {
      size += c.length;
      if (size > limit) { reject(new Error('payload too large')); req.destroy(); return; }
      chunks.push(c);
    });
    req.on('end', () => resolve(Buffer.concat(chunks)));
    req.on('error', reject);
  });
}

function extOf(name) {
  const m = /\.([A-Za-z0-9]{1,10})$/.exec(String(name || ''));
  return m ? m[1].toLowerCase() : '';
}

async function handleDocUpload(req, res, query) {
  let name;
  try { name = decodeURIComponent(query.get('name') || ''); } catch (_) { name = ''; }
  name = S(name, 200) || '附件';
  const ext = extOf(name);
  if (!DOC_EXTS.has(ext)) return json(res, 400, { error: `不支持的文档类型 .${ext || '(无后缀)'}` });
  let buf;
  try { buf = await readRaw(req, DOC_MAX); }
  catch (e) { return json(res, 413, { error: e.message === 'payload too large' ? '文档需在 50MB 以内' : '读取失败' }); }
  if (!buf.length) return json(res, 400, { error: '文档内容为空' });
  const fname = `${uid('u')}.${ext}`;
  try {
    fs.mkdirSync(UPLOAD_DIR, { recursive: true });
    fs.writeFileSync(path.join(UPLOAD_DIR, fname), buf);
  } catch (_) {
    return json(res, 500, { error: '文档保存失败' });
  }
  return json(res, 200, { ok: true, url: `/uploads/${fname}`, name, size: buf.length });
}

/* ---------------- 静态文件 ---------------- */
const MIME = {
  '.html': 'text/html; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.ttf': 'font/ttf',
  '.woff2': 'font/woff2',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.jpeg': 'image/jpeg',
  '.gif': 'image/gif',
  '.webp': 'image/webp',
  '.ico': 'image/x-icon',
  '.txt': 'text/plain; charset=utf-8',
};

function serveStatic(req, res, pathname) {
  let base = PUBLIC_DIR;
  let rel = pathname === '/' ? 'index.html' : pathname.slice(1);
  let isUpload = false;
  if (pathname.startsWith('/uploads/')) { base = UPLOAD_DIR; rel = path.basename(pathname); isUpload = true; }
  const filePath = path.normalize(path.join(base, rel));
  if (!filePath.startsWith(base)) return notFound(res);
  fs.readFile(filePath, (err, buf) => {
    if (err) return notFound(res);
    const ext = path.extname(filePath).toLowerCase();
    const headers = {
      'Content-Type': MIME[ext] || 'application/octet-stream',
      'Cache-Control': 'no-store',
    };
    // 上传的文件：图片允许内联显示，其余一律作为下载附件（防止 .html 等在本页内执行）
    if (isUpload) {
      headers['X-Content-Type-Options'] = 'nosniff';
      if (!['.png', '.jpg', '.jpeg', '.gif', '.webp'].includes(ext)) {
        headers['Content-Disposition'] = `attachment; filename="${path.basename(filePath)}"`;
      }
    }
    res.writeHead(200, headers);
    res.end(buf);
  });
}

function notFound(res) {
  res.writeHead(404, { 'Content-Type': 'text/html; charset=utf-8' });
  res.end(`<!DOCTYPE html><html lang="zh-CN"><head><meta charset="UTF-8"><title>404</title></head>
<body style="background:#f6f2e8;color:#161513;font-family:KaiTi,serif;display:flex;min-height:100vh;align-items:center;justify-content:center;">
<div style="border:2.5px solid #161513;border-radius:255px 15px 225px 15px/15px 225px 15px 255px;padding:40px 60px;text-align:center;transform:rotate(-1deg);box-shadow:6px 6px 0 #161513;">
<div style="font-size:56px;font-weight:bold;">404</div><div style="font-size:18px;margin-top:10px;">这一页被编辑撕掉了……</div></div></body></html>`);
}

/* ---------------- 入口 ---------------- */
async function handler(req, res) {
  const u = new URL(req.url, 'http://x');
  const pathname = decodeURIComponent(u.pathname);
  try {
    /* 口令锁：设置了 ACCESS_CODE 时，公网来源要求登录；内网直连免口令 */
    if (ACCESS_CODE && !isLanRequest(req)) {
      const ip = req.socket.remoteAddress || '?';
      if (pathname === '/api/login' && req.method.toUpperCase() === 'POST') {
        let body = {};
        try { body = await readBody(req, 4096); } catch (_) { body = {}; }
        if (failRecord(ip).n >= 20) return json(res, 429, { error: '错太多次了，休息 5 分钟再来' });
        if (String(body.code || '').trim() === ACCESS_CODE) {
          loginFails.delete(ip);
          res.writeHead(200, {
            'Content-Type': 'application/json; charset=utf-8',
            'Set-Cookie': `auth=${AUTH_TOKEN}; HttpOnly; SameSite=Lax; Path=/; Max-Age=${30 * 86400}`,
          });
          return res.end('{"ok":true}');
        }
        failRecord(ip).n += 1;
        return json(res, 401, { error: '口令不对' });
      }
      if (pathname === '/api/logout') {
        res.writeHead(200, {
          'Content-Type': 'application/json; charset=utf-8',
          'Set-Cookie': 'auth=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0',
        });
        return res.end('{"ok":true}');
      }
      if (!isAuthed(req)) {
        if (pathname === '/login') return serveLogin(res);
        if (pathname.startsWith('/api/')) return json(res, 401, { error: '未登录或会话已失效' });
        res.writeHead(302, { Location: '/login' });
        return res.end();
      }
      if (pathname === '/login') { res.writeHead(302, { Location: '/' }); return res.end(); }
    }
    /* 内网免口令用户访问 /login：不需要登录页，直接回主页 */
    if (ACCESS_CODE && pathname === '/login') { res.writeHead(302, { Location: '/' }); return res.end(); }
    if (pathname === '/api/upload/doc' && req.method.toUpperCase() === 'POST') {
      return await handleDocUpload(req, res, u.searchParams);
    }
    if (pathname.startsWith('/api/')) {
      const method = req.method.toUpperCase();
      for (const [m, re, fn] of routes) {
        if (m !== method) continue;
        const match = pathname.match(re);
        if (!match) continue;
        let body = {};
        if (method === 'POST' || method === 'PATCH' || method === 'PUT') {
          const limit = pathname.startsWith('/api/upload') ? 90 * 1024 * 1024 : 1024 * 1024;
          try { body = await readBody(req, limit); }
          catch (e) { return json(res, e.message === 'bad json' ? 400 : 413, { error: e.message === 'bad json' ? '请求体不是合法 JSON' : '请求体过大' }); }
        }
        return fn(req, res, match, body);
      }
      return json(res, 404, { error: '接口不存在' });
    }
    if (req.method !== 'GET' && req.method !== 'HEAD') return notFound(res);
    return serveStatic(req, res, pathname);
  } catch (e) {
    console.error('[x] 处理请求出错：', e);
    return json(res, 500, { error: '服务器内部错误' });
  }
}

function openBrowser(url) {
  if (process.env.NO_OPEN) return;
  try {
    if (process.platform === 'win32') spawn('cmd', ['/c', 'start', '', url], { detached: true, stdio: 'ignore' }).unref();
    else if (process.platform === 'darwin') spawn('open', [url], { detached: true, stdio: 'ignore' }).unref();
    else spawn('xdg-open', [url], { detached: true, stdio: 'ignore' }).unref();
  } catch (_) { /* 打不开浏览器也不影响服务 */ }
}

function listen(port, triesLeft) {
  const server = http.createServer(handler);
  server.on('error', (err) => {
    if (err && err.code === 'EADDRINUSE') {
      // 端口被占：仅当显式允许（PORT_AUTOINCREMENT=1，本地 start.bat 默认开）时顺延端口；
      // 服务器部署必须钉死端口（隧道指向它），否则报错退出。
      if (process.env.PORT_AUTOINCREMENT === '1' && triesLeft > 0) {
        console.log(`[i] 端口 ${port} 被占用，改用 ${port + 1} …`);
        listen(port + 1, triesLeft - 1);
      } else {
        console.error(`[x] 端口 ${port} 已被占用。若是漫画志旧进程请先停掉（kill），或换 PORT 启动；隧道/书签都指向固定端口，不建议顺延。`);
        process.exit(1);
      }
    } else {
      console.error('[x] 启动失败：', err.message);
      process.exit(1);
    }
  });
  server.listen(port, BIND_HOST, () => {
    const showHost = BIND_HOST === '0.0.0.0' || BIND_HOST === '::' ? `http://<本机IP>:${port}` : `http://${BIND_HOST}:${port}`;
    const line = '─'.repeat(46);
    console.log('');
    console.log(`  ┌${line}┐`);
    console.log('  │  服务器改动 · 漫画志   —— 开演！           │');
    console.log(`  │  ${showHost.padEnd(41)}│`);
    console.log(`  │  监听 ${String(BIND_HOST).padEnd(14)}  口令锁 ${String(ACCESS_CODE ? '已启用' : '未启用').padEnd(4)}      │`);
    console.log('  │  数据文件：./data.json                     │');
    console.log('  │  按 Ctrl+C 闭幕                            │');
    console.log(`  └${line}┘`);
    if (BIND_HOST === '127.0.0.1') openBrowser(`http://127.0.0.1:${port}`);
  });
}

listen(BASE_PORT, 20);
