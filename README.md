# 服务器改动 · 漫画志（Server Manga Log）

<p align="center">
  <img src="docs/img/01-overview.png" width="100%" alt="服务器改动·漫画志 主页全貌">
</p>

> **生化环材的同学做实验，要手写实验记录：**
> 日期、页码、现象、数据、导师签字，一样不能少。
>
> 而我们计算机的"实验"跑在服务器上——
> 改了哪行配置、重启了哪个服务、当时为什么这么改，
> 三天之后只剩一句：「我记得我改过啊？」
>
> **把服务器当实验室，把每一次改动画成漫画。**
> 一件事 = 一「话」，一次改动 = 一「格」：
> 时间、类型印章、结果徽章、命令、路径、截图、数据表，
> 全部画进黑白漫画的方格里——谁改的、为什么改、怎么验证的，一目了然。
>
> 实验记录别人手写，你的记录——**画出来**。

一款**纯本地、零依赖**的改动记录工具：黑白漫画风 + 手稿线条，
一个 Node 就能跑（连数据库都不用），数据永远只落在你自己的磁盘上。

---

## 📖 目次

- [分格：一次改动，一格漫画](#分格一次改动一格漫画)
- [细节自动排版：路径・文件名・命令・链接](#细节自动排版路径文件名命令链接)
- [附件：贴图 + 文档 + 在线预览](#附件贴图--文档--在线预览)
- [快记一笔 & 遗留事项](#快记一笔--遗留事项)
- [卷末统计](#卷末统计)
- [表格编辑器](#表格编辑器)
- [口令锁：内网免登录，公网设防](#口令锁内网免登录公网设防)
- [快速开始](#快速开始)
- [部署到服务器 · 外网访问](#部署到服务器--外网访问)
- [数据与备份](#数据与备份)

## 分格：一次改动，一格漫画

<p align="center">
  <img src="docs/img/02-panels.png" width="72%" alt="漫画分格">
</p>

每一格自带编号、时间戳和一枚**类型印章**：配置「改」、代码「码」、部署「上」、修复「修」、排查「查」、回滚「撤」。
结果用徽章标注——实心「成功」、虚线「待验证」、斜纹「✗ 失败」，翻一眼就知道这话进展到哪。
整话结束还有一枚「つづく」：**未完待续，下一格由你来画。**

## 细节自动排版：路径・文件名・命令・链接

写细节的时候不用管格式，随手贴——渲染时自动区分：

| 你写的 | 它变成 |
|---|---|
| `/data2/lee/outputs/best.pth` 的父目录、`~/conf` | <code>路径</code>：等宽 + 底纹 + 实线下划 |
| `nginx.conf`、`best.pth`、`train.py` | <code>文件名</code>：等宽 + 虚线下划 |
| `$ git push origin main`、纯命令行、``` 围栏块 | <code>命令</code>：反白黑块 + `$` 提示符 |
| `http://...` | <code>链接</code>：波浪下划，点击直达 |

## 附件：贴图 + 文档 + 在线预览

<p align="center">
  <img src="docs/img/03-preview-csv.png" width="72%" alt="CSV 在线预览">
</p>

- **📎 贴图**：选图、**Ctrl+V 直接粘贴截图**、或把图片拖进细节框；点缩略图灯箱放大
- **📄 文档**：日志、配置、脚本、PDF、压缩包（单件 50MB）都能挂进格子

<p align="center">
  <img src="docs/img/04-preview-text.png" width="72%" alt="配置文件在线预览">
</p>

- 点文件卡片**在线预览**：CSV/TSV 自动转表格、文本/YAML/代码黑底码块、PDF 内嵌翻页
- 预览满意了再点「⇩ 下载」——预览不满意，连下载都省了

<p align="center">
  <img src="docs/img/09-lightbox.png" width="72%" alt="截图灯箱">
</p>

## 快记一笔 & 遗留事项

<p align="center">
  <img src="docs/img/05-todo-quick.png" width="72%" alt="快记一笔与遗留事项">
</p>

- 顶部⚡**快记条**：一句话回车入格，类型按关键词自动识别（`systemctl restart`→部署，`回滚`→回滚）
- 每话一份**遗留事项**：还没做完的事勾掉自动划线，数据条实时显示「遗留 N 件」——
  实验记录最怕的不是没做，是**忘了没做**

## 卷末统计

<p align="center">
  <img src="docs/img/06-stats.png" width="72%" alt="卷末统计">
</p>

连载几话、画了几格、近十二周的改动节奏、哪台服务器被你折腾得最狠、
配置/代码/部署各占几格、成功率多少——**卷末一页，全说清楚。**
向导师汇报、写周报、复盘事故，直接把这页拍上去。

## 表格编辑器

<p align="center">
  <img src="docs/img/07-table-editor.png" width="72%" alt="表格编辑器">
</p>

压测数据、对比结果、巡检清单——「⊞ 表格」插一张，行列自由增减，
格子里直接渲染成漫画风表格，导出 Markdown 时自动转标准表格。

## 口令锁：内网免登录，公网设防

<p align="center">
  <img src="docs/img/08-login.png" width="52%" alt="口令锁登录页">
</p>

部署到服务器后：内网直连免口令，打开就用；走公网隧道的访客，先过「验明正身」。
错误口令限次重试，改口令即全端下线。

## 快速开始

**本地**：双击 `start.bat`（或 `node server.js`），浏览器自动打开 http://127.0.0.1:4780

- 无需 `npm install`——只用 Node 内置模块；不用数据库——数据存 `data.json`
- 指定端口 `set PORT=8080`；不自动开浏览器 `set NO_OPEN=1`

## 部署到服务器 · 外网访问

应用默认只监听 `127.0.0.1`。上服务器：

```bash
scp -r ./server-manga-log user@server:/opt/server-manga-log   # 传目录
cd /opt/server-manga-log
cp config.example.json config.json && vi config.json          # ★ 设置 accessCode
sudo cp deploy/manga-log.service /etc/systemd/system/
sudo systemctl daemon-reload && sudo systemctl enable --now manga-log
```

- 内网：`http://服务器IP:4780` 直达
- 公网：没有域名/公网 IP 也能走 Cloudflare 快速隧道（`cloudflared tunnel --url http://127.0.0.1:4780`）；
  有域名推荐命名隧道固定地址；有 VPS 可用 frp
- **公网部署务必设置 `ACCESS_CODE`**：内网直连免口令，公网来源强制登录，错误口令限次重试

详细步骤见 [README 部署章节](#部署到服务器--外网访问) 与 `deploy/` 目录。

## 数据与备份

- 记录存 `data.json`，附件存 `uploads/`——备份就是复制这一个目录
- JSON 损坏自动备份重建；「⇩ 导出备份 / ⇒ 导入备份」在卷末统计页
- 导出 Markdown：「⇩ 导出全稿」或「⇩ 本话」，路径/命令/表格/附件链接一并带走

## 目录结构

```
server-manga-log/
├── server.js            # 本地服务 + API + 图片/文档上传（零依赖）
├── start.bat            # Windows 双击启动
├── config.example.json  # 口令/监听配置模板
├── deploy/              # systemd 服务文件
├── docs/img/            # README 截图
├── data.json            # （运行时生成）你的全部记录
├── uploads/             # （运行时生成）贴图与文档
└── public/              # 页面、样式、逻辑、内置手写字体
```

## 快捷键

`N` 开新话 ｜ `/` 搜索 ｜ `Ctrl+Enter` 提交 ｜ `Esc` 关闭弹窗

---

<p align="center">
  <span style="font-size:1.4em">つづく</span><br>
  <sub>下一话，由你来记。</sub>
</p>
