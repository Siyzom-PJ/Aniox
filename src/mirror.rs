//! NPM 镜像源测速模块
//!
//! 并发探测多个镜像源，返回响应最快的节点

use futures::future;
use reqwest::Client;
use std::time::Duration;

const MIRRORS: &[&str] = &[
    "https://registry.npmmirror.com",
    "https://mirrors.cloud.tencent.com/npm/",
    "https://mirrors.huaweicloud.com/repository/npm/",
];

const TIMEOUT_SECS: u64 = 3;

const FALLBACK: &str = "https://registry.npmmirror.com";

async fn check_mirror(url: &str) -> Result<String, ()> {
    let client = Client::builder()
        .timeout(Duration::from_secs(TIMEOUT_SECS))
        .build()
        .map_err(|_| ())?;

    client
        .head(url)
        .send()
        .await
        .map_err(|_| ())
        .and_then(|resp| {
            if resp.status().is_success() {
                Ok(url.to_string())
            } else {
                Err(())
            }
        })
}

pub async fn get_fastest_mirror() -> String {
    let futures: Vec<_> = MIRRORS
        .iter()
        .map(|url| Box::pin(check_mirror(url)))
        .collect();

    match future::select_ok(futures).await {
        Ok((result, _)) => result,
        Err(_) => FALLBACK.to_string(),
    }
}
