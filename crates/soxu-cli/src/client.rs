//! `soxu-cli` 轻量级异步 HTTP/1.1 JSON 客户端（基于 `tokio::net::TcpStream`）。

use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// 与本地或局域网 `soxu-daemon` 通信的异步 HTTP 客户端。
#[derive(Debug, Clone)]
pub struct DaemonHttpClient {
    addr: String,
}

impl DaemonHttpClient {
    #[must_use]
    pub fn new(addr: impl Into<String>) -> Self {
        Self { addr: addr.into() }
    }

    pub async fn get_json<Resp: DeserializeOwned>(&self, path: &str) -> Result<Resp> {
        let req = format!(
            "GET {path} HTTP/1.1\r\nHost: {}\r\nAccept: application/json\r\nConnection: close\r\n\r\n",
            self.addr
        );
        self.send_and_decode(&req).await
    }

    pub async fn post_json<Req: Serialize, Resp: DeserializeOwned>(
        &self,
        path: &str,
        payload: &Req,
    ) -> Result<Resp> {
        let body = serde_json::to_string(payload).context("序列化请求体失败")?;
        let req = format!(
            "POST {path} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            self.addr,
            body.len(),
            body
        );
        self.send_and_decode(&req).await
    }

    async fn send_and_decode<Resp: DeserializeOwned>(&self, raw_request: &str) -> Result<Resp> {
        let mut stream = TcpStream::connect(&self.addr)
            .await
            .with_context(|| format!("无法连接 soxu-daemon ({})", self.addr))?;
        stream
            .write_all(raw_request.as_bytes())
            .await
            .context("发送 HTTP 请求失败")?;

        let mut buf = Vec::new();
        stream
            .read_to_end(&mut buf)
            .await
            .context("读取 HTTP 响应失败")?;
        let (status, body) = parse_http_response(&buf)?;
        if !(200..300).contains(&status) {
            bail!("守护进程返回非成功状态码 {status}: {body}");
        }
        serde_json::from_str(&body).context("解析守护进程 JSON 响应失败")
    }
}

/// 解析原始 HTTP/1.1 响应报文（兼容常规报文体与分块传输编码）。
pub fn parse_http_response(raw: &[u8]) -> Result<(u16, String)> {
    let text = std::str::from_utf8(raw).context("HTTP 响应非合法 UTF-8")?;
    let (head, body_raw) = text
        .split_once("\r\n\r\n")
        .context("HTTP 响应缺少头部与正文分隔符")?;
    let status_line = head.lines().next().context("HTTP 响应缺少状态行")?;
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .context("HTTP 状态行缺少状态码")?
        .parse()
        .context("解析 HTTP 状态码失败")?;

    let is_chunked = head.lines().any(|line| {
        line.to_ascii_lowercase().starts_with("transfer-encoding:")
            && line.to_ascii_lowercase().contains("chunked")
    });
    let body = if is_chunked {
        decode_chunked_body(body_raw)?
    } else {
        body_raw.to_owned()
    };

    Ok((status, body))
}

fn decode_chunked_body(mut rest: &str) -> Result<String> {
    let mut out = String::new();
    while !rest.is_empty() {
        let Some((size_line, after_size)) = rest.split_once("\r\n") else {
            break;
        };
        let size_hex = size_line.split(';').next().unwrap_or("").trim();
        let len = usize::from_str_radix(size_hex, 16).context("解析 chunked 长度失败")?;
        if len == 0 {
            break;
        }
        if after_size.len() < len {
            bail!("chunked 响应正文被截断");
        }
        let (chunk, tail) = after_size.split_at(len);
        out.push_str(chunk);
        rest = tail.strip_prefix("\r\n").unwrap_or(tail);
    }
    Ok(out)
}
