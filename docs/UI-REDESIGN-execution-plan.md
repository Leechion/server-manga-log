# LeeConnect GUI 改版执行方案（macOS 原生风格）

> 目标：把 `leeconnect/src/bin/gui.rs` 改成 `docs/LeeConnect-macos-prototype.html` 里的效果（浏览器直接打开即可，窗口上方可切换状态和明暗）。
> 取代：上一版 BoardUI 风格方案。`docs/UI-REDESIGN-spec-boardui_002.html` 只作历史参考，不再按它实现。
> 范围：只改客户端 UI（egui/eframe 0.37，glow 后端），不动协议和 host。依赖后端的功能先禁用或显示 `—`，等 `EXECUTION-PLAN.md` 对应条目完成后接线。
> 更新：2026-11-23。规模记号沿用 `EXECUTION-PLAN.md`：S < M < L < XL。

---

## 0. 设计要点（实现时以这几条为准）

1. **画面优先**：连接后侧栏自动收起，远程画面铺满内容区，只有一条悬浮工具条（HUD）。
2. **少即是多**：常驻按钮从 9 个减到 6 个（全屏、剪贴板、发送文件、切换显示器、隐私屏、更多）+ 断开；锁屏、Ctrl+Alt+Del、端口映射、远程光标、手柄放进「更多」菜单；延迟、带宽等数据放进 ⓘ 弹出框，不常驻。
3. **原生质感**：毛玻璃侧栏、统一工具栏（标题栏和工具栏合一）、系统蓝强调色、分组列表（inset grouped）、标准控件尺寸。
4. **同一套图标**：全部使用 Lucide 线条图标，描边 1.6，不再使用 emoji 和自绘图形。
5. **颜色只表达状态**：绿色 = 在线 / 已连接，橙色 = 连接中，红色 = 断开等危险操作，其余都是中性色加系统蓝。

## 1. 现状与目标差距

| 区域 | 现状（gui.rs，413 行，单文件） | 目标 |
|---|---|---|
| 窗口 | 系统标题栏 + 4 个独立面板 | 无边框窗口，统一工具栏 52px，侧栏和标题栏连成一体 |
| 主题 | egui 默认深色 | 浅色 / 深色两套 macOS 配色，默认浅色 |
| 字体 | 系统 CJK 回退（simhei / Deng / NotoSansSC-VF） | Segoe UI Variable + 微软雅黑 UI（Windows 自带，不打包） |
| 左栏 | `selectable_value` 文本列表 | 224px 毛玻璃侧栏，分「我的设备 / 其他设备」，图标 + 名称 + 在线点 |
| 未连接 | 一行提示文字 | 设备详情页：设备插画、名称、状态、「连接」按钮、两组设置列表 |
| 连接中 | 无（直接显示提示） | 居中菊花转圈 + 当前步骤文字 + 「取消」 |
| 已连接 | 画面 + 顶栏 + 底栏 8 个按钮 | 画面铺满黑底，底部居中悬浮 HUD，侧栏自动收起 |
| 浮层 | 无 | ⓘ 连接信息弹出框、更多菜单、连接码 Sheet、断开 Alert、右上角通知 |
| 状态 | `Connecting / Live / Closed` | 增加连接阶段、失败原因、首帧判定 |

## 2. 文件结构

和上一版相同。Cargo 会把 `src/bin/gui/main.rs` 识别为 `gui` bin，`build.ps1 run --bin gui` 不用改；迁移时删掉旧的 `src/bin/gui.rs`（两者不能共存）。

```
leeconnect/src/bin/gui/
├── main.rs        # 入口 + Session 网络线程（从现 gui.rs 迁出）
├── app.rs         # GuiApp 状态、整体布局、状态分发
├── theme.rs       # 配色 token、apply_visuals、字体
├── icons.rs       # Lucide 图标字体注册 + 图标常量
├── shell.rs       # 无边框窗口：统一工具栏、拖动、窗口按钮、缩放边缘
├── widgets.rs     # 侧栏项、分组列表行、按钮、开关、下拉、HUD 按钮、菊花
└── views/
    ├── idle.rs        # 设备详情页
    ├── connecting.rs  # 连接中
    ├── live.rs        # 已连接：画面 + HUD
    └── overlays.rs    # 弹出框、菜单、Sheet、Alert、通知
leeconnect/assets/lucide.ttf   # Lucide 图标字体（ISC 许可，随附 LICENSE）
```

## 3. 分阶段执行

每个阶段结束时都要能编译运行，并截图和原型对照。

### M0 · 拆文件，行为不变（S）
- 按第 2 节拆分，逻辑一行不改。
- **验收**：`cargo build --bin gui` 通过；host+gui 双进程画面、鼠标输入和改版前一样。

### M1 · 配色、字体、图标（M）
- `theme.rs`：`Palette` 结构体 + `LIGHT` / `DARK` 常量（取值见第 5 节），`apply(ctx, &Palette)` 设置：
  - `visuals.panel_fill = content`，`window_fill = group`，`extreme_bg_color = field`，`faint_bg_color = hover`
  - `widgets.inactive / hovered / active`：`weak_bg_fill`、`bg_stroke`（按钮描边）、`fg_stroke`、`corner_radius(7)`
  - `selection.bg_fill = accent`（文字选区用 30% 透明度），`hyperlink_color = accent`
  - `window_corner_radius = 12`，`window_shadow` / `popup_shadow` 用第 5 节的浮层阴影
  - `spacing.item_spacing = (8, 6)`，`button_padding = (14, 5)`，`interact_size.y = 28`
- 字体：按顺序从 `C:\Windows\Fonts` 读取 `SegUIVar.ttf`（Win11）→ `segoeui.ttf`（Win10）作为拉丁字体；中文按 `msyh.ttc` → 现有候选列表回退。`msyh.ttc` 是字体集合，需设置 `FontData.index`，**以 egui 0.37 源码为准**。半粗体单独注册 `FontFamily::Name("semibold")`（`seguisb.ttf`），因为 egui 不能按字重选字体。
- 图标：`icons.rs` 用 `include_bytes!` 注册 `lucide.ttf` 为 `FontFamily::Name("icons")`，图标常量是 `char`（码位从 lucide-static 包的 `font/info.json` 取，**以实际下载的版本为准**）。需要的图标约 20 个：`panel-left search monitor pc-case laptop plus info maximize clipboard folder-up eye-off ellipsis power lock keyboard network gamepad-2 check copy chevrons-up-down mouse-pointer-2`。
  - 备选方案：`egui_extras` 开 `svg` feature，用 `include_image!` 加载 SVG 并 `tint` 上色。体积更大，只在字体方案不可行时用。
- **验收**：现有布局下整体变成 macOS 浅色配色，中英文和图标都正常显示，没有方块字。

### M2 · 窗口外壳（M，含一项技术验证）
- `ViewportBuilder::with_decorations(false).with_transparent(true).with_min_inner_size([960.0, 600.0])`。
- **统一工具栏**（52px）：左侧侧栏开关按钮 + 标题（设备名 14 半粗体 + 一行状态小字），右侧未连接时是搜索框，已连接时是 ⓘ 按钮。
  - 空白处按下时发送 `ViewportCommand::StartDrag`；双击发送 `Maximized` 切换。
  - **窗口按钮**（已定）：Windows 样式，放在工具栏最右侧：最小化 / 最大化（还原）/ 关闭，每个 46×52，图标 10px 细线；悬停时前两个变 `hover` 底色，关闭变 `#C42B1C` 红底白图标。原型左上角的红黄绿**不做**，工具栏左侧的侧栏开关按钮从 14px 内边距开始。
  - 缩放：无边框窗口需要自己处理边缘缩放（`ViewportCommand::BeginResize`），**以 0.37 源码为准**。
- **毛玻璃侧栏（技术验证）**：在 Windows 11 上用 `window-vibrancy` 库给窗口加 Mica 或 Acrylic，侧栏区域填半透明色（`side` token），内容区填不透明色。
  - 先单独做一个最小 demo，确认三件事：能否从 eframe 0.37 拿到窗口句柄、glow 后端透明窗口是否正常、和 `window-vibrancy` 版本是否兼容。
  - 不可行或 Windows 10 时退回不透明侧栏色（浅色 `#ECECEF`，深色 `#2A2A2D`），观感接近，不影响其他阶段。**已确认 Windows 10 接受不透明侧栏**，运行时按系统版本选择，不需要额外提示用户。
- **验收**：窗口可拖动、可双击最大化、可边缘缩放；侧栏开关能收起 / 展开（`ctx.animate_bool` 做 280ms 宽度动画）。

### M3 · 组件（M）
每个组件都是 `fn xxx(ui: &mut Ui, ...) -> Response`，颜色统一从 `Palette` 取。

| 组件 | 规格 | 实现要点 |
|---|---|---|
| `side_item` | 高 30，圆角 7，图标 17 系统蓝 + 名称 13 + 右侧在线点 7px / 「本机」小字 | 选中底色 `sel`，悬停 `hover`；离线时图标和文字变灰 |
| `group(ui, \|ui\| ..)` | 白底（深色 `#2B2B2E`），圆角 10，1px 边缘线 | `Frame::new().fill(group).corner_radius(10).stroke(edge)` |
| `group_row` | 高 40，左右内边距 14，行间分隔线从左边 14px 开始 | 分隔线用 `painter.hline` |
| `push_button` | 高 28，圆角 7，13 中粗；`.lg` 高 32 | 次要：白底 + 描边 + 1px 阴影；主要：系统蓝实色 + 深一级描边；危险：红色实色 |
| `switch` | 32×19，滑块 15，白色带阴影 | 打开为系统蓝；`animate_bool` 做 180ms 滑动 |
| `popup_button` | 文字 + `chevrons-up-down`，悬停才显示底色 | 点击弹出 `egui::Popup` / `menu`，**以 0.37 API 为准** |
| `hud_button` | 36×34，圆角 9，图标 18 | 激活：蓝色 28% 底 + 浅蓝图标；断开按钮红色；悬停显示提示文字 |
| `spinner` | 30px，12 根辐条依次变淡 | 自绘，`request_repaint` 驱动；不用 egui 默认圆弧 spinner |
| `menu_item` | 高 26，圆角 5，悬停整行变系统蓝 + 白字 | 右侧快捷键或勾选标记；禁用项灰字 |
| `dev_art` | 显示器 / 笔记本插画，196px 宽 | `Painter` 画圆角矩形 + 渐变屏幕（`Mesh` 两色顶点），离线时屏幕变暗 |

- **验收**：临时 `--gallery` 参数打开一个组件陈列页，和原型逐个对照。

### M4 · 三个主视图（M）
- **未连接 `views::idle`**：可滚动，内容居中最大宽 540。
  - 设备插画 → 设备名（22 半粗体）→ 状态行（点 + 「在线 · 局域网 · Windows 11」）→「输入连接码…」+「连接」。离线设备的「连接」禁用并显示「设备离线」。
  - 选中本机时，按钮换成「本机连接码 482 193 + 拷贝」，不显示连接选项。连接码接线依赖 #3，之前显示 `—`。
  - 「设备信息」组：分辨率、上次连接、连接方式。
  - 「连接选项」组：画质（流畅 / 均衡 / 清晰）、播放远程声音、同步剪贴板。声音依赖 #2，先禁用。
- **连接中 `views::connecting`**：居中菊花 + 「正在连接『工位电脑』」+ 当前步骤（正在握手… / 正在协商画质与编码… / 正在接收画面…）+「取消」。
- **已连接 `views::live`**：
  - 进入时自动收起侧栏；内容区黑底 `#0A0A0B`，画面等比缩放居中。
  - **坐标映射逻辑原样保留**，只把可用区域换成内容区。
  - HUD 用 `egui::Area` 锚定在底部居中，离底 18px，背景是 72% 不透明的深灰 + 白色 10% 描边。鼠标静止 3 秒且不在 HUD 附近时淡出，移到下方 120px 内重新出现；全屏时同样适用。
- **验收**：1280×800 窗口里三个视图和原型一致；连接后画面宽度 ≥ 1200px（侧栏已收起）；四个角点击坐标准确。

### M5 · 状态模型与断开（M）
- 网络线程的状态扩展：
  ```rust
  enum Stage { Handshake, Negotiate, Stream }
  enum Status {
      Idle,
      Connecting { stage: Stage, started: Instant },
      Live { since: Instant },
      Failed(String),
  }
  ```
  - `client.connect` 前设为 `Handshake`；握手完成设为 `Negotiate`；`open_bi` 后设为 `Stream`。
  - **收到第一帧解码画面**才设为 `Live`（现在是 `open_bi` 后就设为 Live）。`Stream` 超过 10 秒没有画面转 `Failed("10 秒内没有收到画面")`。
  - 出错设为 `Failed(e.to_string())`，界面弹 Alert，按钮「好」+「重试」。
- **取消 / 断开**：新增 `stop: Arc<AtomicBool>`，网络循环的 tick 分支检查，置位后 `conn.close(0u32.into(), b"bye")` 并退出。HUD 的电源按钮先弹 Alert（应用图标 + 「断开与『工位电脑』的连接？」+ 取消 / 断开），确认后才置位。
- **设备切换**：已连接时侧栏点击其他设备不生效（原型行为），需要先断开。
- **验收**：不启动 host 运行 gui，3 秒内出现失败 Alert 并显示真实错误；启动 host 后能依次看到三个步骤文字，然后进入画面；断开后回到设备详情页。

### M6 · 浮层与反馈（M）
| 浮层 | 触发 | 内容 | 现在能否接真数据 |
|---|---|---|---|
| ⓘ 连接信息弹出框 | 工具栏 ⓘ | 「连接质量 良好」+ 大号延迟数字 + 30 秒延迟折线 + 分辨率 / 帧率 / 编码 / 带宽 / 时长 | 延迟先用 QUIC `conn.rtt()`；带宽累加 payload 字节；时长本地计时；分段延迟归因（#16）以后加 |
| 更多菜单 | HUD「…」 | 锁定远程电脑、发送 Ctrl+Alt+Del、端口映射…、显示远程光标 ✓、手柄（未检测到） | 全部依赖后端，先禁用 |
| 连接码 Sheet | 「输入连接码…」 | 从工具栏下方落下，6 个数字格（3+3 分组），支持整段粘贴，填满前「连接」禁用 | 依赖 #3 |
| 通知 | 剪贴板同步、隐私屏开关等事件 | 右上角，应用图标 + 标题 + 一行说明，2.6 秒后滑出 | 剪贴板事件可接 |

- 弹出框、菜单、通知用 `egui::Area` + 半透明填充；egui 不能做真正的背景模糊，用 86–88% 不透明度近似，观感足够。
- Sheet 和 Alert 用 `egui::Modal`，遮罩 10% 黑，**以 0.37 API 为准**。
- 按 Esc 关闭所有浮层；点击浮层外部关闭弹出框和菜单。

### M7 · 外观设置与持久化（S）
- 外观**默认固定浅色**（已定），不跟随系统；设置里可以手动切到深色。启动时如果 `ui.json` 不存在就用浅色。
- eframe 没开 `persistence` feature，不额外开启。用 `serde_json` 读写 `%APPDATA%\LeeConnect\ui.json`：外观、侧栏展开状态、画质、声音和剪贴板开关、窗口大小。
- **验收**：重启后保持上次设置。

### M8 · 视觉验收（S）
- 浅色、深色各截一套图：未连接（远程设备 / 本机 / 离线设备）、连接中、已连接、HUD 更多菜单、ⓘ 弹出框、连接码 Sheet、断开 Alert、失败 Alert，和原型并排对比。
- 三种窗口尺寸：960×600（最小）、1280×800、1920×1080。确认文字不截断、HUD 不遮挡画面关键区域。
- 截图放 `leeconnect/target/ui-macos/`，在 `EXECUTION-PLAN.md` #1 下补一条记录。

## 4. egui 的限制与替代做法

| 原型效果 | egui 0.37 现状 | 替代做法 |
|---|---|---|
| 侧栏背景模糊 | 不能模糊窗口后面的内容 | Windows 11 用 Mica / Acrylic（M2 技术验证）；否则用不透明侧栏色 |
| 浮层背景模糊 | 不能模糊 egui 自己画的内容 | 提高不透明度到 86–88% |
| 按钮顶部高光渐变 | 没有原生渐变 | 实色 + 深一级描边；需要时用 `Mesh` 画两色顶点 |
| 多层阴影 | `Shadow` 只有一层（`offset: [i8; 2]`、`blur: u8`、`spread: u8`、`color`） | 取最明显的一层，再加 1px 描边模拟边缘线 |
| 字重 500 / 600 | 不能按字重选择字体 | 单独注册 Segoe UI Semibold 字体族 |
| 等宽数字 | 不支持 OpenType 特性 | 跳动的数字（延迟、时长、连接码）用 Monospace 字体族 |
| CSS 过渡动画 | 只有 `animate_bool` / `animate_value` | 只给侧栏收起、开关、通知滑入、HUD 淡出加动画 |

已核对的 0.37 类型：`Frame` 字段为 `inner_margin / fill / stroke / corner_radius / outer_margin / shadow`；`WidgetVisuals` 字段为 `bg_fill / weak_bg_fill / bg_stroke / corner_radius / fg_stroke / expansion`；`Visuals` 含 `panel_fill / window_fill / extreme_bg_color / faint_bg_color / window_corner_radius / window_shadow / popup_shadow / selection / widgets`。`Margin` 和 `CornerRadius` 是整数类型，字面量直接写整数。

**待编译确认**：`Modal`、`Popup`、`Panel::*` 的方法名，`FontData.index`，系统主题跟随，`BeginResize`，eframe 窗口句柄获取。本机 cargo 缓存目录没有读取权限，这些在实现时以 `cargo check` 和本地 crate 源码为准。

## 5. Token 表（写入 `theme.rs`）

| Token | 浅色 | 深色 | 用途 |
|---|---|---|---|
| `content` | `#F6F6F8` | `#1F1F21` | 内容区、工具栏 |
| `side` | `rgba(238,238,242,.66)`，不透明回退 `#ECECEF` | `rgba(40,40,44,.62)`，回退 `#2A2A2D` | 侧栏 |
| `group` | `#FFFFFF` | `#2B2B2E` | 分组列表 |
| `edge` | `rgba(0,0,0,.07)` | `rgba(255,255,255,.06)` | 分组外描边 |
| `field` | `#FFFFFF` | `#1C1C1E` | 输入框、连接码格 |
| `btn` / `btn_edge` | `#FFFFFF` / `rgba(0,0,0,.13)` | `#3A3A3D` / `rgba(255,255,255,.08)` | 次要按钮 |
| `text` | `#1D1D1F` | `#F5F5F7` | 主文字 |
| `text_2` | `rgba(60,60,67,.64)` | `rgba(235,235,245,.62)` | 次要文字 |
| `text_3` | `rgba(60,60,67,.38)` | `rgba(235,235,245,.32)` | 分组标题、占位 |
| `sep` | `rgba(60,60,67,.12)` | `rgba(255,255,255,.08)` | 分隔线 |
| `hover` / `sel` | `rgba(0,0,0,.045)` / `rgba(0,0,0,.075)` | `rgba(255,255,255,.06)` / `rgba(255,255,255,.10)` | 悬停、选中 |
| `pop` | `rgba(250,250,252,.86)` | `rgba(44,44,48,.88)` | 弹出框、通知、Alert |
| `accent` | `#007AFF` | `#0A84FF` | 主按钮、开关、图标、焦点环（32–40% 透明） |
| `green` | `#34C759` | `#30D158` | 在线、已连接 |
| `orange` | `#FF9F0A` | 同左 | 连接中 |
| `red` | `#FF3B30` | `#FF453A` | 断开 |
| `live_bg` | `#0A0A0B` | 同左 | 已连接时画面背景 |
| `hud` | `rgba(32,32,36,.72)` + 白 10% 描边 | 同左 | 悬浮工具条 |

尺寸：工具栏 52、侧栏 224、侧栏项 30、分组行 40、按钮 28 / 32、HUD 按钮 36×34。圆角：窗口 12、分组 10、弹出框 12、Alert 14、HUD 14、按钮 7、菜单项 5。字号：标题 22 / 15 / 14，正文 13，小字 11–12.5。

延迟着色：< 40ms 绿色「良好」，40–90ms 橙色「一般」，> 90ms 红色「较差」。

```rust
// theme.rs 骨架（M1）
use egui::{Color32, CornerRadius, Shadow, Stroke, Visuals};

pub struct Palette {
    pub dark: bool,
    pub content: Color32, pub side: Color32, pub group: Color32, pub edge: Color32,
    pub field: Color32, pub btn: Color32, pub btn_edge: Color32,
    pub text: Color32, pub text_2: Color32, pub text_3: Color32,
    pub sep: Color32, pub hover: Color32, pub sel: Color32, pub pop: Color32,
    pub accent: Color32, pub green: Color32, pub orange: Color32, pub red: Color32,
}

pub const fn rgb(h: u32) -> Color32 {
    Color32::from_rgb((h >> 16) as u8, (h >> 8) as u8, h as u8)
}

pub fn apply(ctx: &egui::Context, p: &Palette) {
    let mut v = if p.dark { Visuals::dark() } else { Visuals::light() };
    v.panel_fill = p.content;
    v.window_fill = p.group;
    v.extreme_bg_color = p.field;
    v.faint_bg_color = p.hover;
    v.window_corner_radius = CornerRadius::same(12);
    v.window_shadow = Shadow { offset: [0, 10], blur: 30, spread: 0,
                               color: Color32::from_black_alpha(if p.dark { 128 } else { 56 }) };
    v.popup_shadow = v.window_shadow;
    v.selection.bg_fill = p.accent.gamma_multiply(0.3);
    v.selection.stroke = Stroke::new(1.0, p.accent);
    for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active] {
        w.corner_radius = CornerRadius::same(7);
    }
    v.widgets.inactive.weak_bg_fill = p.btn;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, p.btn_edge);
    v.widgets.hovered.weak_bg_fill = p.btn;
    v.widgets.active.weak_bg_fill = p.sel;
    v.hyperlink_color = p.accent;
    ctx.set_visuals(v);
}
```

## 6. 依赖 `EXECUTION-PLAN.md` 的接线点

| UI 元素 | 依赖条目 | 依赖完成前的显示 |
|---|---|---|
| 连接码 Sheet、本机连接码、真实设备列表 | #3 双机 LAN | Sheet 可打开，「连接」提示暂未开放；本机连接码显示 `—`；设备列表用占位数据 |
| 播放远程声音 | #2 音频 | 开关禁用 |
| 切换显示器 | #4 | HUD 按钮禁用，提示「只有一个显示器」 |
| 显示远程光标 | #5 | 菜单项禁用 |
| 锁定远程电脑、Ctrl+Alt+Del | #6 | 菜单项禁用 |
| 隐私屏 | #7 | HUD 按钮禁用 |
| 手柄 | #8 | 菜单项禁用，显示「未检测到」 |
| 画质选项真正生效 | #9 | 只存本地 |
| 端口映射 | 已有 `portmap.rs` | 菜单项可打开，面板后续单独设计 |
| 分段延迟 | #16 | 弹出框只显示总延迟（`conn.rtt()`） |

原则：没有真数据时显示 `—` 或禁用并说明原因，**不把原型里的示例数字写进产品代码**。

## 7. 已确认的决定（2026-11-23）

1. **窗口按钮**：Windows 样式，放右上角（最小化 / 最大化 / 关闭）。原型里的红黄绿不实现。
2. **Windows 10 支持**：支持，侧栏用不透明色；Windows 11 尝试 Mica / Acrylic。
3. **外观默认值**：固定浅色，不跟随系统；用户可在设置里改成深色，选择会被保存。

## 8. 风险

- **无边框窗口**：拖动、缩放、Aero Snap（拖到屏幕边缘分屏）在无边框窗口上可能不完整。M2 里先验证，不行就保留系统标题栏，只把工具栏放在标题栏下方（失去统一工具栏的效果，其他不受影响）。
- **`window-vibrancy` 兼容性**：需要和 eframe 0.37 的窗口句柄以及 glow 透明窗口配合，未验证。失败时退回不透明侧栏，不阻塞其他阶段。
- **egui 0.37 API 变动**：每个阶段都以 `cargo check` 和本地 crate 源码为准，不照抄旧版示例。
- **图标字体码位**：Lucide 不同版本码位可能变化，`icons.rs` 必须和打包的 `lucide.ttf` 来自同一个版本。
- **帧率**：浮层和动画增加后，UI 帧耗时要保持在 2ms 以内，不能拖慢 60fps 画面。HUD 淡出后不再每帧重绘它。
- **首帧判定**：host 迟迟不发画面时会停在「正在接收画面…」，已加 10 秒超时。

## 9. 排期汇总

| 阶段 | 内容 | 规模 | 前置 |
|---|---|---|---|
| M0 | 拆文件 | S | — |
| M1 | 配色、字体、图标 | M | M0 |
| M2 | 窗口外壳 + 毛玻璃验证 | M | M1 |
| M3 | 组件 | M | M1 |
| M4 | 三个主视图 | M | M2、M3 |
| M5 | 状态模型 + 断开 | M | M4 |
| M6 | 浮层与反馈 | M | M4 |
| M7 | 外观设置 + 持久化 | S | M1 |
| M8 | 视觉验收 | S | M4–M7 |

M2 和 M3 可以并行；M5 和 M6 可以并行。

## 10. 最终验收清单

- [ ] 所有界面在浅色、深色下都和原型一致（配色、圆角、间距、图标）
- [ ] 界面上没有 emoji，图标全部来自同一套 Lucide
- [ ] 连接后侧栏自动收起，常驻控件只有 HUD 的 7 个按钮
- [ ] 连接过程显示三个步骤；失败时显示真实错误并能重试
- [ ] 断开需要确认，确认后会话真正关闭并回到设备详情页
- [ ] 没接通的功能都是禁用状态并有原因提示，没有写死的假数据
- [ ] 窗口可拖动、缩放、最大化；外观和侧栏状态重启后保持
- [ ] 右上角是 Windows 样式窗口按钮；Windows 10 上侧栏为不透明色且显示正常；首次启动为浅色
- [ ] 鼠标坐标映射和改版前一致，60fps 画面不掉帧
