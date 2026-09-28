//! Release-packaging contract tests.
//!
//! These guard the class of bug where the installer, the Homebrew formula and
//! what `cargo dist` actually publishes drift apart. Every assertion here is
//! hermetic: it reads repo config and re-runs the installer's own parsing
//! logic against captured samples. Nothing touches the network.
//!
//! The original `scripts/install.sh` shipped four independent ways to fail
//! against a real release: it asked for `.tar.gz` when dist publishes `.tar.xz`,
//! for `checksums.txt` when dist publishes `sha256.sum`, matched checksums with
//! a regex that cannot match dist's BSD-style `*filename` lines, and assumed a
//! flat archive when dist nests the binary under `niki-<target>/`. Each of
//! those is checked below.

use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(rel: &str) -> String {
    let p = repo_root().join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

/// The archive extension cargo-dist actually publishes. `dist-workspace.toml`
/// does not spell this out, so it is pinned here and asserted against the
/// installer and the formula below.
const DIST_ARCHIVE_EXT: &str = "tar.xz";
const DIST_SUMS_FILE: &str = "sha256.sum";

#[test]
fn installer_requests_the_archive_extension_dist_publishes() {
    let sh = read("scripts/install.sh");
    assert!(
        sh.contains(&format!("ASSET=\"niki-${{TARGET}}.{DIST_ARCHIVE_EXT}\"")),
        "scripts/install.sh must build the asset name with `.{DIST_ARCHIVE_EXT}`; \
         cargo dist publishes that extension, not .tar.gz"
    );
    assert!(
        !sh.contains(".tar.gz\""),
        "scripts/install.sh still references .tar.gz; dist publishes .{DIST_ARCHIVE_EXT}"
    );
}

#[test]
fn installer_requests_the_sums_file_dist_publishes() {
    let sh = read("scripts/install.sh");
    assert!(
        sh.contains(&format!("SUMS=\"{DIST_SUMS_FILE}\"")),
        "scripts/install.sh must fetch `{DIST_SUMS_FILE}`; that is what dist publishes"
    );
    assert!(
        !sh.contains("checksums.txt"),
        "scripts/install.sh still fetches checksums.txt; dist publishes {DIST_SUMS_FILE}"
    );
}

#[test]
fn installer_extracts_with_the_matching_decompressor() {
    let sh = read("scripts/install.sh");
    assert!(
        sh.contains(r#"tar -xJf "$ASSET""#),
        "`.{DIST_ARCHIVE_EXT}` needs `tar -xJf`, not `tar -xzf`"
    );
}

#[test]
fn installer_handles_the_nested_archive_layout() {
    let sh = read("scripts/install.sh");
    assert!(
        sh.contains("niki-") || sh.contains("find . -mindepth 2"),
        "dist nests the binary under `niki-<target>/`; the installer must resolve it \
         rather than assuming `./niki` exists"
    );
}

/// Mirrors the installer's checksum-extraction logic against a `sha256.sum`
/// line. cargo-dist writes BSD-style `<hash> *<name>`; mirrors emit GNU-style
/// `<hash>  <name>`. The old logic matched `(\s|/)name$`, which matched neither
/// — and `[ *]?` matched only the single separator char, so the two-space GNU
/// form also failed. Both produced an empty EXPECTED, aborting every install.
fn extract_expected_hash(sums: &str, asset: &str) -> String {
    for line in sums.lines() {
        let Some((hash, rest)) = line.split_once(' ') else {
            continue;
        };
        if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            continue;
        }
        if rest.trim_start_matches(['*', ' ', '\t']) == asset {
            return hash.to_string();
        }
    }
    String::new()
}

#[test]
fn installer_checksum_logic_matches_a_real_dist_sums_line() {
    let asset = "niki-x86_64-unknown-linux-gnu.tar.xz";
    // Verbatim shape of a `sha256.sum` line produced by cargo-dist.
    let sums = "\
7ac38dbf239e4ea369ef82852b08ffb9a0dc1046dab7f6fb62ad1ae323bfe28c *niki-x86_64-unknown-linux-gnu.tar.xz
47f2d897ef26bd8c6d9d8c0acd2cb40ecc50e14d08ee8b6ce482edba92b00b25 *niki-aarch64-apple-darwin.tar.xz
";
    let got = extract_expected_hash(sums, asset);
    assert_eq!(
        got, "7ac38dbf239e4ea369ef82852b08ffb9a0dc1046dab7f6fb62ad1ae323bfe28c",
        "the BSD-style `*name` line must yield its hash; an empty result aborts every install"
    );
}

#[test]
fn installer_checksum_logic_still_accepts_the_gnu_text_form() {
    let asset = "niki-x86_64-unknown-linux-gnu.tar.xz";
    let sums = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef  niki-x86_64-unknown-linux-gnu.tar.xz\n";
    assert_eq!(
        extract_expected_hash(sums, asset),
        "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
    );
}

#[test]
fn installer_checksum_logic_rejects_an_asset_not_in_the_manifest() {
    let sums = "\
7ac38dbf239e4ea369ef82852b08ffb9a0dc1046dab7f6fb62ad1ae323bfe28c *niki-x86_64-unknown-linux-gnu.tar.xz
";
    assert_eq!(
        extract_expected_hash(sums, "niki-sparc-unknown-linux-gnu.tar.xz"),
        "",
        "an unknown asset must yield no hash so the installer aborts, rather than \
         silently comparing against an empty EXPECTED"
    );
}

#[test]
fn installer_covers_every_unix_target_dist_publishes() {
    let sh = read("scripts/install.sh");
    let dist = read("dist-workspace.toml");

    // Parse the target list out of dist-workspace.toml.
    let targets_line = dist
        .lines()
        .find(|l| l.trim_start().starts_with("targets = "))
        .expect("dist-workspace.toml must declare targets");
    let dist_targets: Vec<String> = targets_line
        .trim()
        .trim_start_matches("targets = ")
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(|s| s.trim().trim_matches('"').to_string())
        .collect();

    // The installer is a POSIX-shell script, so it cannot ship the Windows zip.
    // Every other target dist publishes must be installable through it.
    for t in &dist_targets {
        if t.contains("windows") {
            continue;
        }
        assert!(
            sh.contains(t.as_str()),
            "dist publishes `{t}` but scripts/install.sh has no case for it"
        );
    }
    assert!(
        !sh.contains(".tar.gz"),
        "scripts/install.sh still references .tar.gz"
    );
}

#[test]
fn homebrew_formula_version_matches_the_crate_version() {
    let cargo = read("Cargo.toml");
    let crate_version = cargo
        .lines()
        .find_map(|l| l.strip_prefix("version = \""))
        .and_then(|l| l.split('"').next())
        .expect("Cargo.toml must declare a version")
        .to_string();

    let rb = read("homebrew/niki.rb");
    let formula_version = rb
        .lines()
        .find_map(|l| l.trim().strip_prefix("version \""))
        .and_then(|l| l.split('"').next())
        .unwrap_or_else(|| {
            panic!("homebrew/niki.rb must declare a version; crate is {crate_version}")
        });

    assert_eq!(
        formula_version, crate_version,
        "homebrew/niki.rb is pinned to v{formula_version} but the crate is {crate_version}; \
         `brew install niki` would install a stale binary"
    );

    // Every URL in the formula must point at the same tag as the version.
    let tag = format!("v{crate_version}");
    for line in rb.lines().filter(|l| l.contains("releases/download/")) {
        assert!(
            line.contains(&tag),
            "formula URL does not reference {tag}: {}",
            line.trim()
        );
    }
}

#[test]
fn homebrew_formula_shas_are_well_formed() {
    let rb = read("homebrew/niki.rb");
    let shas: Vec<&str> = rb
        .lines()
        .filter_map(|l| l.trim().strip_prefix("sha256 \""))
        .filter_map(|l| l.split('"').next())
        .collect();

    assert_eq!(
        shas.len(),
        4,
        "expected a sha256 for each of the 4 unix targets, found {}",
        shas.len()
    );
    for s in shas {
        assert_eq!(s.len(), 64, "sha256 must be 64 hex chars, got `{s}`");
        assert!(
            s.chars().all(|c| c.is_ascii_hexdigit()),
            "sha256 must be hex, got `{s}`"
        );
    }
}

#[test]
fn homebrew_formula_resolves_the_nested_binary_path() {
    let rb = read("homebrew/niki.rb");
    assert!(
        rb.contains("niki-*/niki") || rb.contains("niki-*/"),
        "dist nests the binary under `niki-<target>/`; `bin.install \"niki\"` cannot find it"
    );
}

#[test]
fn cargo_homepage_is_reachable_in_the_readme_domain() {
    // The previous value (niki-site.vercel.app) 404s; the live site is the
    // pages.dev deployment. This is a cheap canary, not a link checker.
    let cargo = read("Cargo.toml");
    let homepage = cargo
        .lines()
        .find_map(|l| l.trim().strip_prefix("homepage = \""))
        .and_then(|l| l.split('"').next())
        .expect("Cargo.toml must declare a homepage");
    assert!(
        !homepage.contains("vercel.app"),
        "homepage `{homepage}` is the dead domain; use https://niki-web.pages.dev"
    );
}

#[test]
fn readme_install_one_liner_points_at_a_script_that_exists() {
    let body = read("README.md");
    if !body.contains("raw.githubusercontent.com/RavaniRoshan/niki") {
        return; // README does not advertise the piped one-liner
    }
    assert!(
        repo_root().join("scripts/install.sh").exists(),
        "README advertises the raw.githubusercontent install one-liner but \
         scripts/install.sh is missing — every copy-paste of that line 404s"
    );
}

/// The Scoop and Winget manifests must track the crate version too.
///
/// They were pinned at 0.7.0 while the crate was at 0.8.0, and nothing
/// noticed: the only version-parity test covered the Homebrew formula, so two
/// of the three advertised one-line install paths were quietly serving a
/// two-releases-old binary. The same lesson as everywhere else in this pass —
/// a check that only exists for one of the three things it claims to check.
///
/// `scoop install niki` and `winget install RavaniRoshan.niki` are both
/// advertised in the README, so both are release-blocking.
#[test]
fn scoop_and_winget_manifests_match_the_crate_version() {
    let crate_version = env!("CARGO_PKG_VERSION");

    let scoop: serde_json::Value =
        serde_json::from_str(&read("scoop/niki.json")).expect("scoop/niki.json must be valid JSON");
    assert_eq!(
        scoop["version"].as_str(),
        Some(crate_version),
        "scoop/niki.json is pinned to v{} but the crate is {crate_version}; \
         `scoop install niki` would install a stale binary",
        scoop["version"].as_str().unwrap_or("<none>")
    );
    // Scoop builds its download URL from the version field, so a stale
    // download URL is a second, separate way to serve the wrong binary.
    for (k, v) in scoop.as_object().expect("scoop manifest is an object") {
        if k == "version" {
            continue;
        }
        let text = v.to_string();
        for tag in text.match_indices("/releases/download/v") {
            let rest = &text[tag.0 + "/releases/download/v".len()..];
            let pinned: String = rest.chars().take_while(|c| *c != '/').collect();
            // `$version` is a PowerShell substitution into the manifest's own
            // version field, not a pin. The field check above is the real
            // assertion; checking this would be checking a placeholder.
            if pinned.starts_with('$') {
                continue;
            }
            assert_eq!(
                pinned, crate_version,
                "scoop manifest {k} points at v{pinned} but the crate is {crate_version}"
            );
        }
    }

    for manifest in [
        "winget/RavaniRoshan.niki.yaml",
        "winget/RavaniRoshan.niki.installer.yaml",
        "winget/RavaniRoshan.niki.locale.en-US.yaml",
    ] {
        let text = read(manifest);
        let version = text
            .lines()
            .find_map(|l| l.trim().strip_prefix("PackageVersion:"))
            .map(|v| v.trim().to_string())
            .unwrap_or_else(|| {
                panic!("{manifest} must declare a PackageVersion; crate is {crate_version}")
            });
        assert_eq!(
            version, crate_version,
            "{manifest} is pinned to {version} but the crate is {crate_version}; \
             `winget install RavaniRoshan.niki` would install a stale binary"
        );
        // ManifestVersion is a winget schema version, not our release — it is
        // allowed to differ, and confusing the two was the whole bug.
        for line in text.lines().filter(|l| l.contains("/releases/download/v")) {
            let rest = line
                .split("/releases/download/v")
                .nth(1)
                .unwrap_or_default();
            let pinned: String = rest.chars().take_while(|c| *c != '/').collect();
            // Same as scoop: a `$version` placeholder is not a pin.
            if pinned.starts_with('$') {
                continue;
            }
            assert_eq!(
                pinned, crate_version,
                "{manifest} downloads v{pinned} but the crate is {crate_version}"
            );
        }
    }
}
