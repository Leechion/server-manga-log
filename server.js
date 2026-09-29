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
const SNAPSHOT_DIR = path.join(ROOT, '.backups');
const USERS_DIR = path.join(ROOT, 'users');
const SNAPSHOT_LIMIT = 40;

/* 可选 config.json：
 * { "accessCode": "口令", "host": "127.0.0.1", "port": 4780,
 *   "users": [{ "name": "alice", "code": "alice 的口令" }],
 *   "trustProxy": false, "secureCookie": false }
 * （环境变量 ACCESS_CODE/HOST/PORT/TRUST_PROXY/SECURE_COOKIE 优先；users 非空即进入多用户模式） */
function loadConfig() {
  let raw = '';
  try { raw = fs.readFileSync(path.join(ROOT, 'config.json'), 'utf8'); }
  catch (e) {
    if (e.code !== 'ENOENT') console.warn('[!] config.json 读取失败，按默认配置启动：', e.message);
    return {};
  }
  if (!raw.trim()) return {};
  try { return JSON.parse(raw); }
  catch (_) {
    console.error('[x] config.json 不是合法 JSON（注释、尾逗号都会导致解析失败）。');
    console.error('    为防 accessCode 被静默禁用导致公网裸奔，服务拒绝启动。请修正语法后重试。');
    process.exit(1);
  }
}
const CFG = loadConfig();
const ACCESS_CODE = String(process.env.ACCESS_CODE ?? CFG.accessCode ?? '').trim();
const BIND_HOST = process.env.HOST || CFG.host || '127.0.0.1';
const BASE_PORT = Number(process.env.PORT) || Number(CFG.port) || 4780;
const flagOf = (envVal, cfgVal) => (envVal != null ? /^(1|true|yes)$/i.test(String(envVal)) : cfgVal === true);
/* 反代头信任开关：默认关。只有立即 TCP 对端本身可信（同机隧道/反代）时才采信转发头；
 * 反代部署在另一台机器时显式开启，并同时用防火墙收口。 */
const TRUST_PROXY = flagOf(process.env.TRUST_PROXY, CFG.trustProxy);
/* 会话 Cookie 加 Secure：外层是 HTTPS 隧道/反代时开启，防口令会话被降级窃听 */
const SECURE_COOKIE = flagOf(process.env.SECURE_COOKIE, CFG.secureCookie);

/* ---------------- 多用户模式 ----------------
 * config.users 非空即启用：每个账号在 users/<name>/ 下拥有独立的
 * data.json / uploads/ / .backups/，登录颁发签名会话 Cookie，互相完全隔离。
 * 多用户模式下 ACCESS_CODE 与内网免口令均不生效（身份必须明确）。 */
function normalizeUsers(raw) {
  if (raw == null) return [];
  if (!Array.isArray(raw)) { console.error('[x] config.json 的 users 必须是数组。'); process.exit(1); }
  const out = [];
  const seen = new Set();
  for (const u of raw) {
    const name = String((u && u.name) || '').trim().toLowerCase();
    const code = String((u && u.code) || '').trim();
    if (!/^[a-z0-9_-]{1,32}$/.test(name) || !code) {
      console.error('[x] users 配置无效：每项需要 name（小写字母/数字/-/_，1-32 位）和 code（非空口令）。出错项：', JSON.stringify(u));
      process.exit(1);
    }
    if (seen.has(name)) { console.error(`[x] users 用户名重复：${name}`); process.exit(1); }
    seen.add(name);
    out.push({ name, code });
  }
  return out;
}
const USERS = normalizeUsers(CFG.users);
const MULTI = USERS.length > 0;
if (MULTI && ACCESS_CODE) console.warn('[!] 多用户模式已启用，accessCode 将被忽略（登录按用户口令校验）。');

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
/* 随机段用 crypto 而非 Math.random：附件名会被公网枚举，时间戳便于排序 */
function uid(prefix) {
  return `${prefix}_${Date.now().toString(36)}${crypto.randomBytes(4).toString('hex')}`;
}

function seedData() {
  const t = (offset, hhmm) => stampLocal(offset, hhmm);
  const tasks = [
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
  ];
  return {
    version: 2,
    seededAt: new Date().toISOString(),
    activeMangaId: 'm_legacy',
    mangas: [{ id: 'm_legacy', title: '服务器改动', createdAt: new Date().toISOString(), tasks }],
  };
}

function normalizeStoredData(parsed) {
  if (parsed && Array.isArray(parsed.mangas) && parsed.mangas.length) {
    const mangas = parsed.mangas.map((m, i) => ({
      id: (m && typeof m.id === 'string' && /^m_[\w-]+$/.test(m.id)) ? m.id : `m_${i + 1}`,
      title: (m && typeof m.title === 'string' ? m.title.trim().slice(0, 80) : '') || `漫画 ${i + 1}`,
      createdAt: (m && typeof m.createdAt === 'string' ? m.createdAt.slice(0, 40) : '') || new Date().toISOString(),
      tasks: Array.isArray(m && m.tasks) ? m.tasks : [],
    }));
    const activeMangaId = mangas.some((m) => m.id === parsed.activeMangaId) ? parsed.activeMangaId : mangas[0].id;
    return { version: 2, seededAt: parsed.seededAt || new Date().toISOString(), activeMangaId, mangas };
  }
  if (parsed && Array.isArray(parsed.tasks)) {
    return {
      version: 2,
      seededAt: parsed.seededAt || new Date().toISOString(),
      activeMangaId: 'm_legacy',
      mangas: [{ id: 'm_legacy', title: '服务器改动', createdAt: parsed.seededAt || new Date().toISOString(), tasks: parsed.tasks }],
    };
  }
  throw new Error('bad shape');
}
function allTasksOf(store) {
  if (store && Array.isArray(store.mangas)) return store.mangas.flatMap((m) => Array.isArray(m.tasks) ? m.tasks : []);
  return store && Array.isArray(store.tasks) ? store.tasks : [];
}
/* 一个 store = 一份彼此独立的数据域：
 * 单用户模式用 DEFAULT_STORE（仓根 data.json / uploads / .backups），
 * 多用户模式每个账号一份 users/<name>/ 下的 store。 */
function newUserStoreData(store) {
  const now = new Date().toISOString();
  return {
    version: 2,
    seededAt: now,
    activeMangaId: 'm_1',
    mangas: [{ id: 'm_1', title: `${store.name} 的记录`, createdAt: now, tasks: [] }],
  };
}
function loadStoreData(store, seed) {
  try {
    const parsed = JSON.parse(fs.readFileSync(store.dataFile, 'utf8'));
    return normalizeStoredData(parsed);
  } catch (e) {
    if (fs.existsSync(store.dataFile)) {
      try { fs.renameSync(store.dataFile, `${store.dataFile}.bak-${Date.now()}`); } catch (_) { /* 忽略备份失败 */ }
      console.warn(`[!] ${path.basename(store.dataFile)}（${store.name || '单用户'}）无法解析，已备份为 .bak 文件并重建。`);
    }
    const fresh = seed();
    try { saveData(store, fresh); } catch (_) { /* 首次写盘失败不致命 */ }
    return fresh;
  }
}
/* 写盘采用「临时文件 + rename」保证原子性。
 *
 * 同时记录写盘时的 mtime：若发现 data.json 被本进程之外的写入改动过，
 * 说明有别的程序（如 import_history.py）也在写同一个文件。此时**不静默合并**
 * —— 合并无法区分「外部新增」与「本进程删除」，会把手动删掉的记录又复活。
 * 改为：先备份外部版本，再以本进程内存为准落盘，并明确告警，把处置权交还用户。 */
function saveData(store, d) {
  try {
    const st = fs.statSync(store.dataFile);
    const today = dayStr();
    if (store.lastDailySnapDay !== today) {
      const daily = hasSnapshotForDay(store, today) || createSnapshot(store, 'daily');
      if (daily) store.lastDailySnapDay = today;
    }
    if (store.lastMtimeMs && st.mtimeMs !== store.lastMtimeMs) {
      const stamp = new Date().toISOString().replace(/[:.]/g, '-');
      const conflictPath = `${store.dataFile}.conflict-${stamp}`;
      try {
        fs.copyFileSync(store.dataFile, conflictPath);
        createSnapshot(store, 'external-edit');
        console.warn(`[!] 检测到 ${path.basename(store.dataFile)} 被其他程序改动过。`);
        console.warn(`    已把外部版本另存为 ${path.basename(conflictPath)}，本次以当前内存状态落盘。`);
        console.warn('    若外部版本里有需要保留的记录，请用界面上的「⇒ 导入备份」导入该文件。');
      } catch (e) {
        console.warn('[!] 检测到外部改动，但备份失败：', e.message);
      }
    }
  } catch (_) { /* 文件尚不存在（首次写入） */ }
  const tmp = `${store.dataFile}.tmp`;
  /* 多用户首次写入时 users/<name>/ 还不存在，先建目录再落盘 */
  fs.mkdirSync(path.dirname(store.dataFile), { recursive: true });
  fs.writeFileSync(tmp, JSON.stringify(d, null, 2), 'utf8');
  fs.renameSync(tmp, store.dataFile);
  try { store.lastMtimeMs = fs.statSync(store.dataFile).mtimeMs; } catch (_) { store.lastMtimeMs = 0; }
}

function snapshotFiles(store) {
  try {
    return fs.readdirSync(store.snapDir)
      .filter((name) => /^snapshot-[\dTZ_-]+-(?:daily|before-import|before-restore|before-delete|external-edit)-[\w-]+\.json$/.test(name))
      .map((name) => ({ name, full: path.join(store.snapDir, name), mtime: fs.statSync(path.join(store.snapDir, name)).mtimeMs }))
      .sort((a, b) => b.mtime - a.mtime);
  } catch (_) { return []; }
}
function hasSnapshotForDay(store, day) {
  const prefix = `snapshot-${day}`;
  return snapshotFiles(store).some((item) => item.name.startsWith(prefix));
}
function createSnapshot(store, reason) {
  if (!fs.existsSync(store.dataFile)) return null;
  try {
    fs.mkdirSync(store.snapDir, { recursive: true });
    const now = new Date();
    const stamp = `${dayStr()}T${pad(now.getHours())}-${pad(now.getMinutes())}-${pad(now.getSeconds())}`;
    const id = `snapshot-${stamp}-${reason}-${crypto.randomBytes(3).toString('hex')}`;
    const full = path.join(store.snapDir, `${id}.json`);
    fs.copyFileSync(store.dataFile, full);
    const assetDir = path.join(store.snapDir, id);
    let saved;
    try { saved = JSON.parse(fs.readFileSync(store.dataFile, 'utf8')); } catch (_) { saved = null; }
    if (saved && (Array.isArray(saved.tasks) || Array.isArray(saved.mangas))) {
      fs.mkdirSync(assetDir, { recursive: true });
      const urls = allTasksOf(saved).flatMap((task) => (task.entries || []).flatMap((entry) => [
        ...(entry.images || []).map((item) => item.url), ...(entry.files || []).map((item) => item.url),
      ]));
      for (const url of new Set(urls)) {
        if (!/^\/uploads\/[\w.-]+$/.test(url)) continue;
        const source = path.join(store.uploadDir, path.basename(url));
        const destination = path.join(assetDir, path.basename(url));
        if (!fs.existsSync(source)) continue;
        try { fs.linkSync(source, destination); }
        catch (_) { try { fs.copyFileSync(source, destination); } catch (_) { /* 缺失或不可读附件不阻止记录快照 */ } }
      }
    }
    for (const old of snapshotFiles(store).slice(SNAPSHOT_LIMIT)) {
      try {
        fs.unlinkSync(old.full);
        const oldId = old.name.slice(0, -5);
        const oldAssets = path.join(store.snapDir, oldId);
        if (oldAssets.startsWith(`${store.snapDir}${path.sep}`)) fs.rmSync(oldAssets, { recursive: true, force: true });
      } catch (_) { /* 忽略清理失败 */ }
    }
    return id;
  } catch (e) {
    console.warn('[!] 自动快照失败：', e.message);
    return null;
  }
}
function listSnapshots(store) {
  return snapshotFiles(store).map(({ name, full, mtime }) => {
    let tasks = 0;
    try { tasks = allTasksOf(JSON.parse(fs.readFileSync(full, 'utf8'))).length; } catch (_) { /* 显示损坏快照，恢复接口会给出错误 */ }
    const id = name.slice(0, -5);
    const reason = id.match(/-(daily|before-import|before-restore|before-delete|external-edit)-[\w-]+$/)?.[1] || 'daily';
    return { id, createdAt: new Date(mtime).toISOString(), reason, tasks };
  });
}

const DEFAULT_STORE = {
  name: '', root: ROOT,
  dataFile: DATA_FILE, uploadDir: UPLOAD_DIR, snapDir: SNAPSHOT_DIR,
  lastMtimeMs: 0, lastDailySnapDay: '', data: null,
};
const stores = new Map([['', DEFAULT_STORE]]);
/* 多用户：按需装载某个账号的数据域（首次访问时读盘，缺文件就建一份空的） */
function ensureStore(name) {
  const cached = stores.get(name);
  if (cached) return cached;
  const root = path.join(USERS_DIR, name);
  const store = {
    name, root,
    dataFile: path.join(root, 'data.json'),
    uploadDir: path.join(root, 'uploads'),
    snapDir: path.join(root, '.backups'),
    lastMtimeMs: 0, lastDailySnapDay: '', data: null,
  };
  store.data = loadStoreData(store, () => newUserStoreData(store));
  stores.set(name, store);
  return store;
}
DEFAULT_STORE.data = loadStoreData(DEFAULT_STORE, seedData);

/* ---------------- 单实例锁 ----------------
 * 数据全量驻留内存、saveData 整份覆盖写盘，两个实例并存必然互相覆盖丢数据。
 * 用 PID 文件阻止第二个实例启动（.gitignore 里已忽略 server.pid）。 */
const LOCK_FILE = path.join(ROOT, 'server.pid');
function acquireLock() {
  try {
    if (fs.existsSync(LOCK_FILE)) {
      const oldPid = Number(String(fs.readFileSync(LOCK_FILE, 'utf8')).trim());
      if (Number.isFinite(oldPid) && oldPid > 0 && oldPid !== process.pid) {
        try {
          process.kill(oldPid, 0); // 信号 0：只探测进程是否存在
          console.error(`[x] 已有实例在运行（PID ${oldPid}）。`);
          console.error('    同时开两个实例会互相覆盖 data.json 导致记录丢失，已拒绝启动。');
          console.error('    若确认没有实例在跑，删除 server.pid 后重试。');
          process.exit(1);
        } catch (_) { /* 陈旧锁（进程已不存在），继续覆盖 */ }
      }
    }
    fs.writeFileSync(LOCK_FILE, String(process.pid), 'utf8');
    const release = () => { try { fs.unlinkSync(LOCK_FILE); } catch (_) {} };
    process.on('exit', release);
    process.on('SIGINT', () => { release(); process.exit(0); });
    process.on('SIGTERM', () => { release(); process.exit(0); });
  } catch (e) {
    console.warn('[!] 无法创建实例锁（不影响启动）：', e.message);
  }
}
acquireLock();

/* ---------------- 工具 ---------------- */
const S = (v, max) => (typeof v === 'string' ? v.trim().slice(0, max) : '');
function mangaForRequest(store, req) {
  const id = new URL(req.url, 'http://x').searchParams.get('mangaId') || store.data.activeMangaId;
  return store.data.mangas.find((m) => m.id === id) || store.data.mangas.find((m) => m.id === store.data.activeMangaId) || store.data.mangas[0] || null;
}
const findTask = (store, id, req) => mangaForRequest(store, req)?.tasks.find((t) => t.id === id);
function clientData(store, mangaId) {
  const manga = store.data.mangas.find((m) => m.id === mangaId) || store.data.mangas[0];
  return {
    version: 2,
    seededAt: store.data.seededAt,
    activeMangaId: manga?.id || '',
    activeMangaTitle: manga?.title || '',
    mangas: store.data.mangas.map((m) => ({ id: m.id, title: m.title, createdAt: m.createdAt, taskCount: m.tasks.length })),
    tasks: manga ? manga.tasks : [],
  };
}

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
function normalizeRelation(v) {
  if (!v || typeof v !== 'object') return null;
  const taskId = typeof v.taskId === 'string' && /^t_[\w-]+$/.test(v.taskId) ? v.taskId : '';
  const entryId = typeof v.entryId === 'string' && /^e_[\w-]+$/.test(v.entryId) ? v.entryId : '';
  const type = ['相关', '修复', '回滚', '验证'].includes(v.type) ? v.type : '相关';
  return taskId && entryId ? { taskId, entryId, type } : null;
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
/* 导入与快照恢复共用的 tasks 清洗：丢弃空标题，字段限长限类型，杜绝脏数据入库 */
const cleanTasks = (input) => input.slice(0, 500).map((t) => {
  const title = S(t && t.title, 120);
  if (!title) return null;
  const now = stampLocal(0, `${pad(new Date().getHours())}:${pad(new Date().getMinutes())}`);
  return {
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
      reviewAt: S(e && e.reviewAt, 16),
      relation: normalizeRelation(e && e.relation),
      images: normalizeImages(e && e.images),
      tables: normalizeTables(e && e.tables),
      files: normalizeFiles(e && e.files),
    })).filter((e) => e.title),
  };
}).filter(Boolean);

function removeUploads(store, urls) {
  for (const u of urls || []) {
    if (!/^\/uploads\/[\w.-]+$/.test(u)) continue;
    try { fs.unlinkSync(path.join(store.uploadDir, path.basename(u))); } catch (_) { /* 文件可能已不在 */ }
  }
}
/* 旧 URL 列表中已被新列表抛弃的那些（差集），用于安全删除附件 */
function orphansOf(oldUrls, newItems) {
  const kept = new Set((newItems || []).map((x) => x.url));
  return (oldUrls || []).filter((u) => !kept.has(u));
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
      reviewAt: S(body.reviewAt, 16),
      relation: normalizeRelation(body.relation),
      images: normalizeImages(body.images),
      tables: normalizeTables(body.tables),
      files: normalizeFiles(body.files),
    },
  };
}

/* 统一安全响应头：防点击劫持（iframe 套壳骗点「撕掉」）、防 MIME 嗅探、限制外链引用 */
const SECURITY_HEADERS = {
  'X-Content-Type-Options': 'nosniff',
  'X-Frame-Options': 'DENY',
  'Referrer-Policy': 'no-referrer',
};

/* 真实客户端 IP：
 * - 立即 TCP 对端可信（TRUST_PROXY 显式开启，或同机 cloudflared / 本机反代连到回环/内网）时，
 *   优先取 Cloudflare 注入的 CF-Connecting-IP，其次 X-Forwarded-For；
 * - 对端不可信（公网直连）时这些头是攻击者随口编的，一律忽略、按 socket 真实地址判定，
 *   否则伪造一个 `X-Forwarded-For: 127.0.0.1` 就能伪装内网把口令锁整个架空。
 * 登录失败计数与内网判定都必须走这里，否则公网访客会共用同一个计数桶。 */
function peerTrusted(req) {
  return TRUST_PROXY || isPrivateIp(req.socket.remoteAddress || '');
}
function clientIp(req) {
  if (peerTrusted(req)) {
    const cf = req.headers['cf-connecting-ip'];
    if (cf) return String(cf).trim();
    const xff = req.headers['x-forwarded-for'];
    if (xff) return String(xff).split(',')[0].trim();
  }
  return req.socket.remoteAddress || '?';
}

/* 常量时间字符串比较，避免口令比对的时序侧信道 */
function safeEqual(a, b) {
  const ba = Buffer.from(String(a), 'utf8');
  const bb = Buffer.from(String(b), 'utf8');
  if (ba.length !== bb.length) return false;
  return crypto.timingSafeEqual(ba, bb);
}

/* ---------------- 口令锁（设置了 ACCESS_CODE 才启用） ---------------- */
/* 每次启动生成随机盐：token 不再只是口令的纯函数，
 * 重启即让所有旧会话失效（改口令同样全端下线）。 */
const AUTH_SALT = crypto.randomBytes(16).toString('hex');
const AUTH_TOKEN = ACCESS_CODE
  ? crypto.createHash('sha256').update(`manga-log::${AUTH_SALT}::${ACCESS_CODE}`).digest('hex')
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

/* ---------------- 多用户会话 ----------------
 * token = v1.<用户名>.<到期时间>.<HMAC 签名>：用进程随机密钥签名，
 * 重启即全部会话失效（与单用户模式的随机盐同思路），Cookie 里不出现口令明文。 */
const AUTH_SECRET = crypto.randomBytes(32).toString('hex');
function signToken(name) {
  const exp = Math.floor(Date.now() / 1000) + 30 * 86400;
  const body = `v1.${name}.${exp}`;
  return `${body}.${crypto.createHmac('sha256', AUTH_SECRET).update(body).digest('hex')}`;
}
function verifyToken(tok) {
  const parts = String(tok || '').split('.');
  if (parts.length !== 4 || parts[0] !== 'v1') return null;
  const [, name, exp, sig] = parts;
  if (!/^[a-z0-9_-]{1,32}$/.test(name)) return null;
  const want = crypto.createHmac('sha256', AUTH_SECRET).update(`v1.${name}.${exp}`).digest('hex');
  if (!safeEqual(sig, want)) return null;
  if (!(Number(exp) > Math.floor(Date.now() / 1000))) return null;
  return USERS.some((u) => u.name === name) ? name : null;
}
function sessionUser(req) {
  return MULTI ? verifyToken(getCookie(req, 'auth')) : null;
}
function authCookie(value, maxAge) {
  return `auth=${value}; HttpOnly; SameSite=Lax; Path=/; Max-Age=${maxAge}${SECURE_COOKIE ? '; Secure' : ''}`;
}

/* 内网免口令：直连的内网/本机来源直接放行；经隧道来的公网请求必须口令。
 * 判定统一走 clientIp()：同机隧道场景 socket 恒为回环，须靠可信对端注入的转发头还原真实访客 IP；
 * 公网直连时 socket 地址本身就是真实来源，转发头不参与判定（见 clientIp 注释）。 */
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
  return isPrivateIp(clientIp(req));
}
const loginFails = new Map();
function failRecord(ip) {
  const now = Date.now();
  let rec = loginFails.get(ip);
  if (!rec || now > rec.reset) { rec = { n: 0, reset: now + 5 * 60 * 1000 }; loginFails.set(ip, rec); }
  if (loginFails.size > 512) {
    for (const [key, val] of loginFails) if (now > val.reset) loginFails.delete(key);
  }
  return rec;
}
function serveLogin(res, multi) {
  res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8', 'Cache-Control': 'no-store' });
  const userField = multi
    ? `<input id="user" type="text" autocomplete="username" autocapitalize="off" placeholder="用户名…" style="flex:1;font-size:16px;padding:9px 12px;border:2px solid #161513;background:#fbf8f0;border-radius:14px 5px 16px 6px/6px 16px 5px 14px;outline:none;font-family:inherit;margin-bottom:10px;">`
    : '';
  const subtitle = multi ? 'SERVER MANGA LOG · 登录后只看到自己的记录' : 'SERVER MANGA LOG · 口令确认后开演';
  res.end(`<!DOCTYPE html><html lang="zh-CN"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>验明正身 · 漫画志</title></head>
<body style="margin:0;min-height:100vh;display:flex;align-items:center;justify-content:center;background:#efe9da;background-image:radial-gradient(circle,rgba(22,21,19,.08) 1px,transparent 1.4px);background-size:7px 7px;font-family:KaiTi,'楷体',STKaiti,serif;color:#161513;">
<div style="background:#f6f2e8;border:2.5px solid #161513;border-radius:255px 15px 225px 15px/15px 225px 15px 255px;box-shadow:8px 8px 0 rgba(22,21,19,.85);padding:38px 44px;max-width:420px;transform:rotate(-.6deg);text-align:center;">
<div style="font-family:KaiTi,serif;font-size:34px;letter-spacing:6px;">验明正身</div>
<div style="font-size:13px;color:rgba(22,21,19,.55);letter-spacing:3px;margin:4px 0 20px;">${subtitle}</div>
<form id="f" style="display:flex;flex-wrap:wrap;gap:10px;">
${userField}<input id="code" type="password" autofocus placeholder="输入口令…" style="flex:1;font-size:16px;padding:9px 12px;border:2px solid #161513;background:#fbf8f0;border-radius:14px 5px 16px 6px/6px 16px 5px 14px;outline:none;font-family:inherit;">
<button style="border:2px solid #161513;background:#161513;color:#f6f2e8;font-weight:700;font-size:15px;letter-spacing:2px;padding:8px 16px;border-radius:14px 5px 16px 6px/6px 16px 5px 14px;box-shadow:3px 3px 0 rgba(22,21,19,.4);cursor:pointer;font-family:inherit;">开演</button>
</form>
<div id="msg" style="margin-top:14px;font-size:13.5px;min-height:20px;color:#161513;"></div>
</div>
<script>
document.getElementById('f').addEventListener('submit', async (ev) => {
  ev.preventDefault();
  const msg = document.getElementById('msg');
  const userEl = document.getElementById('user');
  const payload = { code: document.getElementById('code').value };
  if (userEl) payload.name = userEl.value;
  try {
    const r = await fetch('/api/login', { method: 'POST', headers: { 'Content-Type': 'application/json', 'X-Requested-With': 'manga-log' }, body: JSON.stringify(payload) });
    const b = await r.json().catch(() => ({}));
    if (r.ok) { location.href = '/'; return; }
    msg.textContent = b.error || '口令不对';
  } catch (e) { msg.textContent = '网络异常，稍后再试'; }
});
</script>
</body></html>`);
}

/* ---------------- API ----------------
 * 每个处理函数第 5 个参数 C 是当前请求所属的数据域（store）：
 * 单用户模式为 DEFAULT_STORE，多用户模式为该账号的 users/<name>/。 */
const routes = [
  ['GET', /^\/api\/whoami$/, (req, res, m, body, C) => json(res, 200, { multi: MULTI, user: MULTI ? C.name : null })],
  ['POST', /^\/api\/logout$/, (req, res) => {
    res.writeHead(200, { 'Content-Type': 'application/json; charset=utf-8', 'Set-Cookie': authCookie('', 0) });
    return res.end('{"ok":true}');
  }],
  ['GET', /^\/api\/data$/, (req, res, m, body, C) => {
    const query = new URL(req.url, 'http://x').searchParams;
    return json(res, 200, query.get('all') === '1' ? C.data : clientData(C, mangaForRequest(C, req)?.id));
  }],
  ['POST', /^\/api\/mangas$/, (req, res, m, body, C) => {
    const title = S(body.title, 80);
    if (!title) return json(res, 400, { error: '漫画名不能为空' });
    const manga = { id: uid('m'), title, createdAt: new Date().toISOString(), tasks: [] };
    C.data.mangas.unshift(manga);
    C.data.activeMangaId = manga.id;
    saveData(C, C.data);
    return json(res, 200, { ok: true, manga, data: clientData(C, manga.id) });
  }],
  ['PATCH', /^\/api\/mangas\/([A-Za-z0-9_-]+)$/, (req, res, m, body, C) => {
    const manga = C.data.mangas.find((item) => item.id === m[1]);
    if (!manga) return json(res, 404, { error: '找不到这部漫画' });
    const title = S(body.title, 80);
    if (!title) return json(res, 400, { error: '漫画名不能为空' });
    manga.title = title;
    saveData(C, C.data);
    return json(res, 200, { ok: true, manga, data: clientData(C, manga.id) });
  }],
  ['GET', /^\/api\/snapshots$/, (req, res, m, body, C) => json(res, 200, { snapshots: listSnapshots(C) })],
  ['POST', /^\/api\/snapshots\/restore$/, (req, res, m, body, C) => {
    const id = typeof body.id === 'string' && /^snapshot-[\dTZ_-]+-(?:daily|before-import|before-restore|before-delete|external-edit)-[\w-]+$/.test(body.id) ? body.id : '';
    if (!id) return json(res, 400, { error: '快照编号无效' });
    const full = path.join(C.snapDir, `${id}.json`);
    if (!full.startsWith(`${C.snapDir}${path.sep}`) || !fs.existsSync(full)) return json(res, 404, { error: '找不到这份快照' });
    let restored;
    try { restored = JSON.parse(fs.readFileSync(full, 'utf8')); } catch (_) { return json(res, 400, { error: '快照文件损坏，无法恢复' }); }
    try { restored = normalizeStoredData(restored); }
    catch (_) { return json(res, 400, { error: '快照内容不完整，无法恢复' }); }
    /* 快照里的 tasks 也过一遍与导入一致的清洗，维持「入库必经清洗」的纵深防御 */
    for (const manga of restored.mangas) manga.tasks = cleanTasks(manga.tasks);
    const assetDir = path.join(C.snapDir, id);
    const requiredAssets = allTasksOf(restored).flatMap((task) => (task.entries || []).flatMap((entry) => [
      ...(entry.images || []).map((item) => item.url), ...(entry.files || []).map((item) => item.url),
    ]));
    let missingAssets = 0;
    for (const url of new Set(requiredAssets)) {
      if (!/^\/uploads\/[\w.-]+$/.test(url)) continue;
      const name = path.basename(url);
      const source = path.join(assetDir, name);
      const destination = path.join(C.uploadDir, name);
      if (!fs.existsSync(source)) { if (!fs.existsSync(destination)) missingAssets++; continue; }
      if (fs.existsSync(destination)) {
        try {
          const [srcStat, dstStat] = [fs.statSync(source), fs.statSync(destination)];
          if (srcStat.dev === dstStat.dev && srcStat.ino === dstStat.ino) continue;
        } catch (_) { /* 继续尝试复制快照附件 */ }
      }
      try { fs.mkdirSync(C.uploadDir, { recursive: true }); fs.copyFileSync(source, destination); }
      catch (_) { missingAssets++; }
    }
    createSnapshot(C, 'before-restore');
    C.data = restored;
    saveData(C, C.data);
    return json(res, 200, { ok: true, mangas: C.data.mangas.length, tasks: allTasksOf(C.data).length, missingAssets });
  }],

  ['POST', /^\/api\/tasks$/, (req, res, m, body, C) => {
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
    mangaForRequest(C, req).tasks.unshift(task);
    saveData(C, C.data);
    return json(res, 200, { ok: true, task });
  }],

  ['PATCH', /^\/api\/tasks\/([A-Za-z0-9_-]+)$/, (req, res, m, body, C) => {
    const task = findTask(C, m[1], req);
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
    saveData(C, C.data);
    return json(res, 200, { ok: true, task });
  }],

  ['DELETE', /^\/api\/tasks\/([A-Za-z0-9_-]+)$/, (req, res, m, body, C) => {
    const task = findTask(C, m[1], req);
    if (!task) return json(res, 404, { error: '没有这一话' });
    createSnapshot(C, 'before-delete');
    const manga = mangaForRequest(C, req);
    manga.tasks = manga.tasks.filter((t) => t.id !== m[1]);
    (task.entries || []).forEach((e) => {
      removeUploads(C, (e.images || []).map((x) => x.url));
      removeUploads(C, (e.files || []).map((x) => x.url));
    });
    saveData(C, C.data);
    return json(res, 200, { ok: true });
  }],

  ['POST', /^\/api\/upload$/, (req, res, m, body, C) => {
    const m2 = typeof body.dataUrl === 'string'
      ? body.dataUrl.match(/^data:(image\/(?:png|jpeg|gif|webp));base64,([A-Za-z0-9+/=\s]+)$/)
      : null;
    if (!m2) return json(res, 400, { error: '仅支持 png / jpg / gif / webp 图片' });
    const buf = Buffer.from(m2[2].replace(/\s+/g, ''), 'base64');
    if (!buf.length) return json(res, 400, { error: '图片内容为空' });
    if (buf.length > 8 * 1024 * 1024) return json(res, 400, { error: '图片需在 8MB 以内' });
    const fname = `${uid('u')}.${IMG_TYPES[m2[1]]}`;
    try {
      fs.mkdirSync(C.uploadDir, { recursive: true });
      fs.writeFileSync(path.join(C.uploadDir, fname), buf);
    } catch (_) {
      return json(res, 500, { error: '图片保存失败' });
    }
    return json(res, 200, { ok: true, url: `/uploads/${fname}`, name: S(body.name, 120) || '图片' });
  }],

  ['POST', /^\/api\/tasks\/([A-Za-z0-9_-]+)\/entries$/, (req, res, m, body, C) => {
    const task = findTask(C, m[1], req);
    if (!task) return json(res, 404, { error: '没有这一话' });
    const ne = normalizeEntry(body);
    if (ne.error) return json(res, 400, { error: ne.error });
    task.entries.push(ne.value);
    task.updatedAt = ne.value.time;
    saveData(C, C.data);
    return json(res, 200, { ok: true, task });
  }],

  ['PATCH', /^\/api\/tasks\/([A-Za-z0-9_-]+)\/entries\/([A-Za-z0-9_-]+)$/, (req, res, m, body, C) => {
    const task = findTask(C, m[1], req);
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
    if (body.reviewAt !== undefined) entry.reviewAt = S(body.reviewAt, 16);
    if (body.relation !== undefined) entry.relation = normalizeRelation(body.relation);
    /* 附件做差集清理；清理前先快照，让删除的文件仍可从快照恢复。 */
    let assetSnapshotMade = false;
    if (body.images !== undefined) {
      const old = (entry.images || []).map((x) => x.url);
      entry.images = normalizeImages(body.images);
      const removed = orphansOf(old, entry.images);
      if (removed.length) { createSnapshot(C, 'before-delete'); assetSnapshotMade = true; }
      removeUploads(C, removed);
    }
    if (body.tables !== undefined) entry.tables = normalizeTables(body.tables);
    if (body.files !== undefined) {
      const old = (entry.files || []).map((x) => x.url);
      entry.files = normalizeFiles(body.files);
      const removed = orphansOf(old, entry.files);
      if (removed.length && !assetSnapshotMade) createSnapshot(C, 'before-delete');
      removeUploads(C, removed);
    }
    task.updatedAt = stampLocal(0, `${pad(new Date().getHours())}:${pad(new Date().getMinutes())}`);
    saveData(C, C.data);
    return json(res, 200, { ok: true, task });
  }],

  ['POST', /^\/api\/import$/, (req, res, m, body, C) => {
    let importedMangas = null;
    let importedTasks = null;
    if (Array.isArray(body.mangas)) {
      importedMangas = body.mangas.slice(0, 100).map((manga, i) => ({
        id: manga && typeof manga.id === 'string' && /^m_[\w-]+$/.test(manga.id) ? manga.id : uid('m'),
        title: S(manga && manga.title, 80) || `漫画 ${i + 1}`,
        createdAt: S(manga && manga.createdAt, 40) || new Date().toISOString(),
        tasks: cleanTasks(Array.isArray(manga && manga.tasks) ? manga.tasks : []),
      }));
      if (!importedMangas.length) return json(res, 400, { error: '备份里没有漫画' });
    } else if (Array.isArray(body.tasks)) {
      importedTasks = cleanTasks(body.tasks);
    } else {
      return json(res, 400, { error: '缺少 mangas 或 tasks 数组' });
    }

    createSnapshot(C, 'before-import');
    if (importedMangas) {
      const requestedActive = S(body.activeMangaId, 100);
      C.data = {
        version: 2,
        seededAt: C.data.seededAt,
        activeMangaId: importedMangas.some((manga) => manga.id === requestedActive) ? requestedActive : importedMangas[0].id,
        mangas: importedMangas,
      };
    } else {
      mangaForRequest(C, req).tasks = importedTasks;
    }
    saveData(C, C.data);
    return json(res, 200, { ok: true, mangas: C.data.mangas.length, tasks: allTasksOf(C.data).length });
  }],

  ['DELETE', /^\/api\/tasks\/([A-Za-z0-9_-]+)\/entries\/([A-Za-z0-9_-]+)$/, (req, res, m, body, C) => {
    const task = findTask(C, m[1], req);
    if (!task) return json(res, 404, { error: '没有这一话' });
    const removed = task.entries.find((e) => e.id === m[2]);
    if (!removed) return json(res, 404, { error: '没有这一格' });
    createSnapshot(C, 'before-delete');
    task.entries = task.entries.filter((e) => e.id !== m[2]);
    removeUploads(C, (removed.images || []).map((x) => x.url));
    removeUploads(C, (removed.files || []).map((x) => x.url));
    saveData(C, C.data);
    return json(res, 200, { ok: true, task });
  }],
];

function json(res, code, obj) {
  const payload = JSON.stringify(obj);
  res.writeHead(code, {
    'Content-Type': 'application/json; charset=utf-8',
    'Cache-Control': 'no-store',
    ...SECURITY_HEADERS,
  });
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

async function handleDocUpload(req, res, query, store) {
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
    fs.mkdirSync(store.uploadDir, { recursive: true });
    fs.writeFileSync(path.join(store.uploadDir, fname), buf);
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

function serveStatic(req, res, pathname, uploadBase) {
  let base = PUBLIC_DIR;
  let rel = pathname === '/' ? 'index.html' : pathname.slice(1);
  let isUpload = false;
  if (pathname.startsWith('/uploads/')) { base = uploadBase || UPLOAD_DIR; rel = path.basename(pathname); isUpload = true; }
  const filePath = path.normalize(path.join(base, rel));
  const within = path.relative(base, filePath);
  if (!within || within === '..' || within.startsWith(`..${path.sep}`) || path.isAbsolute(within)) return notFound(res);
  fs.readFile(filePath, (err, buf) => {
    if (err) return notFound(res);
    const ext = path.extname(filePath).toLowerCase();
    const headers = {
      'Content-Type': MIME[ext] || 'application/octet-stream',
      'Cache-Control': 'no-store',
      ...SECURITY_HEADERS,
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
  let pathname;
  try { pathname = decodeURIComponent(u.pathname); }
  catch (_) { return json(res, 400, { error: '请求路径编码非法' }); }
  try {
    let store = DEFAULT_STORE;
    /* ---- 多用户模式：必须登录，身份决定数据域 ---- */
    if (MULTI) {
      const methodM = req.method.toUpperCase();
      const user = sessionUser(req);
      if (pathname === '/api/login' && methodM === 'POST') {
        let body = {};
        try { body = await readBody(req, 4096); } catch (_) { body = {}; }
        const name = String(body.name || '').trim().toLowerCase();
        const bucket = `${clientIp(req)}|${name}`;
        if (failRecord(bucket).n >= 20) return json(res, 429, { error: '错太多次了，休息 5 分钟再来' });
        const account = USERS.find((u) => u.name === name);
        if (account && safeEqual(String(body.code || '').trim(), account.code)) {
          loginFails.delete(bucket);
          res.writeHead(200, {
            'Content-Type': 'application/json; charset=utf-8',
            'Set-Cookie': authCookie(signToken(name), 30 * 86400),
          });
          return res.end('{"ok":true}');
        }
        failRecord(bucket).n += 1;
        return json(res, 401, { error: '用户名或口令不对' });
      }
      if (!user) {
        if (pathname.startsWith('/api/') && pathname !== '/api/logout') return json(res, 401, { error: '未登录或会话已失效' });
        if (pathname.startsWith('/uploads/')) return json(res, 401, { error: '未登录或会话已失效' });
        if (pathname === '/login') return serveLogin(res, true);
        res.writeHead(302, { Location: '/login' });
        return res.end();
      }
      if (pathname === '/login') { res.writeHead(302, { Location: '/' }); return res.end(); }
      store = ensureStore(user);
    } else if (ACCESS_CODE && !isLanRequest(req)) {
      /* ---- 单用户模式：口令锁（公网来源要求登录；内网直连免口令） ---- */
      const ip = clientIp(req);
      if (pathname === '/api/login' && req.method.toUpperCase() === 'POST') {
        let body = {};
        try { body = await readBody(req, 4096); } catch (_) { body = {}; }
        if (failRecord(ip).n >= 20) return json(res, 429, { error: '错太多次了，休息 5 分钟再来' });
        if (safeEqual(String(body.code || '').trim(), ACCESS_CODE)) {
          loginFails.delete(ip);
          res.writeHead(200, {
            'Content-Type': 'application/json; charset=utf-8',
            'Set-Cookie': authCookie(AUTH_TOKEN, 30 * 86400),
          });
          return res.end('{"ok":true}');
        }
        failRecord(ip).n += 1;
        return json(res, 401, { error: '口令不对' });
      }
      if (pathname === '/api/logout') {
        res.writeHead(200, { 'Content-Type': 'application/json; charset=utf-8', 'Set-Cookie': authCookie('', 0) });
        return res.end('{"ok":true}');
      }
      if (!isAuthed(req)) {
        if (pathname === '/login') return serveLogin(res, false);
        if (pathname.startsWith('/api/')) return json(res, 401, { error: '未登录或会话已失效' });
        res.writeHead(302, { Location: '/login' });
        return res.end();
      }
      if (pathname === '/login') { res.writeHead(302, { Location: '/' }); return res.end(); }
    }
    /* 单用户口令锁模式下，内网免口令用户访问 /login 直接回主页 */
    if (!MULTI && ACCESS_CODE && pathname === '/login') { res.writeHead(302, { Location: '/' }); return res.end(); }
    if (pathname.startsWith('/api/')) {
      const method = req.method.toUpperCase();
      /* ---- CSRF 防护 ----
       * 浏览器对 application/json 的跨站请求会先发 preflight，因而天然挡住；
       * 但 text/plain、application/x-www-form-urlencoded、multipart/form-data
       * 属于「简单请求」，跨站时不经 preflight 直达服务端。
       * 因此写操作强制要求自定义头 X-Requested-With：
       * 自定义头必定触发 preflight，未授权的跨站 preflight 会被浏览器拒绝。
       * 带请求体的方法还要求 JSON Content-Type——文档上传走原始二进制除外，
       * 但同样必须携带 X-Requested-With（含文档上传的 CSRF 修复）。 */
      if (['POST', 'PATCH', 'PUT', 'DELETE'].includes(method)) {
        if (req.headers['x-requested-with'] !== 'manga-log') {
          return json(res, 403, { error: '跨站请求已拒绝' });
        }
        const hasBody = method !== 'DELETE';
        if (hasBody && pathname !== '/api/upload/doc') {
          const ct = String(req.headers['content-type'] || '');
          if (!ct.includes('application/json')) {
            return json(res, 415, { error: '仅接受 application/json 请求' });
          }
        }
      }
      /* 文档上传：原始二进制 + ?name= 文件名（经上方 CSRF 校验后分发，读原始体而非 JSON） */
      if (pathname === '/api/upload/doc' && method === 'POST') {
        return await handleDocUpload(req, res, u.searchParams, store);
      }
      for (const [m, re, fn] of routes) {
        if (m !== method) continue;
        const match = pathname.match(re);
        if (!match) continue;
        let body = {};
        if (method === 'POST' || method === 'PATCH' || method === 'PUT') {
          // 图片接口硬上限 8MB，base64 后约 11MB，12MB 已含余量；其余接口 1MB
          const limit = pathname.startsWith('/api/upload') ? 12 * 1024 * 1024 : 1024 * 1024;
          try { body = await readBody(req, limit); }
          catch (e) { return json(res, e.message === 'bad json' ? 400 : 413, { error: e.message === 'bad json' ? '请求体不是合法 JSON' : '请求体过大' }); }
        }
        return fn(req, res, match, body, store);
      }
      return json(res, 404, { error: '接口不存在' });
    }
    if (req.method !== 'GET' && req.method !== 'HEAD') return notFound(res);
    return serveStatic(req, res, pathname, store.uploadDir);
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
    const modeLabel = MULTI ? '多用户' : '口令锁';
    const modeText = MULTI ? `${USERS.length} 人` : (ACCESS_CODE ? '已启用' : '未启用');
    const dataText = MULTI ? './users/<用户>/data.json' : './data.json';
    console.log('');
    console.log(`  ┌${line}┐`);
    console.log('  │  服务器改动 · 漫画志   —— 开演！           │');
    console.log(`  │  ${showHost.padEnd(41)}│`);
    console.log(`  │  监听 ${String(BIND_HOST).padEnd(14)}  ${modeLabel} ${String(modeText).padEnd(6)}     │`);
    console.log(`  │  数据文件：${dataText}${' '.repeat(Math.max(1, 31 - dataText.length))}│`);
    console.log('  │  按 Ctrl+C 闭幕                            │');
    console.log(`  └${line}┘`);
    if (BIND_HOST === '127.0.0.1') openBrowser(`http://127.0.0.1:${port}`);
  });
}

listen(BASE_PORT, 20);
