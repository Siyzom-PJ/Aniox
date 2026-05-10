//! 权限管理模块 - 权限自提升逻辑
//!
//! 检测当前进程是否以管理员权限运行，若否则弹出 UAC 请求提升权限。

use std::env;
use std::process;
use windows::core::PCWSTR;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

/// 检查当前进程是否以管理员权限运行
pub fn is_elevated() -> bool {
    is_elevated::is_elevated()
}

/// 以管理员权限重新启动当前程序
/// 将原始命令行参数传递给新进程
pub fn elevate() -> ! {
    let exe_path = env::current_exe().expect("Failed to get current executable path");
    let args: Vec<String> = env::args().collect();

    // 构建命令行参数（跳过第一个，即程序自身路径）
    let command_args: Vec<&str> = args.iter().skip(1).map(|s| s.as_str()).collect();
    let command_line = command_args.join(" ");

    // 将路径转换为 Windows 宽字符字符串
    let exe_wide: Vec<u16> = exe_path
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let cmd_wide: Vec<u16> = command_line
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    // runas 动词请求 UAC 提升权限
    unsafe {
        ShellExecuteW(
            HWND::default(),
            PCWSTR::from_raw("runas\0".encode_utf16().collect::<Vec<u16>>().as_ptr()),
            PCWSTR::from_raw(exe_wide.as_ptr()),
            PCWSTR::from_raw(cmd_wide.as_ptr()),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
    }

    // 立即退出当前未授权进程
    process::exit(0);
}

/// 初始化权限管理
/// 如果当前进程没有管理员权限，则请求提升
pub fn init() {
    if !is_elevated() {
        elevate();
    }
}
