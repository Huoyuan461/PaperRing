// ============================================================
// 纸环 PaperRing 桌面壳(自研薄壳)
//
// 职责只有三件:
//   1. 双击启动 → 用安装包内嵌的 JRE 拉起后端 app.jar(随机空闲端口)
//   2. 轮询健康检查,服务就绪后把窗口从本地启动页切到应用界面
//   3. 应用退出时可靠回收 java 子进程,不留孤儿进程占端口
//
// 后端本体是 Spring Boot 的 bootJar,壳不侵入业务逻辑,
// 两者只通过 127.0.0.1:<端口> 的 HTTP 通信。
// ============================================================

// 发布版不弹控制台窗口(Windows);调试版保留,方便看日志
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use tauri::Manager;

// ---------- 日志 ----------

static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();

/// 追加一行带时间戳的日志到 <用户数据目录>/paperring-shell.log,同时打到 stderr。
/// 壳没有界面日志,排障全靠这个文件;写失败也不能影响主流程,所以全程忽略错误。
fn log_line(msg: &str) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let line = format!("[{secs}] {msg}");
    eprintln!("{line}");
    if let Some(path) = LOG_PATH.get() {
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(f, "{line}");
        }
    }
}

// ---------- Windows Job Object(子进程托管) ----------

/// Windows 上最稳妥的子进程回收方式:把 java 进程加入 Job Object 并设置
/// KILL_ON_JOB_CLOSE —— 无论壳是正常退出、崩溃还是被任务管理器强杀,
/// 只要壳进程消失、句柄关闭,系统就连带杀掉组内的 java 进程。
/// 非 Windows 平台不需要这一层(Linux/macOS 由 ExitRequested 显式 kill 兜底)。
#[cfg(windows)]
mod job {
    use std::os::windows::io::AsRawHandle;
    use std::process::Child;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    pub struct JobHandle(HANDLE);

    // 句柄由 ShellState 独占持有,跨线程只发生在退出回收路径上,安全。
    unsafe impl Send for JobHandle {}
    unsafe impl Sync for JobHandle {}

    impl JobHandle {
        pub fn new() -> Option<Self> {
            unsafe {
                let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if handle.is_null() {
                    return None;
                }
                let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                let ok = SetInformationJobObject(
                    handle,
                    JobObjectExtendedLimitInformation,
                    &info as *const _ as *const std::ffi::c_void,
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                );
                if ok == 0 {
                    CloseHandle(handle);
                    return None;
                }
                Some(Self(handle))
            }
        }

        pub fn assign(&self, child: &Child) -> bool {
            unsafe { AssignProcessToJobObject(self.0, child.as_raw_handle() as HANDLE) != 0 }
        }
    }

    impl Drop for JobHandle {
        fn drop(&mut self) {
            // 句柄关闭即触发 KILL_ON_JOB_CLOSE,组内进程被系统回收
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

// ---------- 壳状态 ----------

struct ShellState {
    child: Mutex<Option<Child>>,
    #[cfg(windows)]
    job: Mutex<Option<job::JobHandle>>,
}

impl ShellState {
    fn new() -> Self {
        Self {
            child: Mutex::new(None),
            #[cfg(windows)]
            job: Mutex::new(None),
        }
    }
}

/// 显式杀掉 java 子进程并等待回收(退出路径调用;Windows 上另有 Job 兜底)。
fn kill_child(state: &ShellState) {
    if let Ok(mut slot) = state.child.lock() {
        if let Some(mut child) = slot.take() {
            log_line(&format!("回收后端进程 pid={}", child.id()));
            let _ = child.kill();
            let _ = child.wait();
        }
    }
    // Job 句柄随 state 析构关闭;这里提前释放,确保 java 先被系统回收
    #[cfg(windows)]
    if let Ok(mut slot) = state.job.lock() {
        *slot = None;
    }
}

// ---------- 资源定位 ----------

/// 在 Tauri 资源目录里找相对路径。主布局由 tauri.conf.json 的 resources 映射决定
/// (app.jar / jre 直接位于资源根下);同时兼容未映射的嵌套布局(resources/...)。
fn resource_path(resource_dir: &PathBuf, rel: &str) -> PathBuf {
    let direct = resource_dir.join(rel);
    if direct.exists() {
        return direct;
    }
    let nested = resource_dir.join("resources").join(rel);
    if nested.exists() {
        return nested;
    }
    direct // 都不存在时返回主路径,让报错信息指向预期位置
}

// ---------- 端口与健康检查 ----------

/// 让操作系统分配一个空闲端口:bind 到 0 端口后读回实际端口再释放。
/// 释放到 java 真正监听之间有极小的时间窗,理论上可能被别的程序抢占;
/// 桌面单机场景概率可忽略,失败时健康检查会超时报错并写日志,可重启重试。
/// 不写死 8080 的原因:用户机器上可能已有其他服务占用。
fn pick_free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .unwrap_or(18080)
}

/// 对应用根路径发一个最朴素的 HTTP/1.0 GET,只看状态行。
/// 返回 Some(状态码) 表示服务已在应答;None 表示还没起来。
fn http_status(port: u16) -> Option<u16> {
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().ok()?;
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(800)).ok()?;
    let _ = stream.set_read_timeout(Some(Duration::from_millis(800)));
    let req = format!("GET / HTTP/1.0\r\nHost: 127.0.0.1:{port}\r\n\r\n");
    stream.write_all(req.as_bytes()).ok()?;
    let mut buf = [0u8; 128];
    let n = stream.read(&mut buf).ok()?;
    if n == 0 {
        return None;
    }
    let head = String::from_utf8_lossy(&buf[..n]);
    // 状态行形如 "HTTP/1.1 200"
    head.split_whitespace().nth(1)?.parse().ok()
}

// ---------- 后端拉起 ----------

/// 拉起 jar 并等到 HTTP 就绪,返回所用端口。任一步失败返回 Err(人话原因)。
fn boot_backend(app: &tauri::AppHandle, state: &ShellState) -> Result<u16, String> {
    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|e| format!("拿不到资源目录:{e}"))?;

    // 内嵌 JRE 与后端 jar 都随安装包放在资源目录里
    let java = resource_path(
        &resource_dir,
        if cfg!(windows) { "jre/bin/java.exe" } else { "jre/bin/java" },
    );
    let jar = resource_path(&resource_dir, "app.jar");
    if !java.exists() {
        return Err(format!("内嵌 JRE 不存在:{}", java.display()));
    }
    if !jar.exists() {
        return Err(format!("后端程序包不存在:{}", jar.display()));
    }

    let port = pick_free_port();
    // 只绑回环:界面只供本机使用,顺带避开 Windows 防火墙弹窗
    let port_arg = format!("--server.port={port}");
    log_line(&format!(
        "启动后端:{} -Xmx1g -jar {} {} (port={})",
        java.display(),
        jar.display(),
        port_arg,
        port
    ));

    // 后端自己的 stdout/stderr 也并进壳日志,一次排障看全
    let (stdout_stdio, stderr_stdio) = match LOG_PATH.get() {
        Some(p) => {
            let f1 = OpenOptions::new().create(true).append(true).open(p).ok();
            let f2 = f1.as_ref().and_then(|f| f.try_clone().ok());
            match (f1, f2) {
                (Some(a), Some(b)) => (Stdio::from(a), Stdio::from(b)),
                _ => (Stdio::null(), Stdio::null()),
            }
        }
        None => (Stdio::null(), Stdio::null()),
    };

    // 工作目录放到用户数据目录:后端要写的配置/缓存落在这里,
    // 保证可写且卸载重装不互相干扰(资源目录在某些安装模式下只读)
    let work_dir = app.path().app_data_dir().ok();
    if let Some(d) = &work_dir {
        let _ = std::fs::create_dir_all(d);
    }

    let mut cmd = Command::new(&java);
    cmd.args([
        "-Xmx1g",
        "-Dfile.encoding=UTF-8",
        "-jar",
        &jar.to_string_lossy(),
        &port_arg,
        "--server.address=127.0.0.1",
    ])
    .stdin(Stdio::null())
    .stdout(stdout_stdio)
    .stderr(stderr_stdio);
    if let Some(d) = &work_dir {
        cmd.current_dir(d);
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("java 进程拉起失败:{e}"))?;
    log_line(&format!("后端进程已拉起 pid={}", child.id()));

    // Windows:立刻把 java 纳入 Job Object 托管(必须在它派生子进程之前,越早越好)
    #[cfg(windows)]
    {
        match job::JobHandle::new() {
            Some(j) => {
                if j.assign(&child) {
                    log_line("java 进程已加入 Job Object 托管");
                } else {
                    log_line("警告:java 进程加入 Job Object 失败,仅靠退出时显式回收");
                }
                if let Ok(mut slot) = state.job.lock() {
                    *slot = Some(j);
                }
            }
            None => log_line("警告:Job Object 创建失败,仅靠退出时显式回收"),
        }
    }

    if let Ok(mut slot) = state.child.lock() {
        *slot = Some(child);
    }

    // 轮询健康检查:Spring Boot 冷启动 + 首次加载前端资源较慢,给足 120 秒
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        // java 若已提前退出(如端口冲突/配置错误),立刻报错,不傻等超时
        if let Ok(mut slot) = state.child.lock() {
            if let Some(c) = slot.as_mut() {
                if let Ok(Some(status)) = c.try_wait() {
                    return Err(format!("后端进程提前退出:{status},详情见日志"));
                }
            }
        }
        match http_status(port) {
            Some(code) if (200..400).contains(&code) => {
                log_line(&format!("健康检查通过 HTTP {code},端口 {port}"));
                return Ok(port);
            }
            _ => {
                if Instant::now() > deadline {
                    return Err("后端启动超时(120 秒未就绪),详情见日志".to_string());
                }
                std::thread::sleep(Duration::from_millis(400));
            }
        }
    }
}

// ---------- 主流程 ----------

fn main() {
    let state = Arc::new(ShellState::new());
    let state_in_setup = state.clone();
    let state_in_run = state.clone();

    let app = tauri::Builder::default()
        .setup(move |app| {
            let handle = app.handle().clone();

            // 日志落点:<用户数据目录>/paperring-shell.log
            if let Ok(dir) = handle.path().app_data_dir() {
                let _ = std::fs::create_dir_all(&dir);
                let _ = LOG_PATH.set(dir.join("paperring-shell.log"));
            }
            log_line("纸环桌面壳启动");

            let window = match app.get_webview_window("main") {
                Some(w) => w,
                None => {
                    log_line("致命错误:找不到主窗口");
                    return Ok(());
                }
            };

            // 后端拉起放独立线程,不阻塞窗口与启动页渲染
            let state = state_in_setup.clone();
            std::thread::spawn(move || {
                match boot_backend(&handle, &state) {
                    Ok(port) => {
                        if let Ok(url) =
                            tauri::Url::parse(&format!("http://127.0.0.1:{port}/"))
                        {
                            let _ = window.navigate(url);
                        }
                    }
                    Err(e) => {
                        log_line(&format!("后端启动失败:{e}"));
                        // 把启动页的状态文案换成失败提示,避免用户对着转圈干等
                        let _ = window.eval(
                            "document.getElementById('status').textContent = \
                             '启动失败,请完全退出后重试;反复失败请把 paperring-shell.log 发给支持人员';",
                        );
                    }
                }
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("纸环桌面壳构建失败");

    app.run(move |_handle, event| {
        // 应用退出(含关窗、系统注销等)时回收 java 子进程
        if let tauri::RunEvent::ExitRequested { .. } = event {
            kill_child(&state_in_run);
        }
    });
}
