//! 极简 WebDAV 客户端：PROPFIND 列目录 + GET 下载到暂存区。
//! 兼容 Alist / Apache / nginx dav / 群晖：multistatus XML 用宽松解析（各家的命名空间前缀不同）。
//! 隐私：密码只在内存与系统钥匙串之间流转，不落盘。

use regex::Regex;
use std::io::Write;
use std::path::Path;
use std::sync::OnceLock;

pub struct DavClient {
    /// 以 / 结尾的完整 base URL，如 https://dav.example.com/dav/
    base: String,
    username: String,
    password: String,
}

#[derive(Debug, Clone)]
pub struct DavEntry {
    /// 相对 base 的解码路径，如 "2023/IMG_1.jpg"（目录以 / 结尾）
    pub rel: String,
    pub is_dir: bool,
    pub size: Option<u64>,
    pub modified: Option<i64>,
}

impl DavClient {
    pub fn new(base: &str, username: &str, password: &str) -> Result<Self, String> {
        let base = base.trim().to_string();
        if !base.starts_with("http://") && !base.starts_with("https://") {
            return Err("地址必须以 http:// 或 https:// 开头".into());
        }
        Ok(Self {
            base: format!("{}/", base.trim_end_matches('/')),
            username: username.to_string(),
            password: password.to_string(),
        })
    }

    fn request(&self, method: &str, path: &str, depth: Option<&str>) -> Result<reqwest::blocking::Response, String> {
        let url = format!("{}{}", self.base, path);
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| e.to_string())?;
        let mut req = client.request(reqwest::Method::from_bytes(method.as_bytes()).map_err(|e| e.to_string())?, &url);
        if let Some(d) = depth {
            req = req.header("Depth", d);
        }
        req = req
            .basic_auth(&self.username, Some(&self.password))
            .header("Content-Type", "application/xml");
        if method == "PROPFIND" {
            req = req.body(PROPFIND_BODY);
        }
        let resp = req.send().map_err(|e| format!("连接失败: {e}"))?;
        Ok(resp)
    }

    pub fn test(&self) -> Result<(), String> {
        let resp = self.request("PROPFIND", "", Some("0"))?;
        if resp.status() == reqwest::StatusCode::MULTI_STATUS || resp.status().is_success() {
            Ok(())
        } else if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            Err("用户名或密码不对（401）".into())
        } else {
            Err(format!("服务器返回 {}", resp.status()))
        }
    }

    /// 列目录（Depth 1）。path 为相对 base 的路径，如 "" 或 "2023/"。
    pub fn list(&self, path: &str) -> Result<Vec<DavEntry>, String> {
        let resp = self.request("PROPFIND", path, Some("1"))?;
        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err("用户名或密码不对（401）".into());
        }
        if !resp.status().is_success() && resp.status().as_u16() != 207 {
            return Err(format!("列目录失败：服务器返回 {}", resp.status()));
        }
        let xml = resp.text().map_err(|e| e.to_string())?;
        Ok(parse_multistatus(&xml, &self.base))
    }

    /// 递归列目录（BFS），带目录数/文件数上限保护。
    pub fn list_recursive(
        &self,
        root: &str,
        progress: &dyn Fn(usize, usize, &str),
    ) -> Result<Vec<DavEntry>, String> {
        let mut files = Vec::new();
        let mut queue = vec![root.to_string()];
        let mut visited = 0usize;
        while let Some(dir) = queue.pop() {
            visited += 1;
            if visited > MAX_DIRS {
                return Err(format!("目录数超过上限 {MAX_DIRS}，请从更小的子目录导入"));
            }
            if files.len() > MAX_FILES {
                return Err(format!("文件数超过上限 {MAX_FILES}，请从更小的子目录导入"));
            }
            let entries = self.list(&dir)?;
            let prefix = dir.trim_end_matches('/');
            for e in entries {
                if e.is_dir {
                    let rel = e.rel.trim_end_matches('/');
                    if rel != prefix && !rel.is_empty() {
                        queue.push(format!("{rel}/"));
                    }
                } else {
                    files.push(e);
                }
            }
            progress(files.len(), visited, &dir);
        }
        Ok(files)
    }

    /// 下载文件到 dest（先写 .part 再改名），并恢复远端修改时间。
    pub fn download(&self, rel: &str, dest: &Path, modified: Option<i64>) -> Result<(), String> {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
        }
        let resp = self.request("GET", rel, None)?;
        if !resp.status().is_success() {
            return Err(format!("下载失败 {}: HTTP {}", rel, resp.status()));
        }
        let part = dest.with_extension("part");
        {
            let mut file = std::fs::File::create(&part).map_err(|e| format!("写文件失败: {e}"))?;
            let mut stream = resp;
            std::io::copy(&mut stream, &mut file).map_err(|e| format!("下载中断: {e}"))?;
            file.flush().ok();
        }
        std::fs::rename(&part, dest).map_err(|e| format!("落盘失败: {e}"))?;
        if let Some(secs) = modified {
            let t = filetime::FileTime::from_unix_time(secs, 0);
            let _ = filetime::set_file_mtime(dest, t);
            let _ = filetime::set_file_mtime(&part, t);
        }
        Ok(())
    }
}

const MAX_DIRS: usize = 20_000;
const MAX_FILES: usize = 200_000;
const PROPFIND_BODY: &str = r#"<?xml version="1.0"?><D:propfind xmlns:D="DAV:"><D:prop><D:resourcetype/><D:getcontentlength/><D:getlastmodified/></D:prop></D:propfind>"#;

/// 解析 207 multistatus。宽松处理命名空间前缀（D: / d: / 无前缀）。
pub fn parse_multistatus(xml: &str, base: &str) -> Vec<DavEntry> {
    static RESPONSE_RE: OnceLock<Regex> = OnceLock::new();
    static HREF_RE: OnceLock<Regex> = OnceLock::new();
    static SIZE_RE: OnceLock<Regex> = OnceLock::new();
    static MOD_RE: OnceLock<Regex> = OnceLock::new();
    let response_re = RESPONSE_RE.get_or_init(|| Regex::new(r"(?is)<(?:[\w-]+:)?response\b.*?</(?:[\w-]+:)?response>").unwrap());
    let href_re = HREF_RE.get_or_init(|| Regex::new(r"(?is)<(?:[\w-]+:)?href>\s*(.*?)\s*</(?:[\w-]+:)?href>").unwrap());
    let size_re = SIZE_RE.get_or_init(|| Regex::new(r"(?is)<(?:[\w-]+:)?getcontentlength>\s*(\d+)\s*<").unwrap());
    let mod_re = MOD_RE.get_or_init(|| Regex::new(r"(?is)<(?:[\w-]+:)?getlastmodified>\s*(.*?)\s*<").unwrap());

    let base_path = url_path(base).to_lowercase();
    let mut out = Vec::new();
    for block in response_re.find_iter(xml) {
        let b = block.as_str();
        let Some(href_raw) = href_re.captures(b).map(|c| c[1].to_string()) else {
            continue;
        };
        // href 可能是完整 URL 或绝对路径；统一取路径并解码
        let decoded = percent_decode(&href_raw);
        let path = if decoded.to_lowercase().starts_with("http") {
            url_path(&decoded)
        } else {
            decoded
        };
        let lower = path.to_lowercase();
        let rel = if lower.starts_with(&base_path) {
            path[base_path.len()..].trim_start_matches('/').to_string()
        } else {
            path.trim_start_matches('/').to_string()
        };
        if rel.is_empty() {
            continue; // 目录自身
        }
        let is_dir = b.to_lowercase().contains(":collection") || b.to_lowercase().contains("<collection");
        let size = size_re.captures(b).and_then(|c| c[1].parse().ok());
        let modified = mod_re
            .captures(b)
            .and_then(|c| parse_http_date(c[1].trim()));
        out.push(DavEntry { rel, is_dir, size, modified });
    }
    out
}

fn url_path(u: &str) -> String {
    let after_scheme = u.splitn(2, "://").nth(1).unwrap_or(u);
    let path_start = after_scheme.find('/').map(|i| &after_scheme[i..]).unwrap_or("/");
    path_start.trim_end_matches('/').to_string() + "/"
}

pub fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let hex = |b: u8| -> Option<u8> {
        match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            b'A'..=b'F' => Some(b - b'A' + 10),
            _ => None,
        }
    };
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn parse_http_date(s: &str) -> Option<i64> {
    let s = s.trim();
    // RFC1123（GMT 是 rfc2822 的废弃时区名，能被直接解析）
    if let Ok(d) = chrono::DateTime::parse_from_rfc2822(s) {
        return Some(d.timestamp());
    }
    // 兜底：无时区后缀按 UTC
    let t = chrono::NaiveDateTime::parse_from_str(s.trim_end_matches(" GMT"), "%a, %d %b %Y %H:%M:%S").ok()?;
    Some(t.and_utc().timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_alist_style() {
        let xml = r#"<?xml version="1.0"?><D:multistatus xmlns:D="DAV:">
<D:response><D:href>/dav/</D:href><D:propstat><D:prop><D:resourcetype><D:collection/></D:resourcetype></D:prop></D:propstat></D:response>
<D:response><D:href>/dav/2023/</D:href><D:propstat><D:prop><D:resourcetype><D:collection/></D:resourcetype><D:getlastmodified>Tue, 14 Nov 2023 09:12:00 GMT</D:getlastmodified></D:prop></D:propstat></D:response>
<D:response><D:href>/dav/IMG%201.jpg</D:href><D:propstat><D:prop><D:resourcetype/><D:getcontentlength>12345</D:getcontentlength><D:getlastmodified>Wed, 01 Mar 2023 08:00:00 GMT</D:getlastmodified></D:prop></D:propstat></D:response>
</D:multistatus>"#;
        let entries = parse_multistatus(xml, "https://x.com/dav/");
        assert_eq!(entries.len(), 2, "自身目录不计入");
        let dir = &entries[0];
        assert_eq!(dir.rel, "2023/");
        assert!(dir.is_dir);
        let file = &entries[1];
        assert_eq!(file.rel, "IMG 1.jpg");
        assert!(!file.is_dir);
        assert_eq!(file.size, Some(12345));
        assert_eq!(file.modified, Some(1677657600));
    }

    #[test]
    fn parse_full_url_href_and_no_prefix() {
        let xml = r#"<multistatus xmlns="DAV:">
<response><href>https://x.com/dav/sub/</href><propstat><prop><resourcetype><collection/></resourcetype></prop></propstat></response>
<response><href>/dav/sub/pic.png</href><propstat><prop><resourcetype/><getcontentlength>7</getcontentlength></prop></propstat></response>
</multistatus>"#;
        let entries = parse_multistatus(xml, "https://x.com/dav/");
        assert_eq!(entries.len(), 2, "子目录条目 + 文件条目");
        assert_eq!(entries[0].rel, "sub/");
        assert!(entries[0].is_dir);
        assert_eq!(entries[1].rel, "sub/pic.png");
        assert_eq!(entries[1].size, Some(7));
    }

    #[test]
    fn percent_decode_works() {
        assert_eq!(percent_decode("IMG%201.jpg"), "IMG 1.jpg");
        assert_eq!(percent_decode("%E4%B8%AD%E6%96%87.jpg"), "中文.jpg");
        assert_eq!(percent_decode("a%2.jpg"), "a%2.jpg"); // 非法序列原样
        assert_eq!(percent_decode("plain.jpg"), "plain.jpg");
    }

    #[test]
    fn http_date_formats() {
        assert_eq!(parse_http_date("Tue, 14 Nov 2023 09:12:00 GMT"), Some(1699953120));
        assert_eq!(parse_http_date("garbage"), None);
    }

}
