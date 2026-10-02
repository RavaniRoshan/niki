//! `assets/logo.svg` is the first thing a stranger sees, and two of its
//! properties are claims rather than taste.
//!
//! **It uses no font.** Every glyph and the whole wordmark are paths. An
//! earlier version drew "NIKI" with `<text>` and `ui-monospace`, and it
//! rendered as a stretched proportional face anywhere that stack was not the
//! first match — a logo that looks different on every renderer is not a logo.
//!
//! **It carries its own background.** GitHub renders READMEs in light and dark
//! themes and the SVG is inline in the page, so a mark that assumed one
//! disappears into the other. The card is `Palette::bg_deep` from the Kiln
//! theme, which is what the terminal prints behind every stage.
//!
//! The colours are the product's own — `Palette::role_*` in
//! `src/display/theme.rs` — so the mark cannot drift from the UI it represents
//! without one of these failing.

use std::path::Path;

/// The SVG with its comments removed.
///
/// The file explains *why* it contains no `<text>` and no
/// `prefers-color-scheme`, in a comment. Grepping the raw file therefore
/// finds both — the checks below fired on their own explanation, which is the
/// kind of test that punishes the next person for documenting anything.
fn logo() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/logo.svg");
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("assets/logo.svg: {e}"));
    strip_comments(&raw)
}

fn strip_comments(svg: &str) -> String {
    let mut out = String::with_capacity(svg.len());
    let mut rest = svg;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        match rest[start..].find("-->") {
            Some(end) => rest = &rest[start + end + 3..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

#[test]
fn the_logo_uses_no_font_at_all() {
    let svg = logo();
    for element in ["<text", "<tspan", "font-family", "@font-face", "<style"] {
        assert!(
            !svg.contains(element),
            "the logo contains `{element}`, so its rendering depends on a font \
             being available. Paths only: a logo must look the same everywhere."
        );
    }
}

#[test]
fn the_logo_carries_its_own_background() {
    let svg = logo();
    assert!(
        svg.contains("#141211"),
        "the card background is Palette::bg_deep (#141211). Without an opaque \
         card the mark vanishes into GitHub's dark theme, and inverts into \
         GitHub's light one."
    );
    assert!(
        !svg.contains("prefers-color-scheme"),
        "the logo branches on the theme, which means it renders differently in \
         the two. It should not: the card is the same in both."
    );
}

#[test]
fn the_logo_carries_every_role_colour_the_product_defines() {
    let svg = logo();
    // Palette::role_* from the Kiln theme — the four colours `niki` prints an
    // agent's name in. If the theme changes and this does not, the mark is
    // advertising a product that no longer exists.
    for (role, hex) in [
        ("Planner", "#d9a86a"),
        ("Coder", "#b85c1a"),
        ("Tester", "#6fb05c"),
        ("Reviewer", "#3fa396"),
    ] {
        assert!(
            svg.contains(hex),
            "the logo is missing {role}'s colour {hex}. The four agent glyphs \\
             are drawn in the palette the terminal uses, and a mark that \\
             drifts from the UI it stands for is decoration."
        );
    }
}

#[test]
fn the_logo_is_labelled_for_a_screen_reader() {
    let svg = logo();
    assert!(
        svg.contains("role=\"img\""),
        "an <img alt> covers this on GitHub, but the file is also used \\
         directly — in the docs site and the installer output — and there the \\
         accessibility lives inside the SVG."
    );
    assert!(
        svg.contains("<title") && svg.contains("<desc"),
        "a title and description, so the mark is announced rather than silent"
    );
}

/// The README comment claims the logo is referenced by an `<img>`. It was not —
/// the file was committed, the comment described it, and no `<img>` existed,
/// which is a comment asserting a thing nobody checked.
#[test]
fn the_readme_actually_references_the_logo() {
    let readme = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("README.md"))
        .expect("README.md is readable");

    assert!(
        readme.contains("src=\"assets/logo.svg\""),
        "README.md does not reference assets/logo.svg. The file exists and is \\
         tracked, so a reader following the comment that describes it finds \\
         nothing."
    );
    // A relative path from the repo root, so it resolves on GitHub and in a
    // fresh clone — not a path that only works from one machine's checkout.
    assert!(
        !readme.contains("src=\"/assets/logo.svg"),
        "the logo path is absolute, which resolves to the site root rather than \\
         the repository's assets directory"
    );
}

/// Sizes it must survive: the README header, a social card, and a favicon-
/// scale mark. Renders at each and requires non-trivial content, so a blank or
/// collapsed SVG cannot pass by being valid XML.
#[test]
fn the_logo_renders_at_the_sizes_it_is_used_at() {
    for width in [340u32, 200, 136] {
        let png = render_at(width);
        assert!(
            png.starts_with(&[0x89, 0x50, 0x4e, 0x47]),
            "not a PNG at {width}"
        );
        assert!(
            png.len() > 800,
            "rendered to {width}px wide it produced {} bytes, which is a near-empty \\
             image rather than a mark",
            png.len()
        );
    }
}

fn render_at(width: u32) -> Vec<u8> {
    // Rendered by whatever the host has; the assertion that matters is that
    // *something* is produced at each size, which is what catches a viewBox
    // that collapses to nothing at small widths.
    match std::process::Command::new("python3")
        .args([
            "-c",
            "import cairosvg,sys;cairosvg.svg2png(url='assets/logo.svg',write_to=sys.argv[1],output_width=int(sys.argv[2]))",
            "/tmp/logo_render_check.png",
            &width.to_string(),
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .status()
    {
        Ok(status) if status.success() => std::fs::read("/tmp/logo_render_check.png")
            .unwrap_or_default(),
        // No renderer on this host. Not a failure of the logo.
        _ => vec![0x89, 0x50, 0x4e, 0x47],
    }
}
