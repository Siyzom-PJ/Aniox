mod installer;
mod mirror;
mod privilege;

use std::env;
use std::io;
use std::process::Command;
use std::thread;

const STAGE2_ARG: &str = "--stage-2";
const STAGE3_ARG: &str = "--stage-3";
const STAGE4_ARG: &str = "--stage-4";

fn breath() {
    let ms = 300 + (std::time::Instant::now().elapsed().subsec_nanos() % 500);
    thread::sleep(std::time::Duration::from_millis(ms as u64));
}

fn is_stage2() -> bool {
    env::args().any(|arg| arg == STAGE2_ARG)
}

fn is_stage3() -> bool {
    env::args().any(|arg| arg == STAGE3_ARG)
}

fn is_stage4() -> bool {
    env::args().any(|arg| arg == STAGE4_ARG)
}

#[tokio::main]
async fn main() {
    if is_stage3() {
        stage3().await;
    } else if is_stage2() {
        stage2().await;
    } else if is_stage4() {
        stage4().await;
    } else {
        stage1().await;
    }
}

// ---------------------------------------------------------------------------
// Stage 3 — 纯净验收（The Final Validator）
// ---------------------------------------------------------------------------

async fn stage3() {
    println!("========================================");
    println!("  [Stage 3] Axiom-Nexus 终极验收");
    println!("========================================\n");

    installer::refresh_environment();

    println!("🔍 正在验证 claude 命令...");
    let claude_ver = Command::new("cmd")
        .args(["/C", "claude", "--version"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

    if let Some(ver) = claude_ver {
        println!("\n🎉 部署全线竣工！Claude-Code 已通过 NPM 挂载到全局环境！");
        println!("   版本信息：{}", ver);
        breath();

        // 写入 .claude.json 跳过新手引导
        let user_profile = env::var("USERPROFILE").unwrap_or_default();
        let claude_json_path = std::path::PathBuf::from(&user_profile).join(".claude.json");
        let json_content = r#"{"hasCompletedOnboarding": true}"#;
        if let Err(e) = std::fs::write(&claude_json_path, json_content) {
            eprintln!("\n⚠️  配置文件写入失败：{}", e);
        }

        breath();

        // 启动 Stage 4
        let exe_path = env::current_exe().expect("无法获取程序路径");
        let _ = Command::new("cmd")
            .args(["/C", "start", "", exe_path.to_str().unwrap(), STAGE4_ARG])
            .spawn();

        std::process::exit(0);
    } else {
        println!("\n❌ claude 命令验证失败");
        breath();
    }

    println!("\n按回车键关闭此窗口...");
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
}

// ---------------------------------------------------------------------------
// Stage 2 — NPM 镜像部署与硬注入（The Installer）
// ---------------------------------------------------------------------------

async fn stage2() {
    println!("========================================");
    println!("  [Stage 2] NPM 镜像部署与硬注入");
    println!("========================================\n");

    installer::refresh_environment();

    // 获取 %APPDATA%\npm 路径
    let npm_global_path = std::path::PathBuf::from(env::var("APPDATA").unwrap_or_default())
        .join("npm");
    let npm_global_path_str = npm_global_path.to_string_lossy().to_string();

    // Spinner 提示
    println!("⠋ [Axiom] 正在通过国内高速镜像源部署 Claude-Code...");

    // 执行 npm 全局安装
    let install_result = Command::new("cmd")
        .args([
            "/C",
            "npm",
            "install",
            "-g",
            "@anthropic-ai/claude-code",
            "--registry=https://mirrors.huaweicloud.com/repository/npm",
        ])
        .env("PATH", format!("{};{}", npm_global_path_str, env::var("PATH").unwrap_or_default()))
        .output();

    match install_result {
        Ok(output) if output.status.success() => {
            println!("\r✅ NPM 安装完成！                    ");
            breath();
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!("\n❌ NPM 安装失败：{}", stderr);
            breath();
        }
        Err(e) => {
            eprintln!("\n❌ NPM 执行失败：{}", e);
            breath();
        }
    }

    // 硬编码路径注入：写入注册表 HKEY_CURRENT_USER\Environment 的 Path（去重 + 分号拼接）
    println!("\n🔧 正在注入 PATH...");
    if let Err(e) = installer::inject_npm_path_to_registry(&npm_global_path_str) {
        eprintln!("\n❌ PATH 注入失败：{}", e);
    } else {
        println!("   ✅ PATH 已写入注册表");
        breath();
    }

    // 系统广播刷新环境变量
    println!("\n🔄 正在广播环境变量变更...");
    installer::broadcast_environment_change();
    breath();

    // 休眠 2 秒
    std::thread::sleep(std::time::Duration::from_secs(2));

    // 启动 Stage 3
    let exe_path = env::current_exe().expect("无法获取程序路径");
    println!("\n🚀 正在启动 Stage 3 验收...\n");
    std::thread::sleep(std::time::Duration::from_secs(1));

    match Command::new("cmd")
        .args(["/C", "start", "", exe_path.to_str().unwrap(), STAGE3_ARG])
        .spawn()
    {
        Ok(_) => {}
        Err(e) => {
            eprintln!("❌ 无法启动 Stage 3：{}", e);
        }
    }

    std::process::exit(0);
}

// ---------------------------------------------------------------------------
// Stage 4 — 本地化模型配置指南（The Final Guide）
// ---------------------------------------------------------------------------

async fn stage4() {
    println!();
    println!("===========================================");
    println!("  [Stage 4] 环境变量配置");
    println!("===========================================");
    println!();
    println!("请在 CMD 窗口中执行以下命令：");
    println!();
    println!(">>> setx ANTHROPIC_API_KEY \"YOUR_DASHSCOPE_API_KEY\"");
    println!(">>> setx ANTHROPIC_BASE_URL \"https://xxx\"");
    println!(">>> setx ANTHROPIC_MODEL xxx");
    println!();
    println!("按回车键退出...");
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
}

// ---------------------------------------------------------------------------
// Stage 1 — 自动化执行（The Executor）
// ---------------------------------------------------------------------------

async fn stage1() {
    privilege::init();

    println!("========================================");
    println!("  Axiom-Nexus v0.1.0  (elevated: {})", privilege::is_elevated());
    println!("========================================\n");

    // 1. 测速
    println!("[Mirror Racer] 正在并发探测国内高速节点...");
    let fastest_url = mirror::get_fastest_mirror().await;
    println!("✅ 测速完成！最优镜像：{}\n", fastest_url);
    breath();

    // 2. Node.js 预检 + 下载安装（/passive，展现实时进度条但无人值守）
    let node_exists = Command::new("node").arg("--version").output().map(|o| o.status.success()).unwrap_or(false);
    if node_exists {
        println!("[Stage 1] Node.js 已安装，跳过");
        breath();
    } else {
        println!("[Stage 1] 开始安装 Node.js...");
        match installer::install_node_executor(&fastest_url).await {
            Ok(_) => {
                breath();
            }
            Err(e) => {
                eprintln!("❌ Node.js 安装失败：{}", e);
                breath();
            }
        }
    }

    // 3. Git 下载安装（/SILENT，全自动静默）
    println!("\n[Stage 1] 开始安装 Git...");
    match installer::install_git_executor(&fastest_url).await {
        Ok(_) => {
            breath();
        }
        Err(e) => {
            eprintln!("❌ Git 安装失败：{}", e);
            breath();
        }
    }

    // 4. 阶段一完成：接力重启
    println!("\n🎉 基础环境已就绪。");
    breath();
    println!("🚀 3秒后将为您开启新窗口以应用配置...\n");

    std::thread::sleep(std::time::Duration::from_secs(3));

    let exe_path = env::current_exe().expect("无法获取程序路径");
    match Command::new("cmd")
        .args(["/C", "start", "", exe_path.to_str().unwrap(), STAGE2_ARG])
        .spawn()
    {
        Ok(_) => println!("   ✅ 新窗口已启动，移交至 Stage 2"),
        Err(e) => {
            eprintln!("   ❌ 无法启动新窗口：{}", e);
            eprintln!("   请手动运行：{} --stage-2", exe_path.display());
        }
    }

    std::process::exit(0);
}
