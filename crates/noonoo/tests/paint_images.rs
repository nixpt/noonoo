//! `noonoo --paint` loads a page's images from beside it.

use std::process::Command;

/// A PNG signature and IHDR chunk for a `width` × `height` image.
fn png(width: u32, height: u32) -> Vec<u8> {
    let mut data = b"\x89PNG\r\n\x1a\n".to_vec();
    data.extend(13u32.to_be_bytes());
    data.extend(b"IHDR");
    data.extend(width.to_be_bytes());
    data.extend(height.to_be_bytes());
    data.extend([8, 6, 0, 0, 0]);
    data
}

#[test]
fn paint_draws_images_beside_the_page() {
    let dir = std::env::temp_dir().join("noonoo-cli-paint-images");
    std::fs::create_dir_all(dir.join("img")).unwrap();
    std::fs::write(dir.join("img/a.png"), png(40, 30)).unwrap();
    let page = dir.join("page.html");
    std::fs::write(
        &page,
        r#"<img src="img/a.png"><img src="missing.png" alt="gone">"#,
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_noonoo"))
        .arg("--paint")
        .arg(&page)
        .output()
        .unwrap();
    assert!(output.status.success());
    let json = String::from_utf8(output.stdout).unwrap();
    assert_eq!(json.matches(r#""op": "draw-image""#).count(), 1, "{json}");
    assert!(json.contains(r#""width": 40"#), "{json}");
    assert!(json.contains("gone"), "{json}");
}
