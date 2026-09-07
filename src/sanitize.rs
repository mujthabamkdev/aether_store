/// Cheap, conservative HTML attribute/text escaper. Avoids pulling in a full
/// sanitizer for a single file's worth of sinks.
pub fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// Escape for JSON-in-HTML (e.g. inside <script> set via innerText? not needed).
/// We keep this module minimal: callers should use textContent / setAttribute.
#[allow(dead_code)]
pub fn json_esc(s: &str) -> String {
    esc(s)
}