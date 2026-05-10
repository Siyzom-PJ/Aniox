mod privilege;
use std::io;
fn main() {
    // 在程序启动时检查并请求管理员权限
    privilege::init();

    println!(
        "Axiom-Nexus started (elevated: {})",
        privilege::is_elevated()
    );
    println!("\n[测试阶段] 请按回车键关闭此窗口...");
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    // ... 后续程序逻辑
}
