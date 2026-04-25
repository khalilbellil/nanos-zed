//! HTML → Markdown transformer + docs-URL fixer for the bundled nanos world
//! EmmyLua docstrings.
//!
//! ## HTML → Markdown
//!
//! The upstream annotations file embeds HTML directly inside `---` Lua doc
//! comments (e.g. `<img src="…"> <b>[Client/Server Side]</b> <a href="…">docs</a>`).
//! VS Code's hover renderer interprets that HTML, but Zed's renderer prints
//! it literally. We rewrite the HTML to plain Markdown so hovers look right
//! without touching the upstream file (which the sync workflow keeps faithful
//! to `docgen-output`).
//!
//! Supported tags (case-sensitive; that's all that appears in upstream):
//!
//! - `<b>` / `<strong>`            → `**bold**`
//! - `<i>` / `<em>`                → `*italic*`
//! - `<code>`                      → `` `code` ``
//! - `<br>`, `<br/>`, `<br />`,
//!   `</br>` (upstream typo)       → split the comment line, continuing the
//!                                   `---` block on the next physical line
//! - `<p>`, `</p>`                 → soft paragraph break (same mechanism)
//! - `<ul>` / `<ol>` / `</ul>` /
//!   `</ol>`                       → soft break (markdown auto-detects lists)
//! - `<li>X</li>`                  → newline + `- X`
//! - `<img src="…">`               → an emoji badge for the four known
//!                                   nanos world side-indicator icons (see
//!                                   [`IMAGE_REPLACEMENTS`]); falls back to
//!                                   `![](…)` for any other image. Editor
//!                                   hovers in Zed don't actually render
//!                                   embedded images, so substituting an
//!                                   emoji is the only way to keep the
//!                                   side-indicator visible.
//! - `<a href="…">text</a>`        → `[text](…)` with the URL run through
//!                                   [`AnnotationsContext::fix_doc_url`] and
//!                                   `text` recursively transformed
//!
//! ## Docs-URL fixer
//!
//! Upstream's docgen produces a couple of categories of broken URLs:
//!
//! 1. Multi-word entity classes (`SceneCapture`, `CharacterSimple`,
//!    `InstancedStaticMesh`, …) are emitted as `/classes/<lowercased
//!    concatenated>` but the docs site uses kebab-case
//!    (`/classes/scene-capture`).
//! 2. A handful of slugs that upstream files under `/static-classes/` are
//!    actually under different categories on the docs site
//!    (`nanosmath` → `/utility-libraries/nanosmath`,
//!    `vector` → `/structs/vector`, …).
//!
//! [`AnnotationsContext`] handles both. It scans the input for
//! `---@class Foo` declarations to build the kebab-case slug map (so new
//! upstream classes are picked up automatically without code changes), and
//! consults a small hand-curated `CATEGORY_REMAP` table for the second case.

use std::collections::HashMap;

const COMMENT_PREFIX: &str = "---";

/// Sentinel used inside a single transformed comment line to mark "split the
/// `---` block here". We can't use a real `\n` because that would break out
/// of the Lua doc comment, so we collect splits per-line and emit a fresh
/// `---` continuation on each one in [`transform_annotations`].
const LINE_BREAK_SENTINEL: char = '\u{1}';

/// The base URL relative `/...` href / src attributes are resolved against.
const DOC_BASE_URL: &str = "https://docs.nanos-world.com";

/// Marker used to find docs URLs we own (vs. arbitrary external links).
const DOCS_HOST: &str = "docs.nanos-world.com";

/// Path prefix for the scripting reference. Everything after this is the
/// `<category>/<…>/<slug>` we want to inspect / rewrite.
const DOCS_API_PREFIX: &str = "/docs/scripting-reference/";

/// Categories under `DOCS_API_PREFIX` whose final slug should be kebab-cased
/// from the original PascalCase class name. We deliberately *don't* touch
/// `/static-classes/` because the docs site keeps those concatenated
/// (`/static-classes/postprocess`, `/static-classes/http`, …).
const KEBAB_CATEGORIES: &[&str] = &["classes", "next/scripting-reference/classes"];

/// Map known nanos world badge image URLs to emoji that render in Zed's
/// hover popover. Match is performed by URL *suffix* so we tolerate
/// variations like `raw.github.com` vs `raw.githubusercontent.com` and any
/// future repository / branch reorganisation upstream might do.
///
/// Order matches the docs site's own emoji vocabulary: 🟧 = Client side,
/// 🟦 = Server side, 👑 = Authority side (the side currently controlling
/// the entity), 🛰️ = Network Authority (the side that *spawned* the
/// entity, i.e. the network-replicating one).
const IMAGE_REPLACEMENTS: &[(&str, &str)] = &[
    ("/assets/both.png", "🟧🟦"),
    ("/assets/client-only.png", "🟧"),
    ("/assets/authority-only.png", "👑"),
    ("/assets/network-authority.png", "🛰️"),
];

/// Static-classes URLs in the upstream annotations file that the actual docs
/// site has moved/categorised differently. The slug stays the same — only
/// the leading category needs swapping. Both keys (slugs) and values
/// (categories) are lowercase, matching what we read out of the URL.
const CATEGORY_REMAP: &[(&str, &str)] = &[
    // Lua-ish helpers: live under /utility-libraries/ on the docs site.
    ("nanosmath", "utility-libraries"),
    ("nanostable", "utility-libraries"),
    ("nanosutils", "utility-libraries"),
    ("json", "utility-libraries"),
    ("toml", "utility-libraries"),
    // Math/utility value types: live under /structs/ on the docs site.
    ("vector", "structs"),
    ("vector2d", "structs"),
    ("color", "structs"),
    ("rotator", "structs"),
    ("quat", "structs"),
    ("matrix", "structs"),
];

/// Per-file context derived from the annotations text: the set of class
/// names declared in it, prepared for URL-slug rewrites. Built once per
/// `transform_annotations` call and threaded through the recursive HTML
/// walker.
pub struct AnnotationsContext {
    /// Map from `name.lowercase()` to the kebab-cased slug expected by
    /// `/classes/<slug>` docs URLs. Only contains entries where the two
    /// differ (i.e. the class name has a casing boundary worth splitting).
    class_slug_kebab: HashMap<String, String>,
}

impl AnnotationsContext {
    /// Scan `input` for `---@class Foo : Bar` declarations and build a
    /// PascalCase → kebab-case slug map for use by [`Self::fix_doc_url`].
    pub fn from_annotations(input: &str) -> Self {
        let mut class_slug_kebab = HashMap::new();
        for line in input.lines() {
            let Some(rest) = line.strip_prefix("---@class ") else {
                continue;
            };
            let name = rest
                .split(|c: char| c.is_whitespace() || c == ':')
                .next()
                .unwrap_or("");
            if name.is_empty() {
                continue;
            }
            let compact = name.to_ascii_lowercase();
            let kebab = pascal_to_kebab(name);
            if compact != kebab {
                class_slug_kebab.insert(compact, kebab);
            }
        }
        Self { class_slug_kebab }
    }

    /// Rewrite a docs URL to point at the correct page. Leaves non-docs
    /// URLs and URLs we don't recognise untouched.
    pub fn fix_doc_url(&self, url: &str) -> String {
        if !url.contains(DOCS_HOST) {
            return url.to_string();
        }
        let Some(api_idx) = url.find(DOCS_API_PREFIX) else {
            return url.to_string();
        };
        let api_start = api_idx + DOCS_API_PREFIX.len();
        let (head, tail) = url.split_at(api_start);

        let (path, suffix) = match tail.find(|c: char| c == '#' || c == '?') {
            Some(i) => (&tail[..i], &tail[i..]),
            None => (tail, ""),
        };

        let mut segments: Vec<String> = path.split('/').map(|s| s.to_string()).collect();
        if segments.len() < 2 {
            return url.to_string();
        }
        let last_idx = segments.len() - 1;
        if segments[last_idx].is_empty() {
            return url.to_string();
        }

        let slug = segments[last_idx].clone();
        let category = segments[0].clone();

        // Category remap fires on slug match alone (e.g. /static-classes/color
        // → /structs/color). Slug stays as-is.
        if let Some(new_category) = CATEGORY_REMAP
            .iter()
            .find_map(|(s, c)| (*s == slug.as_str()).then_some(*c))
        {
            segments[0] = new_category.to_string();
            return format!("{}{}{}", head, segments.join("/"), suffix);
        }

        // Kebab the slug for `/classes/...` URLs when we know the canonical
        // kebab form from a `---@class` declaration we scanned.
        if KEBAB_CATEGORIES.contains(&category.as_str()) {
            if let Some(kebab) = self.class_slug_kebab.get(&slug) {
                segments[last_idx] = kebab.clone();
                return format!("{}{}{}", head, segments.join("/"), suffix);
            }
        }

        url.to_string()
    }
}

/// Convert a PascalCase identifier to the kebab-case slug used by the nanos
/// world docs site.
///
/// The rule we mirror is: insert `-` at every transition from a lowercase
/// letter to either an uppercase letter or a digit. Digit-to-letter
/// transitions do NOT split (so `Text3D` becomes `text-3d`, not `text-3-d`,
/// matching `/classes/text-3d` on the live docs site).
fn pascal_to_kebab(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    let chars: Vec<char> = s.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if i > 0 {
            let prev = chars[i - 1];
            if prev.is_ascii_lowercase() && (c.is_ascii_uppercase() || c.is_ascii_digit()) {
                out.push('-');
            }
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

/// Transform every Lua doc comment (`---…`) line in `input` from HTML to
/// Markdown, leaving non-comment lines untouched. Preserves the original
/// CRLF/LF line endings. Also rewrites any nanos world docs URLs found in
/// `<a href="…">` tags so they point at pages that actually exist (see
/// [`AnnotationsContext::fix_doc_url`]).
pub fn transform_annotations(input: &str) -> String {
    let ctx = AnnotationsContext::from_annotations(input);
    let mut out = String::with_capacity(input.len() + 1024);

    let mut iter = input.split('\n').peekable();
    while let Some(line) = iter.next() {
        let (body, cr) = match line.strip_suffix('\r') {
            Some(b) => (b, "\r"),
            None => (line, ""),
        };

        if let Some(rest) = body.strip_prefix(COMMENT_PREFIX) {
            let transformed = transform_inline_tags(rest, &ctx);
            for (i, piece) in transformed.split(LINE_BREAK_SENTINEL).enumerate() {
                if i > 0 {
                    out.push_str(cr);
                    out.push('\n');
                }
                out.push_str(COMMENT_PREFIX);
                out.push_str(piece);
            }
            out.push_str(cr);
        } else {
            out.push_str(body);
            out.push_str(cr);
        }

        if iter.peek().is_some() {
            out.push('\n');
        }
    }

    out
}

/// Apply the tag → Markdown rewrites described in the module docs to a
/// single-line slice. `<br>`-style tags are encoded as
/// [`LINE_BREAK_SENTINEL`] so the caller can split the result into multiple
/// `---` lines. `ctx` is used to fix nanos world docs URLs in the process.
fn transform_inline_tags(input: &str, ctx: &AnnotationsContext) -> String {
    let mut out = String::with_capacity(input.len());
    let mut remaining = input;

    while let Some(idx) = remaining.find('<') {
        out.push_str(&remaining[..idx]);
        remaining = &remaining[idx..];

        if let Some(rest) = strip_any(remaining, &["<b>", "<strong>"]) {
            out.push_str("**");
            remaining = rest;
            continue;
        }
        if let Some(rest) = strip_any(remaining, &["</b>", "</strong>"]) {
            out.push_str("**");
            remaining = rest;
            continue;
        }
        if let Some(rest) = strip_any(remaining, &["<i>", "<em>"]) {
            out.push('*');
            remaining = rest;
            continue;
        }
        if let Some(rest) = strip_any(remaining, &["</i>", "</em>"]) {
            out.push('*');
            remaining = rest;
            continue;
        }
        if let Some(rest) = remaining.strip_prefix("<code>") {
            out.push('`');
            remaining = rest;
            continue;
        }
        if let Some(rest) = remaining.strip_prefix("</code>") {
            out.push('`');
            remaining = rest;
            continue;
        }

        if let Some(rest) = match_br(remaining) {
            out.push(LINE_BREAK_SENTINEL);
            remaining = rest;
            continue;
        }

        if let Some(rest) = remaining.strip_prefix("<li>") {
            out.push(LINE_BREAK_SENTINEL);
            out.push_str("- ");
            remaining = rest;
            continue;
        }
        if let Some(rest) = remaining.strip_prefix("</li>") {
            remaining = rest;
            continue;
        }

        if let Some(rest) = strip_any(remaining, &["<ul>", "<ol>", "</ul>", "</ol>", "<p>", "</p>"])
        {
            out.push(LINE_BREAK_SENTINEL);
            remaining = rest;
            continue;
        }

        if let Some((url, after)) = match_self_closing_tag(remaining, "img", "src") {
            if let Some(emoji) = replace_known_image(&url) {
                out.push_str(emoji);
            } else {
                out.push_str("![](");
                out.push_str(&process_url(&url, ctx));
                out.push(')');
            }
            remaining = after;
            continue;
        }

        if let Some((url, text, after)) = match_anchor(remaining, ctx) {
            out.push('[');
            out.push_str(&text);
            out.push_str("](");
            out.push_str(&process_url(&url, ctx));
            out.push(')');
            remaining = after;
            continue;
        }

        out.push('<');
        remaining = &remaining[1..];
    }

    out.push_str(remaining);
    out
}

fn strip_any<'a>(s: &'a str, prefixes: &[&str]) -> Option<&'a str> {
    prefixes.iter().find_map(|p| s.strip_prefix(*p))
}

/// Look up an `<img src>` URL in [`IMAGE_REPLACEMENTS`] and return the
/// corresponding emoji if it matches one of the four known nanos world
/// side-indicator badges. Match is by URL suffix (filename + parent dir)
/// rather than full URL so it survives upstream host/branch renames.
fn replace_known_image(url: &str) -> Option<&'static str> {
    IMAGE_REPLACEMENTS
        .iter()
        .find_map(|(suffix, emoji)| url.ends_with(suffix).then_some(*emoji))
}

/// Match `<br>`, `<br/>`, `<br />` (any internal whitespace) or the upstream
/// typo `</br>`. Returns the slice after the matched tag.
fn match_br(s: &str) -> Option<&str> {
    if let Some(rest) = s.strip_prefix("<br") {
        let end = rest.find('>')?;
        if rest[..end].chars().all(|c| c.is_whitespace() || c == '/') {
            return Some(&rest[end + 1..]);
        }
    }
    s.strip_prefix("</br>")
}

/// Match a self-closing tag like `<img src="..." height="21">` or
/// `<img src='/x' />`. Requires the character following the tag name to be
/// whitespace, `>`, or `/` so that e.g. `<image>` doesn't get mistaken for
/// an `<img>` tag.
fn match_self_closing_tag<'a>(s: &'a str, tag: &str, attr: &str) -> Option<(String, &'a str)> {
    let prefix = format!("<{tag}");
    let rest = s.strip_prefix(&prefix)?;
    let next = rest.chars().next()?;
    if !next.is_whitespace() && next != '>' && next != '/' {
        return None;
    }
    let end = rest.find('>')?;
    let url = extract_attr(&rest[..end], attr)?;
    Some((url, &rest[end + 1..]))
}

/// Match `<a href="…">…</a>`. The inner text is recursively transformed so
/// nested `<b>`, `<code>`, etc. inside link labels render as Markdown too.
fn match_anchor<'a>(s: &'a str, ctx: &AnnotationsContext) -> Option<(String, String, &'a str)> {
    let rest = s.strip_prefix("<a")?;
    let next = rest.chars().next()?;
    if !next.is_whitespace() && next != '>' {
        return None;
    }
    let open_end = rest.find('>')?;
    let attrs = &rest[..open_end];
    let after_open = &rest[open_end + 1..];
    let close_idx = after_open.find("</a>")?;
    let inner = &after_open[..close_idx];
    let url = extract_attr(attrs, "href").unwrap_or_default();
    Some((
        url,
        transform_inline_tags(inner, ctx),
        &after_open[close_idx + "</a>".len()..],
    ))
}

/// Extract the value of `name="..."` or `name='...'` from a tag's attribute
/// substring. Returns `None` if the attribute isn't present or its value
/// isn't terminated.
fn extract_attr(attrs: &str, name: &str) -> Option<String> {
    for quote in ['"', '\''] {
        let needle = format!("{name}={quote}");
        if let Some(start) = attrs.find(&needle) {
            let val_start = start + needle.len();
            if let Some(end_off) = attrs[val_start..].find(quote) {
                return Some(attrs[val_start..val_start + end_off].to_string());
            }
        }
    }
    None
}

/// Run a URL through the absolutiser and the docs-URL fixer.
fn process_url(url: &str, ctx: &AnnotationsContext) -> String {
    let absolute = absolutize_url(url);
    ctx.fix_doc_url(&absolute)
}

/// Resolve `/`-rooted relative URLs against the docs site so Markdown links
/// stay clickable from the editor.
fn absolutize_url(url: &str) -> String {
    if url.starts_with('/') && !url.starts_with("//") {
        format!("{DOC_BASE_URL}{url}")
    } else {
        url.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sample `---@class` declarations covering every shape exercised by the
    /// URL-fix tests below.
    const SAMPLE_CLASSES: &str = "\
---@class Actor : Entity
Actor = {}
---@class SceneCapture : Entity, Actor
SceneCapture = {}
---@class CharacterSimple : Entity
CharacterSimple = {}
---@class InstancedStaticMesh : Entity, Actor, Paintable
InstancedStaticMesh = {}
---@class Text3D : Entity, Actor, Paintable
Text3D = {}
---@class Widget3D : Entity, Actor
Widget3D = {}
---@class WebUI : Entity
WebUI = {}
---@class VehicleWheeled : Entity, Actor, Paintable, Damageable, Vehicle
VehicleWheeled = {}
---@class NanosMath
NanosMath = {}
---@class JSON
JSON = {}
---@class Color
Color = {}
---@class Vector
Vector = {}
---@class Vector2D
Vector2D = {}
---@class HTTP
HTTP = {}
---@class PostProcess
PostProcess = {}
";

    #[test]
    fn rewrites_real_upstream_header() {
        let input = "---<img src=\"https://raw.github.com/nanos-world/vscode-extension/master/assets/both.png\" height=\"21\"> <b>[Client/Server Side]</b>\n---<a href=\"https://docs.nanos-world.com/docs/scripting-reference/classes/actor\">docs</a>";
        let expected = "---\u{1F7E7}\u{1F7E6} **[Client/Server Side]**\n---[docs](https://docs.nanos-world.com/docs/scripting-reference/classes/actor)";
        assert_eq!(transform_annotations(input), expected);
    }

    #[test]
    fn known_badge_images_become_emoji() {
        let cases = [
            ("https://raw.github.com/nanos-world/vscode-extension/master/assets/both.png", "\u{1F7E7}\u{1F7E6}"),
            ("https://raw.github.com/nanos-world/vscode-extension/master/assets/client-only.png", "\u{1F7E7}"),
            ("https://raw.github.com/nanos-world/vscode-extension/master/assets/authority-only.png", "\u{1F451}"),
            ("https://raw.github.com/nanos-world/vscode-extension/master/assets/network-authority.png", "\u{1F6F0}\u{FE0F}"),
        ];
        for (url, emoji) in cases {
            let input = format!("---<img src=\"{url}\" height=\"21\"> hello");
            let expected = format!("---{emoji} hello");
            assert_eq!(transform_annotations(&input), expected, "case: {url}");
        }
    }

    #[test]
    fn badge_match_tolerates_alternate_host() {
        // `raw.githubusercontent.com` should match the same suffix as
        // `raw.github.com` — we identify badges by the trailing
        // `/assets/<name>.png` segment rather than the full URL.
        let input = "---<img src=\"https://raw.githubusercontent.com/nanos-world/vscode-extension/master/assets/client-only.png\"> x";
        let expected = "---\u{1F7E7} x";
        assert_eq!(transform_annotations(input), expected);
    }

    #[test]
    fn unknown_image_still_falls_back_to_markdown() {
        // Anything that isn't one of the four known badges should still
        // emit `![](url)` so future upstream additions remain visible (as
        // a hyperlink) rather than being silently dropped.
        let input = "---<img src=\"https://example.com/foo.png\"> y";
        let expected = "---![](https://example.com/foo.png) y";
        assert_eq!(transform_annotations(input), expected);
    }

    #[test]
    fn br_splits_into_multiple_comment_lines() {
        let input = "---An <b>Actor</b> is...<br>Actors support 3D<br><br>An <b>Actor</b>";
        let expected = "---An **Actor** is...\n---Actors support 3D\n---\n---An **Actor**";
        assert_eq!(transform_annotations(input), expected);
    }

    #[test]
    fn handles_self_closing_br_and_typo_close() {
        let input = "---a<br/>b<br />c</br>d";
        let expected = "---a\n---b\n---c\n---d";
        assert_eq!(transform_annotations(input), expected);
    }

    #[test]
    fn rewrites_strong_em_and_code() {
        let input = "---Use <code>SetForce()</code> with <strong>care</strong> and <em>style</em>";
        let expected = "---Use `SetForce()` with **care** and *style*";
        assert_eq!(transform_annotations(input), expected);
    }

    #[test]
    fn relative_anchor_is_absolutized() {
        let input = "---see <a href=\"/docs/core-concepts/scripting/authority-concepts\">Network Authority</a>";
        let expected = "---see [Network Authority](https://docs.nanos-world.com/docs/core-concepts/scripting/authority-concepts)";
        assert_eq!(transform_annotations(input), expected);
    }

    #[test]
    fn img_supports_single_quotes_and_self_close() {
        let input = "---x <img src='/img/docs/anchors.webp' /> y";
        let expected = "---x ![](https://docs.nanos-world.com/img/docs/anchors.webp) y";
        assert_eq!(transform_annotations(input), expected);
    }

    #[test]
    fn list_items_become_dash_lines() {
        let input = "---intro<ul><li>one</li><li>two</li></ul>outro";
        let expected = "---intro\n---\n---- one\n---- two\n---outro";
        assert_eq!(transform_annotations(input), expected);
    }

    #[test]
    fn anchor_with_inner_bold_recurses() {
        let input = "---<a href=\"/x\"><b>label</b></a>";
        let expected = "---[**label**](https://docs.nanos-world.com/x)";
        assert_eq!(transform_annotations(input), expected);
    }

    #[test]
    fn lua_code_lines_are_left_alone() {
        let input =
            "function Foo()\n    -- regular comment\n    print(\"<b>not transformed</b>\")\nend";
        assert_eq!(transform_annotations(input), input);
    }

    #[test]
    fn unknown_tags_pass_through() {
        let input = "---hello <unknown attr=\"x\">world</unknown>";
        assert_eq!(
            transform_annotations(input),
            "---hello <unknown attr=\"x\">world</unknown>"
        );
    }

    #[test]
    fn preserves_crlf_line_endings() {
        let input = "---<b>a</b>\r\n---<b>b</b>\r\n";
        let expected = "---**a**\r\n---**b**\r\n";
        assert_eq!(transform_annotations(input), expected);
    }

    #[test]
    fn does_not_swallow_image_word() {
        let input = "---<image src=\"x\">y";
        assert_eq!(transform_annotations(input), "---<image src=\"x\">y");
    }

    // --- pascal_to_kebab unit tests ---------------------------------

    #[test]
    fn pascal_to_kebab_handles_typical_names() {
        assert_eq!(pascal_to_kebab("Actor"), "actor");
        assert_eq!(pascal_to_kebab("SceneCapture"), "scene-capture");
        assert_eq!(pascal_to_kebab("CharacterSimple"), "character-simple");
        assert_eq!(
            pascal_to_kebab("InstancedStaticMesh"),
            "instanced-static-mesh"
        );
        assert_eq!(pascal_to_kebab("WebUI"), "web-ui");
        assert_eq!(pascal_to_kebab("HTTP"), "http");
    }

    #[test]
    fn pascal_to_kebab_treats_digit_after_letter_as_split_but_not_after_digit() {
        // Matches the live docs convention: `/classes/text-3d` and
        // `/classes/widget-3d`, NOT `text-3-d` / `widget-3-d`.
        assert_eq!(pascal_to_kebab("Text3D"), "text-3d");
        assert_eq!(pascal_to_kebab("Widget3D"), "widget-3d");
        assert_eq!(pascal_to_kebab("Vector2D"), "vector-2d");
    }

    // --- AnnotationsContext::fix_doc_url tests ---------------------

    #[test]
    fn fix_doc_url_kebabs_classes_slug() {
        let ctx = AnnotationsContext::from_annotations(SAMPLE_CLASSES);
        assert_eq!(
            ctx.fix_doc_url("https://docs.nanos-world.com/docs/scripting-reference/classes/scenecapture"),
            "https://docs.nanos-world.com/docs/scripting-reference/classes/scene-capture"
        );
        assert_eq!(
            ctx.fix_doc_url("https://docs.nanos-world.com/docs/scripting-reference/classes/charactersimple"),
            "https://docs.nanos-world.com/docs/scripting-reference/classes/character-simple"
        );
        assert_eq!(
            ctx.fix_doc_url("https://docs.nanos-world.com/docs/scripting-reference/classes/instancedstaticmesh"),
            "https://docs.nanos-world.com/docs/scripting-reference/classes/instanced-static-mesh"
        );
        assert_eq!(
            ctx.fix_doc_url("https://docs.nanos-world.com/docs/scripting-reference/classes/webui"),
            "https://docs.nanos-world.com/docs/scripting-reference/classes/web-ui"
        );
    }

    #[test]
    fn fix_doc_url_preserves_anchor_when_kebabbing() {
        let ctx = AnnotationsContext::from_annotations(SAMPLE_CLASSES);
        assert_eq!(
            ctx.fix_doc_url("https://docs.nanos-world.com/docs/scripting-reference/classes/scenecapture#function-getfov"),
            "https://docs.nanos-world.com/docs/scripting-reference/classes/scene-capture#function-getfov"
        );
    }

    #[test]
    fn fix_doc_url_kebabs_text3d_and_widget3d() {
        let ctx = AnnotationsContext::from_annotations(SAMPLE_CLASSES);
        assert_eq!(
            ctx.fix_doc_url("https://docs.nanos-world.com/docs/scripting-reference/classes/text3d"),
            "https://docs.nanos-world.com/docs/scripting-reference/classes/text-3d"
        );
        assert_eq!(
            ctx.fix_doc_url("https://docs.nanos-world.com/docs/scripting-reference/classes/widget3d"),
            "https://docs.nanos-world.com/docs/scripting-reference/classes/widget-3d"
        );
    }

    #[test]
    fn fix_doc_url_remaps_static_to_utility_libraries() {
        let ctx = AnnotationsContext::from_annotations(SAMPLE_CLASSES);
        assert_eq!(
            ctx.fix_doc_url("https://docs.nanos-world.com/docs/scripting-reference/static-classes/nanosmath#static-function-clamp"),
            "https://docs.nanos-world.com/docs/scripting-reference/utility-libraries/nanosmath#static-function-clamp"
        );
        assert_eq!(
            ctx.fix_doc_url("https://docs.nanos-world.com/docs/scripting-reference/static-classes/json"),
            "https://docs.nanos-world.com/docs/scripting-reference/utility-libraries/json"
        );
    }

    #[test]
    fn fix_doc_url_remaps_static_to_structs() {
        let ctx = AnnotationsContext::from_annotations(SAMPLE_CLASSES);
        assert_eq!(
            ctx.fix_doc_url("https://docs.nanos-world.com/docs/scripting-reference/static-classes/color#static-function-fromhex"),
            "https://docs.nanos-world.com/docs/scripting-reference/structs/color#static-function-fromhex"
        );
        assert_eq!(
            ctx.fix_doc_url("https://docs.nanos-world.com/docs/scripting-reference/static-classes/vector2d"),
            "https://docs.nanos-world.com/docs/scripting-reference/structs/vector2d"
        );
    }

    #[test]
    fn fix_doc_url_leaves_correct_static_classes_alone() {
        let ctx = AnnotationsContext::from_annotations(SAMPLE_CLASSES);
        let unchanged = [
            "https://docs.nanos-world.com/docs/scripting-reference/static-classes/postprocess",
            "https://docs.nanos-world.com/docs/scripting-reference/static-classes/http",
            "https://docs.nanos-world.com/docs/scripting-reference/static-classes/events#static-function-subscriberemote",
            "https://docs.nanos-world.com/docs/scripting-reference/classes/actor",
            "https://docs.nanos-world.com/docs/scripting-reference/classes/base-classes/actor#function-addangularimpulse",
        ];
        for url in unchanged {
            assert_eq!(ctx.fix_doc_url(url), url, "should not rewrite: {url}");
        }
    }

    #[test]
    fn fix_doc_url_ignores_non_docs_urls() {
        let ctx = AnnotationsContext::from_annotations(SAMPLE_CLASSES);
        let unchanged = [
            "https://raw.github.com/nanos-world/vscode-extension/master/assets/both.png",
            "https://example.com/scripting-reference/classes/scenecapture",
            "https://docs.nanos-world.com/random/page",
        ];
        for url in unchanged {
            assert_eq!(ctx.fix_doc_url(url), url, "should not rewrite: {url}");
        }
    }

    #[test]
    fn transform_annotations_fixes_anchor_url_end_to_end() {
        let input = "\
---@class SceneCapture : Entity, Actor
SceneCapture = {}
---<a href=\"https://docs.nanos-world.com/docs/scripting-reference/classes/scenecapture\">docs</a>
function SceneCapture() end
";
        let expected = "\
---@class SceneCapture : Entity, Actor
SceneCapture = {}
---[docs](https://docs.nanos-world.com/docs/scripting-reference/classes/scene-capture)
function SceneCapture() end
";
        assert_eq!(transform_annotations(input), expected);
    }
}
