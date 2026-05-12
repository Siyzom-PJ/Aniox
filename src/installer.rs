//! Axiom-Nexus 自动化安装器
//!
//! Stage 1 (The Executor): 负责下载并静默安装 Node.js 和 Git，不做安装后检测
//! Stage 2 (The Validator): 负责刷新 PATH 并验收环境

use futures::StreamExt;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::process::Command;
use windows::Win32::UI::WindowsAndMessaging::{
    SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
};
use winreg::enums::*;
use winreg::RegKey;

const NODE_LTS_VERSION: &str = "v20.12.2";
const GIT_VERSION: &str = "v2.44.0.windows.1";
const MSI_NAME: &str = "node_setup.msi";
const GIT_EXE_NAME: &str = "Git-2.44.0-64-bit.exe";
const MIN_MSI_SIZE_BYTES: u64 = 20_000_000;
const MIN_GIT_EXE_SIZE_BYTES: u64 = 40_000_000;

// ---------------------------------------------------------------------------
// 旧版残留清理
// ---------------------------------------------------------------------------

fn clean_nodejs_registry() {
    println!("[清理] 正在扫描旧版 Node.js 注册表残留...");

    // HKLM\Uninstall
    clean_nodejs_in_path(HKEY_LOCAL_MACHINE, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall");
    // HKLM\Installer\UserData
    clean_nodejs_in_path(HKEY_LOCAL_MACHINE, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Installer\UserData\S-1-5-18\Products");
    // HKCU\Uninstall
    clean_nodejs_in_path(HKEY_CURRENT_USER, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall");

    println!("[清理] 完成");
}

fn clean_nodejs_in_path(hkey: winreg::HKEY, path: &str) {
    let hkey_root = RegKey::predef(hkey);
    if let Ok(key) = hkey_root.open_subkey(path) {
        for name in key.enum_keys().filter_map(|k| k.ok()) {
            if let Ok(subkey) = key.open_subkey(&name) {
                if let Ok(display) = subkey.get_value::<String, _>("DisplayName") {
                    if display.contains("Node.js") {
                        let code = subkey.get_value::<String, _>("ParentDisplayName")
                            .ok()
                            .map(|s| format!(" (parent: {})", s))
                            .unwrap_or_default();
                        println!("[清理] 发现残留: {}{}", display, code);

                        if let Ok(ps) = subkey.get_value::<String, _>("ProductCode") {
                            let _ = Command::new("msiexec")
                                .args(["/x", &ps, "/qn", "/norestart"])
                                .spawn();
                            println!("[清理] 已触发卸载: msiexec /x {} /qn", ps);
                        }
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 环境刷新（跨进程 PATH 生效）
// ---------------------------------------------------------------------------

/// 从注册表合并系统 Path 和用户 Path，写入当前进程内存
pub fn refresh_environment() {
    let system_path = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey("SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment")
        .and_then(|k| k.get_value::<String, _>("Path"))
        .unwrap_or_default();

    let user_path = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Environment")
        .and_then(|k| k.get_value::<String, _>("Path"))
        .unwrap_or_default();

    let merged = if user_path.is_empty() {
        system_path.clone()
    } else if system_path.is_empty() {
        user_path.clone()
    } else {
        format!("{};{}", system_path, user_path)
    };

    std::env::set_var("PATH", &merged);
    println!("   [PATH] 已刷新（系统: {} chars，用户: {} chars）", system_path.len(), user_path.len());
}

// ---------------------------------------------------------------------------
// 镜像映射
// ---------------------------------------------------------------------------

fn mirror_url_to_nodejs_base(mirror_url: &str) -> String {
    if mirror_url.contains("huaweicloud") {
        "https://mirrors.huaweicloud.com/nodejs".to_string()
    } else if mirror_url.contains("npmmirror") {
        "https://npmmirror.com/mirrors/node".to_string()
    } else if mirror_url.contains("cloud.tencent") {
        "https://mirrors.cloud.tencent.com/nodejs-release".to_string()
    } else {
        "https://npmmirror.com/mirrors/node".to_string()
    }
}

fn mirror_url_to_git_base(mirror_url: &str) -> String {
    if mirror_url.contains("huaweicloud") {
        "https://mirrors.huaweicloud.com/git-for-windows".to_string()
    } else {
        "https://npmmirror.com/mirrors/git-for-windows".to_string()
    }
}

fn build_nodejs_url(base: &str, version: &str) -> String {
    format!("{}/{}/node-{}-x64.msi", base, version, version)
}

fn build_git_url(base: &str, version: &str) -> String {
    format!("{}/{}/{}", base, version, GIT_EXE_NAME)
}

// ---------------------------------------------------------------------------
// 下载（每 5% 打印一次，格式：[下载进度] X% (Y.Z/Z.Z MB)...）
// ---------------------------------------------------------------------------

async fn download_file(
    url: &str,
    dest: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .build()?;

    let response = client.get(url).send().await?;

    if !response.status().is_success() {
        return Err(format!("下载请求失败，HTTP {}", response.status()).into());
    }

    let total_size = response.content_length().unwrap_or(0);
    if total_size == 0 {
        return Err("镜像服务器返回文件大小为 0，文件不存在".into());
    }

    let total_size_mb = total_size as f64 / 1024.0 / 1024.0;
    println!("\n[下载进度] 开始下载（预计 {:.1} MB）...", total_size_mb);

    let mut file = tokio::fs::File::create(dest).await?;
    let mut stream = response.bytes_stream();
    let mut downloaded: u64 = 0;
    let mut last_pct: i32 = -1;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        downloaded += chunk.len() as u64;
        tokio::io::copy(&mut chunk.as_ref(), &mut file).await?;

        let pct = ((downloaded as f64 / total_size as f64) * 100.0) as i32;
        if pct / 5 != last_pct / 5 {
            let downloaded_mb = downloaded as f64 / 1024.0 / 1024.0;
            println!("\r[下载进度] {:3.1}% ({:.1}/{:.1} MB)...", pct, downloaded_mb, total_size_mb);
            last_pct = pct;
        }
    }

    println!(); // 换行
    Ok(())
}

// ---------------------------------------------------------------------------
// Node.js 下载并安装（Stage 1 执行器）
// ---------------------------------------------------------------------------

/// 下载 Node.js MSI 并执行安装（/quiet 模式，真正静默无弹窗）
/// 仅检查安装进程退出码，不做安装后 node -v 检测
pub async fn install_node_executor(mirror_url: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    clean_nodejs_registry();

    let base = mirror_url_to_nodejs_base(mirror_url);
    let url = build_nodejs_url(&base, NODE_LTS_VERSION);
    println!("\n🌐 Node.js 下载源：{}", url);

    let temp_dir = std::env::temp_dir();
    let msi_path = temp_dir.join(MSI_NAME);

    download_file(&url, &msi_path).await?;

    let metadata = tokio::fs::metadata(&msi_path).await?;
    if metadata.len() < MIN_MSI_SIZE_BYTES {
        let _ = tokio::fs::remove_file(&msi_path).await;
        return Err(format!(
            "下载文件异常：{:.1} MB（预期 >{:.1} MB）",
            metadata.len() as f64 / 1024.0 / 1024.0,
            MIN_MSI_SIZE_BYTES as f64 / 1024.0 / 1024.0
        ).into());
    }

    println!("\n🔧 正在安装 Node.js {} ...", NODE_LTS_VERSION);
    let status = Command::new("msiexec")
        .args(["/i", msi_path.to_str().unwrap(), "/quiet", "/norestart"])
        .spawn()?
        .wait()?;

    let _ = tokio::fs::remove_file(&msi_path).await;

    if !status.success() {
        let code = status.code().unwrap_or(-1);
        return Err(format!("Node.js 安装失败（状态码 {}）", code).into());
    }

    println!("✅ Node.js 安装程序执行成功！");
    Ok(())
}

// ---------------------------------------------------------------------------
// Git 下载并安装（Stage 1 执行器，全自动静默）
// ---------------------------------------------------------------------------

/// 检测系统是否已安装 Git（Stage 1 预检用）
pub fn is_git_installed() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// 下载 Git for Windows 并执行安装（/VERYSILENT 模式，全自动静默）
/// 仅检查安装进程退出码，不做安装后 git --version 检测
pub async fn install_git_executor(mirror_url: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if is_git_installed() {
        println!("✅ Git 已安装，跳过");
        return Ok(());
    }

    let base = mirror_url_to_git_base(mirror_url);
    let url = build_git_url(&base, GIT_VERSION);
    println!("\n🌐 Git 下载源：{}", url);

    let temp_dir = std::env::temp_dir();
    let git_exe_path = temp_dir.join(GIT_EXE_NAME);

    download_file(&url, &git_exe_path).await?;

    let metadata = tokio::fs::metadata(&git_exe_path).await?;
    if metadata.len() < MIN_GIT_EXE_SIZE_BYTES {
        let _ = tokio::fs::remove_file(&git_exe_path).await;
        return Err(format!(
            "下载文件异常：{:.1} MB（预期 >{:.1} MB）",
            metadata.len() as f64 / 1024.0 / 1024.0,
            MIN_GIT_EXE_SIZE_BYTES as f64 / 1024.0 / 1024.0
        ).into());
    }

    println!("\n🔧 正在安装 Git {} ...", GIT_VERSION);
    let status = Command::new(&git_exe_path)
        .args(["/VERYSILENT", "/NORESTART"])
        .spawn()?
        .wait()?;

    let _ = tokio::fs::remove_file(&git_exe_path).await;

    if !status.success() {
        let code = status.code().unwrap_or(-1);
        return Err(format!("Git 安装失败（状态码 {}）", code).into());
    }

    println!("✅ Git 安装程序执行成功！");
    Ok(())
}

// ---------------------------------------------------------------------------
// 环境验收（Stage 2 验证器用）
// ---------------------------------------------------------------------------

/// 检查 Node.js 是否可用（Stage 2 验收用）
pub fn validate_node() -> Option<String> {
    Command::new("node")
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

/// 检查 Git 是否可用（Stage 2 验收用）
pub fn validate_git() -> Option<String> {
    Command::new("git")
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

// ---------------------------------------------------------------------------
// NPM PATH 硬注入
// ---------------------------------------------------------------------------

/// 将 NPM 全局路径注入到 HKEY_CURRENT_USER\Environment Path（去重 + 分号拼接）
pub fn inject_npm_path_to_registry(npm_path: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let env_key = hkcu.open_subkey_with_flags("Environment", KEY_READ | KEY_WRITE)?;

    let current_path: String = env_key.get_value("Path").unwrap_or_default();

    // 去重逻辑：检查是否已包含该路径
    let paths: Vec<&str> = current_path.split(';').filter(|p| !p.is_empty()).collect();
    let normalized_npm = npm_path.trim_end_matches('\\').trim_end_matches('/');

    let already_exists = paths.iter().any(|p| {
        let normalized = p.trim_end_matches('\\').trim_end_matches('/');
        normalized.eq_ignore_ascii_case(normalized_npm)
    });

    let new_path = if already_exists {
        current_path.clone()
    } else {
        if current_path.is_empty() {
            npm_path.to_string()
        } else {
            format!("{};{}", current_path, npm_path)
        }
    };

    env_key.set_value("Path", &new_path)?;
    println!("   [PATH] 注入完成（总计 {} 个路径）", new_path.split(';').filter(|p| !p.is_empty()).count());

    Ok(())
}

/// 广播 WM_SETTINGCHANGE 消息以刷新环境变量
pub fn broadcast_environment_change() {
    let env_ptr: Vec<u16> = std::ffi::OsStr::new("Environment")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let _ = SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            windows::Win32::Foundation::WPARAM(0),
            windows::Win32::Foundation::LPARAM(env_ptr.as_ptr() as isize),
            SMTO_ABORTIFHUNG,
            5000,
            None,
        );
    }
    println!("   [广播] WM_SETTINGCHANGE 已发送");
}
