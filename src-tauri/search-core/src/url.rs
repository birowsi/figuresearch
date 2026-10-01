//! Minimal URL helpers (no external crates).

pub fn scheme(url: &str) -> &str {
    url.split_once("://").map(|(s, _)| s).unwrap_or("https")
}

/// `https://host:port`
pub fn origin(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else { return String::new() };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    format!("{scheme}://{authority}")
}

/// Lowercase host without port and without a leading `www.`/`m.`.
pub fn host(url: &str) -> String {
    let Some((_, rest)) = url.split_once("://") else { return String::new() };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = authority.rsplit_once('@').map(|(_, h)| h).unwrap_or(authority);
    let host = host.split(':').next().unwrap_or("").to_ascii_lowercase();
    host.strip_prefix("www.").or_else(|| host.strip_prefix("m.")).unwrap_or(&host).to_string()
}

pub fn path(url: &str) -> &str {
    let Some((_, rest)) = url.split_once("://") else { return "/" };
    let start = rest.find('/').unwrap_or(rest.len());
    let rest = &rest[start..];
    let end = rest.find(['?', '#']).unwrap_or(rest.len());
    if end == 0 { "/" } else { &rest[..end] }
}

pub fn query_param<'a>(url: &'a str, key: &str) -> Option<&'a str> {
    let query = url.split_once('?')?.1;
    let query = query.split('#').next().unwrap_or(query);
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        k.eq_ignore_ascii_case(key).then_some(v)
    })
}

fn normalize_path(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    let trailing = path.ends_with('/') || path.ends_with("/.") || path.ends_with("/..");
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    let mut out = format!("/{}", parts.join("/"));
    if trailing && out != "/" {
        out.push('/');
    }
    out
}

/// Resolves `href` against `base`. Returns None for non-navigational links.
pub fn resolve(base: &str, href: &str) -> Option<String> {
    let href = href.trim();
    let lower = href.to_ascii_lowercase();
    if href.is_empty() || href.starts_with('#') || ["javascript:", "mailto:", "tel:", "data:", "about:"].iter().any(|p| lower.starts_with(p)) {
        return None;
    }
    let href = href.split('#').next().unwrap_or(href);
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return Some(href.to_string());
    }
    if href.starts_with("//") {
        return Some(format!("{}:{href}", scheme(base)));
    }
    let base_origin = origin(base);
    if base_origin.is_empty() {
        return None;
    }
    let (href_path, href_query) = match href.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (href, None),
    };
    let joined = if href_path.is_empty() {
        path(base).to_string()
    } else if href_path.starts_with('/') {
        normalize_path(href_path)
    } else {
        let base_path = path(base);
        let dir = &base_path[..base_path.rfind('/').map(|i| i + 1).unwrap_or(0)];
        normalize_path(&format!("{dir}{href_path}"))
    };
    Some(match href_query {
        Some(q) => format!("{base_origin}{joined}?{q}"),
        None => format!("{base_origin}{joined}"),
    })
}

pub fn percent_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(bytes.len() * 3);
    for &b in bytes {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0f) as usize] as char);
        }
    }
    out
}

/// Fills `{input}` in a site URL template with an already percent-encoded query.
pub fn fill_template(template: &str, encoded_query: &str) -> Result<String, String> {
    if template.matches("{input}").count() != 1 {
        return Err("URL에 {input} 자리표시자가 정확히 하나 있어야 해요".into());
    }
    Ok(template.replace("{input}", encoded_query))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_relative_links() {
        let base = "https://www.shop.test/goods/goods_search.php?keyword=x";
        assert_eq!(resolve(base, "../goods/goods_view.php?goodsNo=1").unwrap(), "https://www.shop.test/goods/goods_view.php?goodsNo=1");
        assert_eq!(resolve(base, "/product/a/12/").unwrap(), "https://www.shop.test/product/a/12/");
        assert_eq!(resolve(base, "//cdn.test/x.jpg").unwrap(), "https://cdn.test/x.jpg");
        assert_eq!(resolve(base, "?page=2").unwrap(), "https://www.shop.test/goods/goods_search.php?page=2");
        assert_eq!(resolve(base, "view.php?no=3#top").unwrap(), "https://www.shop.test/goods/view.php?no=3");
        assert_eq!(resolve(base, "javascript:void(0)"), None);
        assert_eq!(host("https://WWW.Shop.test:8080/a"), "shop.test");
        assert_eq!(query_param("https://a.b/c?GoodsNo=12&x=1", "goodsno"), Some("12"));
    }

    #[test]
    fn encodes_queries() {
        assert_eq!(percent_encode("미쿠 1/7".as_bytes()), "%EB%AF%B8%EC%BF%A0%201%2F7");
        assert_eq!(fill_template("https://a.b/s?q={input}&x=1", "abc").unwrap(), "https://a.b/s?q=abc&x=1");
        assert!(fill_template("https://a.b/s", "abc").is_err());
    }
}
