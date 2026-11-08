//! The system fonts a window draws with by default.
//!
//! A window's text must reach the GPU as glyph outlines, so the app needs real
//! faces. Without fontconfig, it looks for well-known families by file name in
//! the usual font folders: Noto, then DejaVu, then Liberation, for each of
//! sans-serif, serif, and monospace, with regular, bold, italic, and
//! bold-italic files where present. The sans-serif faces are also registered
//! under the empty family, which `FontSet` matches for any request, so a page
//! asking for `system-ui` or a font that isn't installed still gets text.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use noonoo_ttf::{Font, FontSet};

/// One face to load: a font file registered as `family` at a weight and style.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FacePlan {
    /// The family name it answers to (`""` matches any request).
    pub family: String,
    /// The weight, 100–900.
    pub weight: u16,
    /// Whether it's the italic (or oblique) face.
    pub italic: bool,
    /// The font file.
    pub path: PathBuf,
}

/// Each generic family's candidates, best first: (family name, file stem).
const GENERICS: &[(&str, &[(&str, &str)])] = &[
    (
        "sans-serif",
        &[
            ("Noto Sans", "NotoSans"),
            ("DejaVu Sans", "DejaVuSans"),
            ("Liberation Sans", "LiberationSans"),
        ],
    ),
    (
        "serif",
        &[
            ("Noto Serif", "NotoSerif"),
            ("DejaVu Serif", "DejaVuSerif"),
            ("Liberation Serif", "LiberationSerif"),
        ],
    ),
    (
        "monospace",
        &[
            ("Noto Sans Mono", "NotoSansMono"),
            ("DejaVu Sans Mono", "DejaVuSansMono"),
            ("Liberation Mono", "LiberationMono"),
        ],
    ),
];

/// Each style's file-name suffixes, in the conventions of those families:
/// (weight, italic, suffixes after the stem).
const STYLES: &[(u16, bool, &[&str])] = &[
    (400, false, &["-Regular.ttf", ".ttf"]),
    (700, false, &["-Bold.ttf"]),
    (400, true, &["-Italic.ttf", "-Oblique.ttf"]),
    (700, true, &["-BoldItalic.ttf", "-BoldOblique.ttf"]),
];

/// The faces to register, given every font file found.
pub fn plan(files: &[PathBuf]) -> Vec<FacePlan> {
    let find = |name: &str| {
        files
            .iter()
            .find(|path| path.file_name().is_some_and(|file| file == name))
    };
    let mut faces = Vec::new();
    for &(generic, candidates) in GENERICS {
        // The first candidate with a regular face.
        let chosen = candidates.iter().find_map(|&(family, stem)| {
            let styles: Vec<(u16, bool, &PathBuf)> = STYLES
                .iter()
                .filter_map(|&(weight, italic, suffixes)| {
                    suffixes
                        .iter()
                        .find_map(|suffix| find(&format!("{stem}{suffix}")))
                        .map(|path| (weight, italic, path))
                })
                .collect();
            let has_regular = styles
                .iter()
                .any(|&(weight, italic, _)| weight == 400 && !italic);
            has_regular.then_some((family, styles))
        });
        let Some((family, styles)) = chosen else {
            continue;
        };
        let mut names = vec![generic, family];
        if generic == "sans-serif" {
            names.push("");
        }
        for name in names {
            for &(weight, italic, path) in &styles {
                faces.push(FacePlan {
                    family: name.to_owned(),
                    weight,
                    italic,
                    path: path.clone(),
                });
            }
        }
    }
    faces
}

/// Every `.ttf` file under `dirs`, searched recursively. Missing folders are
/// skipped; depth is bounded so a symlink loop can't run forever.
pub fn font_files(dirs: &[PathBuf]) -> Vec<PathBuf> {
    fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if depth < 8 {
                    walk(&path, depth + 1, out);
                }
            } else if path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("ttf"))
            {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    for dir in dirs {
        walk(dir, 0, &mut out);
    }
    out
}

/// Load the planned faces into a font set that lives for the process.
/// Files that don't read or parse, or have no TrueType outlines, are skipped;
/// `None` when nothing loads.
///
/// The fonts and the set are leaked: the host's glyph renderer and the
/// shaper both borrow them for the life of the window. [`system`] loads
/// once per process.
pub fn load(plan: &[FacePlan]) -> Option<&'static FontSet<'static>> {
    // Each file once, though it's registered under several names.
    let mut paths: Vec<&PathBuf> = Vec::new();
    for face in plan {
        if !paths.contains(&&face.path) {
            paths.push(&face.path);
        }
    }
    let fonts: Vec<(PathBuf, Font)> = paths
        .into_iter()
        .filter_map(|path| {
            let bytes = std::fs::read(path).ok()?;
            let font = Font::parse(&bytes).ok()?;
            font.has_outlines().then(|| (path.clone(), font))
        })
        .collect();
    if fonts.is_empty() {
        return None;
    }
    let fonts: &'static [(PathBuf, Font)] = Box::leak(fonts.into_boxed_slice());
    let mut set = FontSet::new();
    for face in plan {
        if let Some((_, font)) = fonts.iter().find(|(path, _)| *path == face.path) {
            set.add(face.family.clone(), face.weight, face.italic, font);
        }
    }
    Some(Box::leak(Box::new(set)))
}

/// The usual font folders on Linux and other Unix desktops.
fn font_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/usr/share/fonts"),
        PathBuf::from("/usr/local/share/fonts"),
    ];
    if let Some(data) = std::env::var_os("XDG_DATA_HOME") {
        dirs.push(PathBuf::from(data).join("fonts"));
    }
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        dirs.push(home.join(".local/share/fonts"));
        dirs.push(home.join(".fonts"));
    }
    dirs
}

/// The system font set, found and loaded once per process; `None` when no
/// known family is installed (text then falls back to `DrawText` runs, which
/// the GPU backend doesn't draw).
pub fn system() -> Option<&'static FontSet<'static>> {
    static SYSTEM: OnceLock<Option<&'static FontSet<'static>>> = OnceLock::new();
    *SYSTEM.get_or_init(|| load(&plan(&font_files(&font_dirs()))))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(names: &[&str]) -> Vec<PathBuf> {
        names
            .iter()
            .map(|name| PathBuf::from(format!("/fonts/{name}")))
            .collect()
    }

    fn has(plan: &[FacePlan], family: &str, weight: u16, italic: bool, file: &str) -> bool {
        plan.iter().any(|face| {
            face.family == family
                && face.weight == weight
                && face.italic == italic
                && face.path == Path::new(&format!("/fonts/{file}"))
        })
    }

    #[test]
    fn the_first_installed_family_wins_for_each_generic() {
        let plan = plan(&files(&[
            "DejaVuSans.ttf",
            "NotoSans-Regular.ttf",
            "NotoSans-Bold.ttf",
            "NotoSans-BoldItalic.ttf",
            "DejaVuSansMono.ttf",
            "DejaVuSansMono-Oblique.ttf",
        ]));
        // Sans-serif is Noto (preferred over DejaVu), under its generic and
        // real names and as the catch-all.
        for family in ["sans-serif", "Noto Sans", ""] {
            assert!(
                has(&plan, family, 400, false, "NotoSans-Regular.ttf"),
                "{family}"
            );
            assert!(
                has(&plan, family, 700, false, "NotoSans-Bold.ttf"),
                "{family}"
            );
            assert!(
                has(&plan, family, 700, true, "NotoSans-BoldItalic.ttf"),
                "{family}"
            );
        }
        assert!(!plan.iter().any(|f| f.path.ends_with("DejaVuSans.ttf")));
        // Monospace falls to DejaVu, whose regular has no suffix and whose
        // italic is "Oblique".
        assert!(has(&plan, "monospace", 400, false, "DejaVuSansMono.ttf"));
        assert!(has(
            &plan,
            "monospace",
            400,
            true,
            "DejaVuSansMono-Oblique.ttf"
        ));
        assert!(has(
            &plan,
            "DejaVu Sans Mono",
            400,
            false,
            "DejaVuSansMono.ttf"
        ));
        // No serif installed, no serif faces.
        assert!(!plan.iter().any(|f| f.family == "serif"));
    }

    #[test]
    fn a_family_without_a_regular_face_is_skipped() {
        let plan = plan(&files(&["NotoSans-Bold.ttf", "LiberationSans-Regular.ttf"]));
        assert!(has(
            &plan,
            "sans-serif",
            400,
            false,
            "LiberationSans-Regular.ttf"
        ));
        assert!(!plan.iter().any(|f| f.path.ends_with("NotoSans-Bold.ttf")));
    }

    #[test]
    fn no_fonts_no_faces() {
        assert!(plan(&[]).is_empty());
    }

    /// A scratch folder for one test, removed when dropped.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("noonoo-winit-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn font_files_are_found_in_nested_folders() {
        let scratch = Scratch::new("scan");
        std::fs::create_dir_all(scratch.0.join("noto/sub")).unwrap();
        std::fs::write(scratch.0.join("noto/sub/NotoSans-Regular.ttf"), b"x").unwrap();
        std::fs::write(scratch.0.join("Top.ttf"), b"x").unwrap();
        std::fs::write(scratch.0.join("noto/readme.txt"), b"x").unwrap();
        let mut found = font_files(&[scratch.0.clone(), scratch.0.join("missing")]);
        found.sort();
        assert_eq!(
            found,
            vec![
                scratch.0.join("Top.ttf"),
                scratch.0.join("noto/sub/NotoSans-Regular.ttf")
            ]
        );
    }

    #[test]
    fn a_file_that_is_not_a_font_is_skipped() {
        let scratch = Scratch::new("load");
        let path = scratch.0.join("NotoSans-Regular.ttf");
        std::fs::write(&path, b"not a font").unwrap();
        let face = FacePlan {
            family: String::new(),
            weight: 400,
            italic: false,
            path,
        };
        assert!(load(&[face]).is_none());
    }

    /// With a real font installed, it loads and draws outlines. Skipped
    /// where the file isn't present, since it comes from the system.
    #[test]
    fn an_installed_font_loads() {
        let path = PathBuf::from("/usr/share/fonts/noto/NotoSans-Regular.ttf");
        if !path.exists() {
            return;
        }
        let face = FacePlan {
            family: "sans-serif".into(),
            weight: 400,
            italic: false,
            path,
        };
        let set = load(&[face]).expect("Noto Sans loads");
        let font = set.select_best(&["sans-serif".into()], 400, false).unwrap();
        assert!(font.has_outlines());
    }
}
