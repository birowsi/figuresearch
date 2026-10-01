//! A small, forgiving HTML parser.
//!
//! Store search pages are frequently malformed (unclosed `<li>`, stray end
//! tags, inline scripts). This parser never fails: it builds a best-effort
//! element tree that is good enough for locating product cards, links, images
//! and prices.

pub type NodeId = usize;

#[derive(Debug, Clone)]
pub enum NodeKind {
    Document,
    Element { tag: String, attrs: Vec<(String, String)> },
    Text(String),
}

#[derive(Debug, Clone)]
pub struct Node {
    pub kind: NodeKind,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
}

#[derive(Debug, Clone)]
pub struct Document {
    pub nodes: Vec<Node>,
}

const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];
const RAW_TEXT: &[&str] = &["script", "style", "textarea", "title", "noscript", "template"];
/// Tags that implicitly close an open sibling of the same kind.
const SELF_NESTING_FORBIDDEN: &[&str] = &["li", "p", "option", "tr", "td", "th", "dt", "dd"];

impl Document {
    pub fn parse(html: &str) -> Self {
        let mut doc = Document {
            nodes: vec![Node { kind: NodeKind::Document, parent: None, children: vec![] }],
        };
        let mut stack: Vec<NodeId> = vec![0];
        let bytes = html.as_bytes();
        let mut i = 0;
        let mut text_start = 0;

        while i < bytes.len() {
            if bytes[i] != b'<' {
                i += 1;
                continue;
            }
            // Comment
            if html[i..].starts_with("<!--") {
                doc.push_text(&stack, &html[text_start..i]);
                let end = html[i + 4..].find("-->").map(|p| i + 4 + p + 3).unwrap_or(bytes.len());
                i = end;
                text_start = i;
                continue;
            }
            // Doctype / processing instruction
            if html[i..].starts_with("<!") || html[i..].starts_with("<?") {
                doc.push_text(&stack, &html[text_start..i]);
                let end = html[i..].find('>').map(|p| i + p + 1).unwrap_or(bytes.len());
                i = end;
                text_start = i;
                continue;
            }
            let closing = bytes.get(i + 1) == Some(&b'/');
            let name_start = if closing { i + 2 } else { i + 1 };
            if !bytes.get(name_start).is_some_and(|b| b.is_ascii_alphabetic()) {
                i += 1; // a literal '<' in text
                continue;
            }
            doc.push_text(&stack, &html[text_start..i]);
            let (tag_end, tag, attrs, self_closing) = parse_tag(html, name_start);
            i = tag_end;
            text_start = i;

            if closing {
                if let Some(pos) = stack.iter().rposition(|&id| doc.tag(id) == Some(tag.as_str()))
                    && pos > 0 {
                        stack.truncate(pos);
                    }
                continue;
            }

            if SELF_NESTING_FORBIDDEN.contains(&tag.as_str()) {
                // Close an open element of the same tag if no list/table boundary sits in between.
                let boundary = ["ul", "ol", "table", "tbody", "thead", "select", "dl", "div"];
                if let Some(pos) = stack.iter().rposition(|&id| {
                    let t = doc.tag(id);
                    t == Some(tag.as_str()) || t.is_some_and(|t| boundary.contains(&t))
                })
                    && doc.tag(stack[pos]) == Some(tag.as_str()) && pos > 0 {
                        stack.truncate(pos);
                    }
            }

            let parent = *stack.last().unwrap();
            let id = doc.nodes.len();
            doc.nodes.push(Node {
                kind: NodeKind::Element { tag: tag.clone(), attrs },
                parent: Some(parent),
                children: vec![],
            });
            doc.nodes[parent].children.push(id);

            if RAW_TEXT.contains(&tag.as_str()) {
                let close = format!("</{tag}");
                let lower_rest = html[i..].to_ascii_lowercase();
                let end = lower_rest.find(&close).map(|p| i + p).unwrap_or(bytes.len());
                let raw = &html[i..end];
                if !raw.is_empty() {
                    let text_id = doc.nodes.len();
                    doc.nodes.push(Node {
                        kind: NodeKind::Text(raw.to_string()),
                        parent: Some(id),
                        children: vec![],
                    });
                    doc.nodes[id].children.push(text_id);
                }
                i = html[end..].find('>').map(|p| end + p + 1).unwrap_or(bytes.len());
                text_start = i;
                continue;
            }
            if !self_closing && !VOID.contains(&tag.as_str()) {
                stack.push(id);
            }
        }
        doc.push_text(&stack, &html[text_start.min(html.len())..]);
        doc
    }

    fn push_text(&mut self, stack: &[NodeId], raw: &str) {
        if raw.is_empty() {
            return;
        }
        let parent = *stack.last().unwrap();
        let id = self.nodes.len();
        self.nodes.push(Node { kind: NodeKind::Text(decode_entities(raw)), parent: Some(parent), children: vec![] });
        self.nodes[parent].children.push(id);
    }

    pub fn tag(&self, id: NodeId) -> Option<&str> {
        match &self.nodes[id].kind {
            NodeKind::Element { tag, .. } => Some(tag.as_str()),
            _ => None,
        }
    }

    pub fn attr(&self, id: NodeId, name: &str) -> Option<&str> {
        match &self.nodes[id].kind {
            NodeKind::Element { attrs, .. } => {
                attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
            }
            _ => None,
        }
    }

    pub fn classes(&self, id: NodeId) -> impl Iterator<Item = &str> {
        self.attr(id, "class").unwrap_or("").split_ascii_whitespace()
    }

    pub fn has_class_containing(&self, id: NodeId, needle: &str) -> bool {
        self.classes(id).any(|c| c.to_ascii_lowercase().contains(needle))
    }

    /// Hidden through a `displaynone`-style class or inline `display:none`.
    pub fn is_hidden(&self, id: NodeId) -> bool {
        if self.classes(id).any(|c| {
            let c = c.to_ascii_lowercase();
            c == "displaynone" || c == "hidden" || c == "blind" || c == "sr-only" || c == "screen_out"
        }) {
            return true;
        }
        self.attr(id, "style").is_some_and(|s| {
            let compact: String = s.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_ascii_lowercase();
            compact.contains("display:none")
        })
    }

    pub fn ancestors(&self, id: NodeId) -> Ancestors<'_> {
        Ancestors { doc: self, next: self.nodes[id].parent }
    }

    /// All descendants in document order (excluding `id` itself).
    pub fn descendants(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack: Vec<NodeId> = self.nodes[id].children.iter().rev().copied().collect();
        while let Some(n) = stack.pop() {
            out.push(n);
            stack.extend(self.nodes[n].children.iter().rev().copied());
        }
        out
    }

    pub fn elements(&self) -> impl Iterator<Item = NodeId> + '_ {
        (0..self.nodes.len()).filter(|&id| matches!(self.nodes[id].kind, NodeKind::Element { .. }))
    }

    /// Visible text with collapsed whitespace. Skips scripts, styles and hidden elements.
    pub fn text(&self, id: NodeId) -> String {
        let mut out = String::new();
        self.collect_text(id, &mut out, true);
        collapse_ws(&out)
    }

    /// Text including hidden elements (still skipping scripts/styles).
    pub fn text_all(&self, id: NodeId) -> String {
        let mut out = String::new();
        self.collect_text(id, &mut out, false);
        collapse_ws(&out)
    }

    fn collect_text(&self, id: NodeId, out: &mut String, skip_hidden: bool) {
        match &self.nodes[id].kind {
            NodeKind::Text(t) => {
                out.push_str(t);
                out.push(' ');
            }
            NodeKind::Element { tag, .. } => {
                if matches!(tag.as_str(), "script" | "style" | "noscript" | "template" | "title") {
                    return;
                }
                if skip_hidden && self.is_hidden(id) {
                    return;
                }
                if tag == "br" {
                    out.push(' ');
                }
                for &c in &self.nodes[id].children {
                    self.collect_text(c, out, skip_hidden);
                }
            }
            NodeKind::Document => {
                for &c in &self.nodes[id].children {
                    self.collect_text(c, out, skip_hidden);
                }
            }
        }
    }

    /// Raw text of script elements (for embedded JSON).
    pub fn scripts(&self) -> Vec<(Option<&str>, &str)> {
        self.elements()
            .filter(|&id| self.tag(id) == Some("script"))
            .filter_map(|id| {
                let child = *self.nodes[id].children.first()?;
                match &self.nodes[child].kind {
                    NodeKind::Text(t) => Some((self.attr(id, "type"), t.as_str())),
                    _ => None,
                }
            })
            .collect()
    }
}

pub struct Ancestors<'a> {
    doc: &'a Document,
    next: Option<NodeId>,
}

impl Iterator for Ancestors<'_> {
    type Item = NodeId;
    fn next(&mut self) -> Option<NodeId> {
        let current = self.next?;
        self.next = self.doc.nodes[current].parent;
        Some(current)
    }
}

/// Returns (index after '>', lowercase tag name, attributes, self-closing).
fn parse_tag(html: &str, name_start: usize) -> (usize, String, Vec<(String, String)>, bool) {
    let bytes = html.as_bytes();
    let mut i = name_start;
    while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'>' && bytes[i] != b'/' {
        i += 1;
    }
    let tag = html[name_start..i].to_ascii_lowercase();
    let mut attrs = Vec::new();
    let mut self_closing = false;
    loop {
        while i < bytes.len() && (bytes[i].is_ascii_whitespace()) {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        match bytes[i] {
            b'>' => {
                i += 1;
                break;
            }
            b'/' => {
                self_closing = bytes.get(i + 1) == Some(&b'>');
                i += 1;
                continue;
            }
            _ => {}
        }
        let key_start = i;
        while i < bytes.len() && !bytes[i].is_ascii_whitespace() && !matches!(bytes[i], b'=' | b'>') {
            if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'>') {
                break;
            }
            i += 1;
        }
        let key = html[key_start..i].to_ascii_lowercase();
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let mut value = String::new();
        if i < bytes.len() && bytes[i] == b'=' {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < bytes.len() && (bytes[i] == b'"' || bytes[i] == b'\'') {
                let quote = bytes[i];
                let start = i + 1;
                let end = html[start..].find(quote as char).map(|p| start + p).unwrap_or(bytes.len());
                value = decode_entities(&html[start..end]);
                i = (end + 1).min(bytes.len());
            } else {
                let start = i;
                while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'>' {
                    i += 1;
                }
                value = decode_entities(&html[start..i]);
            }
        }
        if key.is_empty() {
            i += 1;
            continue;
        }
        if !attrs.iter().any(|(k, _): &(String, String)| *k == key) {
            attrs.push((key, value));
        }
    }
    (i.min(bytes.len()), tag, attrs, self_closing)
}

pub fn collapse_ws(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn decode_entities(value: &str) -> String {
    if !value.contains('&') {
        return value.to_string();
    }
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(pos) = rest.find('&') {
        out.push_str(&rest[..pos]);
        rest = &rest[pos..];
        let end = rest[1..].find(|c: char| c == ';' || c == '&' || c.is_whitespace()).map(|p| p + 1);
        let Some(end) = end.filter(|&e| e <= 12 && rest.as_bytes().get(e) == Some(&b';')) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" | "#39" => Some('\''),
            "nbsp" => Some(' '),
            "middot" => Some('·'),
            "times" => Some('×'),
            "hellip" => Some('…'),
            "#8361" | "won" => Some('₩'),
            _ if entity.starts_with("#x") || entity.starts_with("#X") => {
                u32::from_str_radix(&entity[2..], 16).ok().and_then(char::from_u32)
            }
            _ if entity.starts_with('#') => entity[1..].parse::<u32>().ok().and_then(char::from_u32),
            _ => None,
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_malformed_lists_and_attributes() {
        let doc = Document::parse(
            "<ul class=list><li><a href='/a?x=1&amp;y=2'>One</a><li>Two<b>bold</ul><p>after</p>",
        );
        let lis: Vec<_> = doc.elements().filter(|&id| doc.tag(id) == Some("li")).collect();
        assert_eq!(lis.len(), 2);
        assert_eq!(doc.text(lis[1]), "Two bold");
        let a = doc.elements().find(|&id| doc.tag(id) == Some("a")).unwrap();
        assert_eq!(doc.attr(a, "href"), Some("/a?x=1&y=2"));
        let p = doc.elements().find(|&id| doc.tag(id) == Some("p")).unwrap();
        assert_eq!(doc.tag(doc.nodes[p].parent.unwrap()), None, "p must not be nested inside ul");
    }

    #[test]
    fn skips_scripts_and_hidden_text() {
        let doc = Document::parse(
            "<div><script>var a = '<li>x</li>';</script><span class='title displaynone'>상품명 :</span><span style='display: none'>no</span>보임 &#54620;&#xAE00;</div>",
        );
        assert_eq!(doc.text(0), "보임 한글");
        assert_eq!(doc.scripts().len(), 1);
    }

    #[test]
    fn tolerates_garbage() {
        let doc = Document::parse("<<a <div </span> & &unknown; <img src=x");
        assert!(doc.text(0).contains('&'));
    }
}
