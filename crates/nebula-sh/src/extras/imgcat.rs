//! `imgcat`: an image file shown in the terminal, through the iTerm2 image protocol
//! (OSC 1337 File), which Nebula Terminal, iTerm2 and WezTerm draw.

use std::io::{IsTerminal, Read, Write};

const USAGE: &str = "Usage: imgcat [-w WIDTH] FILE…
Show images in the terminal. With no FILE, reads one from standard input.
  -w, --width WIDTH  in cells (40), as a share of the pane (50%) or in pixels (300px)";

/// What the terminal's image support takes at most (xterm.js's addon-image default).
const MAX_BYTES: usize = 20 * 1024 * 1024;

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let triple = u32::from(chunk[0]) << 16
            | u32::from(*chunk.get(1).unwrap_or(&0)) << 8
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for index in 0..4 {
            if index <= chunk.len() {
                out.push(ALPHABET[(triple >> (18 - index * 6) & 0x3f) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// PNG, JPEG, GIF, WebP and BMP, told apart by their first bytes.
fn is_image(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        || bytes.starts_with(&[0xff, 0xd8, 0xff])
        || bytes.starts_with(b"GIF8")
        || (bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP")
        || bytes.starts_with(b"BM")
}

fn valid_width(width: &str) -> bool {
    let digits = width.trim_end_matches('%').trim_end_matches("px");
    !digits.is_empty() && digits.len() <= 5 && digits.bytes().all(|byte| byte.is_ascii_digit())
}

/// The escape sequence that shows `bytes` as an image named `name`.
fn sequence(name: &str, bytes: &[u8], width: Option<&str>) -> String {
    let width = width
        .map(|width| format!(";width={width}"))
        .unwrap_or_default();
    format!(
        "\x1b]1337;File=name={};size={};inline=1{width}:{}\x07\n",
        base64(name.as_bytes()),
        bytes.len(),
        base64(bytes)
    )
}

pub fn run(args: &[String]) -> i32 {
    let mut width = None;
    let mut files = Vec::new();
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!("{USAGE}");
                return 0;
            }
            "-w" | "--width" => match rest.next() {
                Some(value) if valid_width(value) => width = Some(value.as_str()),
                _ => {
                    eprintln!("imgcat: -w takes a width like 40, 50% or 300px");
                    return 2;
                }
            },
            _ => files.push(arg.as_str()),
        }
    }

    let mut images: Vec<(String, std::io::Result<Vec<u8>>)> = files
        .iter()
        .map(|file| {
            let path = crate::sys::translate_path(file);
            (file.to_string(), std::fs::read(path))
        })
        .collect();
    if files.is_empty() {
        if std::io::stdin().is_terminal() {
            eprintln!("{USAGE}");
            return 2;
        }
        let mut bytes = Vec::new();
        let read = std::io::stdin().read_to_end(&mut bytes).map(|_| bytes);
        images.push(("image".to_owned(), read));
    }

    let mut out = std::io::stdout().lock();
    let mut status = 0;
    for (name, read) in images {
        let problem = match &read {
            Err(error) => Some(crate::exec::describe_io_error(error)),
            Ok(bytes) if bytes.len() > MAX_BYTES => Some("larger than 20 MB".to_owned()),
            Ok(bytes) if !is_image(bytes) => {
                Some("not a PNG, JPEG, GIF, WebP or BMP image".to_owned())
            }
            Ok(_) => None,
        };
        match (problem, read) {
            (Some(problem), _) => {
                eprintln!("imgcat: {name}: {problem}");
                status = 1;
            }
            (None, Ok(bytes)) => {
                let file_name = std::path::Path::new(&name)
                    .file_name()
                    .map_or(name.clone(), |part| part.to_string_lossy().into_owned());
                if out
                    .write_all(sequence(&file_name, &bytes, width).as_bytes())
                    .is_err()
                {
                    return 1;
                }
            }
            (None, Err(_)) => unreachable!("a read error is a problem"),
        }
    }
    let _ = out.flush();
    status
}

#[cfg(test)]
mod tests {
    use super::{base64, is_image, sequence, valid_width};

    #[test]
    fn encodes_base64() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn knows_images_by_their_first_bytes() {
        assert!(is_image(b"\x89PNG\r\n\x1a\n...."));
        assert!(is_image(b"GIF89a"));
        assert!(is_image(b"RIFF\0\0\0\0WEBPVP8 "));
        assert!(!is_image(b"hello"));
    }

    #[test]
    fn takes_widths_in_cells_percent_or_pixels() {
        assert!(valid_width("40") && valid_width("50%") && valid_width("300px"));
        assert!(!valid_width("") && !valid_width("wide") && !valid_width("-3"));
        assert_eq!(
            sequence("a.png", b"PNG", Some("50%")),
            "\x1b]1337;File=name=YS5wbmc=;size=3;inline=1;width=50%:UE5H\x07\n"
        );
    }
}
