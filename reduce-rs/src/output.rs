//! Output file writer (Svenska Spel "Externa systemspel" format).

use crate::system::{FullSystem, NUM_MATCHES};
use std::fs;
use std::io;
use std::path::Path;

/// Build the file contents in memory: the product line and one `E,...` line
/// per chosen row.
///
/// The rows are rendered straight from `fs.digits`, so no row vectors are
/// materialised (matters for systems with millions of rows).
pub fn render(product: &str, fs: &FullSystem, chosen: &[u32], newline: &str) -> String {
    let mut buf = String::new();
    buf.push_str(product);
    for &c in chosen {
        buf.push_str(newline);
        buf.push_str("E,");
        let row = &fs.digits[c as usize * NUM_MATCHES..(c as usize + 1) * NUM_MATCHES];
        for (m, &d) in row.iter().enumerate() {
            if m > 0 {
                buf.push(',');
            }
            let sign = fs.entries[m].chars().nth(d as usize).unwrap();
            buf.push(sign);
        }
    }
    buf.push_str(newline);
    buf
}

/// Write the rendered system to a file.
pub fn write_file(
    path: &Path,
    product: &str,
    fs: &FullSystem,
    chosen: &[u32],
    newline: &str,
) -> io::Result<()> {
    let buf = render(product, fs, chosen, newline);
    fs::write(path, buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::parse_system;

    #[test]
    fn writes_expected_bytes() {
        let fs = FullSystem::build(parse_system("1,x,2,1,x,2,1,x,2,1,x,2,1").unwrap());
        let dir = std::env::temp_dir().join("reduce-rs-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("out.txt");
        write_file(&path, "Stryktipset", &fs, &[0], "\n").unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(text, "Stryktipset\nE,1,X,2,1,X,2,1,X,2,1,X,2,1\n");
        write_file(&path, "Stryktipset", &fs, &[0], "\r\n").unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(text, "Stryktipset\r\nE,1,X,2,1,X,2,1,X,2,1,X,2,1\r\n");
    }

    #[test]
    fn render_matches_write_file() {
        let fs = FullSystem::build(parse_system("1,x,x2,2,1,1x2,1,x,1,2,1x,1,1x").unwrap());
        for newline in ["\n", "\r\n"] {
            let rendered = render("Stryktipset", &fs, &[0, 1], newline);
            let dir = std::env::temp_dir().join("reduce-rs-test");
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join("render_check.txt");
            write_file(&path, "Stryktipset", &fs, &[0, 1], newline).unwrap();
            let written = fs::read(&path).unwrap();
            assert_eq!(rendered.into_bytes(), written);
        }
    }
}
