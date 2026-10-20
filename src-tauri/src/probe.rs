use reqwest::{header::{ACCEPT_ENCODING, CONTENT_RANGE, RANGE}, Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeResult {
    pub resume: String,
    pub single_connection_limit: String,
    pub note: String,
}

impl ProbeResult {
    pub fn checking() -> Self {
        Self { resume: "checking".into(), single_connection_limit: "checking".into(), note: String::new() }
    }

    fn unknown(note: &str) -> Self {
        Self { resume: "unknown".into(), single_connection_limit: "unknown".into(), note: note.into() }
    }
}

async fn sample(client: &Client, url: &str, start: u64, size: u64) -> Option<Duration> {
    let end = start + size - 1;
    let began = Instant::now();
    let mut response = client.get(url)
        .header(ACCEPT_ENCODING, "identity")
        .header(RANGE, format!("bytes={start}-{end}"))
        .send().await.ok()?;
    if response.status() != StatusCode::PARTIAL_CONTENT { return None; }
    let expected = format!("bytes {start}-{end}/");
    if !response.headers().get(CONTENT_RANGE)?.to_str().ok()?.starts_with(&expected) { return None; }
    let mut received = 0usize;
    while let Some(chunk) = response.chunk().await.ok()? {
        received += chunk.len();
        if received > size as usize { return None; }
    }
    (received == size as usize).then(|| began.elapsed())
}

pub async fn inspect(url: &str, proxy_url: &str) -> ProbeResult {
    let mut builder = Client::builder().timeout(Duration::from_secs(12));
    if !proxy_url.is_empty() {
        let Ok(proxy) = reqwest::Proxy::all(proxy_url) else { return ProbeResult::unknown("代理地址无效，无法检测") };
        builder = builder.proxy(proxy);
    } else {
        builder = builder.no_proxy();
    }
    let Ok(client) = builder.build() else { return ProbeResult::unknown("无法创建检测请求") };
    let response = client.get(url).header(ACCEPT_ENCODING, "identity").header(RANGE, "bytes=0-0").send().await;
    let Ok(response) = response else { return ProbeResult::unknown("检测请求失败；可能需要登录、特殊请求头或网络连接") };
    if response.status() == StatusCode::OK {
        return ProbeResult { resume: "unsupported".into(), single_connection_limit: "unknown".into(), note: "服务器忽略 Range 请求，无法确认分段和断点续传".into() };
    }
    if response.status() != StatusCode::PARTIAL_CONTENT {
        return ProbeResult::unknown("服务器未返回可验证的分段响应");
    }
    let total = response.headers().get(CONTENT_RANGE).and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("bytes 0-0/"))
        .and_then(|v| v.parse::<u64>().ok());
    let Some(total) = total else { return ProbeResult::unknown("分段响应缺少有效的文件总长度") };
    if total < 2 * 1024 * 1024 {
        return ProbeResult { resume: "supported".into(), single_connection_limit: "unknown".into(), note: "文件较小，未进行单连接限速抽样".into() };
    }
    const SIZE: u64 = 512 * 1024;
    let Some(single) = sample(&client, url, 0, SIZE).await else {
        return ProbeResult { resume: "supported".into(), single_connection_limit: "unknown".into(), note: "区间请求可用，但速度抽样未完成".into() };
    };
    let first = tauri::async_runtime::spawn({ let c = client.clone(); let u = url.to_owned(); async move { sample(&c, &u, SIZE, SIZE).await } });
    let second = tauri::async_runtime::spawn({ let c = client.clone(); let u = url.to_owned(); async move { sample(&c, &u, SIZE * 2, SIZE).await } });
    let began = Instant::now();
    let (left, right) = (first.await.ok().flatten(), second.await.ok().flatten());
    if left.is_none() || right.is_none() {
        return ProbeResult { resume: "supported".into(), single_connection_limit: "unknown".into(), note: "并行速度抽样未完成".into() };
    }
    let parallel = began.elapsed();
    let ratio = 2.0 * single.as_secs_f64() / parallel.as_secs_f64().max(0.001);
    let (limit, note) = if single < Duration::from_millis(700) {
        ("unknown", "单连接响应过快，无法可靠判断是否限速")
    } else if ratio >= 1.5 {
        ("suspected", "两条连接的合计速度明显高于单连接，疑似单连接限速")
    } else {
        ("not_observed", "本次抽样未发现明显的单连接限速")
    };
    ProbeResult { resume: "supported".into(), single_connection_limit: limit.into(), note: note.into() }
}
