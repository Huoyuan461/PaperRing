# 纸环 PaperRing · 桌面壳（shell/）

把已经换皮完成的纸环后端（Spring Boot bootJar，浏览器界面）包装成普通用户双击就能用的桌面软件。本目录是**自研薄 Tauri 壳**，不含任何业务逻辑。

## 架构

```
┌──────────────────── 纸环 PaperRing.exe(Tauri 壳,Rust) ───────────────────┐
│                                                                            │
│  启动 ──► 选空闲端口 ──► 拉起子进程 ──► 轮询健康检查 ──► 窗口切到应用界面    │
│                                                                            │
│   resources/jre/bin/java.exe -Xmx1g -jar resources/app.jar                 │
│        --server.port=<空闲端口> --server.address=127.0.0.1                 │
│                                                                            │
│   窗口(Tauri WebView)  http://127.0.0.1:<端口>/  ← 后端自带的前端 UI        │
│                                                                            │
│  退出 ──► 杀 java 子进程(Windows 另有 Job Object 系统级托管兜底)          │
└────────────────────────────────────────────────────────────────────────────┘
   安装包内容:NSIS 安装器 = 壳 exe + app.jar(约 297MB)+ 裁剪 JRE + 图标
```

关键设计（对应 `src/main.rs`，注释里有详细说明）：

- **端口不写死**：启动时让操作系统分配空闲端口，避免和用户机器上已有服务（包括他自己装的其他版纸环）冲突。
- **启动页**：后端就绪前窗口先显示 `ui/index.html`（纯静态：纸环标记 + “正在启动…”），健康检查通过（HTTP 200–399，超时 120 秒）后才 navigate 到应用；启动失败会把启动页文案换成失败提示，不让用户对着转圈干等。
- **子进程回收三保险**：① 应用退出事件里显式 `kill`；② Windows Job Object 的 `KILL_ON_JOB_CLOSE`——壳被强杀/崩溃时系统连带回收 java；③ java 的 stdout/stderr 全部并入日志，出问题可查。
- **日志**：`<用户数据目录>/paperring-shell.log`（Windows 约为 `%APPDATA%\cn.paperring.app\paperring-shell.log`），壳与后端日志同一个文件。
- **后端工作目录**设为用户数据目录：后端要写的配置/缓存有稳定可写的落点，不依赖安装目录权限。
- **只绑 127.0.0.1**：界面仅供本机使用，顺带避开 Windows 防火墙弹窗。

## 目录说明

| 路径 | 内容 |
|---|---|
| `src/main.rs` | 壳核心逻辑（端口/JRE 拉起/健康检查/子进程回收/日志） |
| `tauri.conf.json` | 产品名、identifier `cn.paperring.app`、版本 1.0.0、窗口、图标、资源映射、NSIS 配置 |
| `ui/index.html` | 本地启动页（纯静态，无外部资源） |
| `icons/` | 图标套图，由 `gen-icons.py` 从品牌主图生成，可重跑 |
| `gen-icons.py` | 图标生成脚本（Pillow；等价于 `cargo tauri icon`） |
| `resources/` | **构建时填充**：`app.jar` + `jre/`，仓库里只有 `README-STAGE.txt` 占位 |
| `.github/workflows/release.yml` | Windows 云端构建（见下节，有一个搬家动作） |
| `Cargo.toml` / `build.rs` | Rust 工程定义（Tauri v2 + windows-sys 的 Job Object） |

## 本地开发怎么跑

前置：装好 Rust 工具链（rustup）和 Tauri CLI（`cargo install tauri-cli --version "^2"` 或 `npm i -g @tauri-apps/cli@^2`），Windows 上另需 WebView2 运行时（Win10/11 一般自带）。

壳跑起来之前必须先把资源填进 `resources/`（布局照 `resources/README-STAGE.txt`）：

1. **app.jar**：在工程根目录构建后端（Mac 上已有现成流程 `build-mac.sh`；或任意平台 `STIRLING_FLAVOR=core gradle :stirling-pdf:bootJar -x test -PbuildWithFrontend=true`），把产出的 `app/core/build/libs/stirling-pdf-*.jar` 复制为 `shell/resources/app.jar`。
2. **jre/**：用本机 JDK 25 的 jlink 裁一个当前平台的运行时出来，命令与 CI 里的一致（模块集 = `jdeps --print-module-deps app.jar` 的结果 ∪ workflow 里那份保守补充集），输出到 `shell/resources/jre`。只需要验证壳逻辑时，也可以临时把 jlink 的 `--add-modules` 换成 `ALL-MODULE-PATH` 先跑通再说体积。

然后在本目录：

```bash
cargo tauri dev     # 调试运行(窗口 + 控制台日志)
cargo tauri build   # 正式打包(Windows 出 NSIS 安装包)
```

## 云端出包（GitHub Actions）

流程（`.github/workflows/release.yml`）：推送 `v*` 标签触发 → windows-latest 上依次装 Temurin JDK 25 / Node 22 / go-task 3.44.1 → 在仓库根跑 gradle 构建 bootJar（core 口味、带前端、npm 走 npmmirror 镜像，参数与 `build-mac.sh` 已验证的组合对齐）→ jdeps+jlink 裁出 Windows JRE → jar 改名 `app.jar` 与 JRE 一起填入 `shell/resources/` → 装 Rust + Tauri CLI 打 NSIS 安装包 → 上传 artifact；打 tag 时同时用 softprops/action-gh-release 发布 Release。也支持 Actions 页手动触发（只产 artifact，不发 Release），适合首轮调试。

> **⚠ 建仓时的搬家动作**：GitHub 只识别**仓库根目录**下的 `.github/workflows/`。本文件随工程放在 `shell/.github/workflows/` 只是开发期的存放位置，建仓推送时要把它移到仓库根的 `.github/workflows/release.yml`（文件内容已经按“仓库根 = paperring 根目录”书写，路径不用改）。整个 paperring 工程就是那个 git 仓，`shell/` 是它的子目录。

## 距离第一个 Windows 安装包还差什么

1. **建 GitHub 仓库并推送**（待拍板后执行）：把 paperring 整仓推上去 + 上面那个 workflow 搬家动作。推完打一个 `v1.0.0` 标签即可触发首轮云端构建。
2. **首次 CI 即编译验证点**：本脚手架在开发机（Linux VM，无 Rust 工具链）上**没有本地编译验证过**，`cargo check` 级别的把握来自逐行对 Tauri v2 / windows-sys 0.59 API 的核对，但不能等同于编译通过。首轮 CI 构建就是验证点，预期可能要按报错微调一两个来回（风险点已尽量前置规避：资源目录做了双布局兜底、windows-sys 只用了最常用的 Job Object 接口）。
3. **代码签名证书未买**：未签名的 NSIS 安装包在 Windows 上会被 SmartScreen 拦截（“Windows 已保护你的电脑”，需点“更多信息 → 仍要运行”）。应对路线：
   - 当前阶段：自用/小范围测试，拦截提示可手动放行，不影响功能验证；
   - 正式售卖前：买 OV 代码签名证书（约 $200–400/年，下单前再询价），把证书指纹配进 `tauri.conf.json` 的 `bundle.windows.certificateThumbprint`（或 CI 里用签名步骤），签名后 SmartScreen 拦截基本消失。
4. **商标/域名等发布前法务**：不在本工程范围，按既有清单走。

## Mac 版后续

同一套壳代码直接复用（`main.rs` 里 java 路径、子进程回收都已按平台分开写；图标套图里 icns 已备好）。补两件事即可：

- CI 增加 `macos-latest` job：同样流程构建 jar → jlink 裁 macOS JRE（arm64；如需 Intel 版再加一个架构维度）→ `tauri build` 出 .app/.dmg。
- **公证**：需要 Apple Developer 账号（$99/年），把签名+公证凭据配成 CI secrets 后 Tauri 构建可自动完成；不公证的 .dmg 在用户机器上会被 Gatekeeper 拦下（能右键打开绕过，但不适合卖货）。
