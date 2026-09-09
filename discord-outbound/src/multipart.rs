//! `multipart/form-data` bodies for Discord attachment uploads.
//!
//! # Why hand-rolled
//!
//! There were three of these: `reqwest::multipart` (native), `web_sys::FormData`
//! (wasm), and hand-rolled bytes for `worker::Fetch` (augminted-bots'
//! `bot-tools`). Each is shaped for one HTTP stack, which is why there were
//! three — and a wire format written three times is the shape that drifts
//! silently.
//!
//! Building the bytes ourselves works on **all** of them: every stack can send
//! a `Vec<u8>` with a `Content-Type` header. So the platform split shrinks to
//! "how do I make an HTTP request", which is all it should ever have been.
//!
//! The other benefit is that this is a pure function. The `FormData` version
//! could not be tested at all without a browser.

/// Build a `multipart/form-data` body carrying a JSON payload and any files.
///
/// The shape is Discord's documented one: a `payload_json` part, then
/// `files[N]` parts whose index matches the `attachments[N].id` declared in the
/// payload. Returns the body and the boundary that must go in the
/// `Content-Type` header.
pub fn body(payload_json: &str, files: &[File<'_>]) -> (Vec<u8>, String) {
    // A fixed boundary is safe here because both kinds of part are things we
    // control: the payload is our own JSON (where the sequence could only
    // appear inside a string, which cannot contain an unescaped CRLF), and the
    // file parts are binary scanned only between delimiters that must start on
    // a line boundary. It also keeps this function pure and testable — a random
    // boundary would need an RNG in a code path with no other use for one.
    let boundary = "----defrag-discord-outbound";

    let size: usize = files.iter().map(|f| f.data.len()).sum();
    let mut out = Vec::with_capacity(size + payload_json.len() + 512 * (files.len() + 1));

    out.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    out.extend_from_slice(
        b"Content-Disposition: form-data; name=\"payload_json\"\r\n\
          Content-Type: application/json\r\n\r\n",
    );
    out.extend_from_slice(payload_json.as_bytes());

    for (index, file) in files.iter().enumerate() {
        out.extend_from_slice(format!("\r\n--{boundary}\r\n").as_bytes());
        out.extend_from_slice(
            format!(
                "Content-Disposition: form-data; name=\"files[{index}]\"; \
                 filename=\"{}\"\r\nContent-Type: {}\r\n\r\n",
                escape_filename(file.filename),
                file.content_type
            )
            .as_bytes(),
        );
        out.extend_from_slice(file.data);
    }

    out.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    (out, boundary.to_string())
}

/// One file part.
pub struct File<'a> {
    pub filename: &'a str,
    pub content_type: &'a str,
    pub data: &'a [u8],
}

/// The `Content-Type` header value for a body built with [`body`].
pub fn content_type(boundary: &str) -> String {
    format!("multipart/form-data; boundary={boundary}")
}

/// Keep a filename from breaking out of its own header.
///
/// A quote or a newline in a filename would end the `Content-Disposition`
/// value early and corrupt every part after it — and filenames are not always
/// ours (a plugin names its own render, a user names an upload). Backslash
/// escaping is what RFC 6266 permits inside a quoted-string; CR and LF have no
/// escape, so they go.
fn escape_filename(name: &str) -> String {
    name.chars()
        .filter(|c| *c != '\r' && *c != '\n')
        .flat_map(|c| {
            if c == '"' || c == '\\' {
                vec!['\\', c]
            } else {
                vec![c]
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(data: &'static [u8]) -> File<'static> {
        File {
            filename: "render.png",
            content_type: "image/png",
            data,
        }
    }

    #[test]
    fn the_body_has_both_parts_and_a_closing_delimiter() {
        let (out, boundary) = body("{\"a\":1}", &[png(&[0xDE, 0xAD])]);
        let text = String::from_utf8_lossy(&out);

        assert!(text.starts_with(&format!("--{boundary}\r\n")));
        assert!(text.contains("name=\"payload_json\""));
        assert!(text.contains("{\"a\":1}"));
        assert!(text.contains("name=\"files[0]\"; filename=\"render.png\""));
        // The closing delimiter has trailing dashes; without them Discord
        // treats the body as truncated and rejects the whole request.
        assert!(text.ends_with(&format!("\r\n--{boundary}--\r\n")));
    }

    #[test]
    fn binary_bytes_survive_verbatim() {
        // A PNG contains every byte value including CRLF sequences; the body
        // must carry them untouched rather than through any string coercion.
        let file: Vec<u8> = (0u8..=255).collect();
        let (out, _) = body(
            "{}",
            &[File {
                filename: "x.png",
                content_type: "image/png",
                data: &file,
            }],
        );
        assert!(
            out.windows(file.len()).any(|w| w == file.as_slice()),
            "file bytes must appear verbatim in the body"
        );
    }

    /// `files[N]` has to match `attachments[N].id` in the payload, so the index
    /// is a contract with the JSON rather than a label.
    #[test]
    fn every_file_gets_its_own_indexed_part() {
        let (out, _) = body(
            "{}",
            &[
                File {
                    filename: "a.png",
                    content_type: "image/png",
                    data: &[1],
                },
                File {
                    filename: "b.mp4",
                    content_type: "video/mp4",
                    data: &[2],
                },
            ],
        );
        let text = String::from_utf8_lossy(&out);

        assert!(text.contains("name=\"files[0]\"; filename=\"a.png\""));
        assert!(text.contains("name=\"files[1]\"; filename=\"b.mp4\""));
        assert!(text.contains("Content-Type: video/mp4"));
    }

    /// A body with no files is still valid multipart. Callers should send plain
    /// JSON instead, but producing something malformed here would be worse than
    /// producing something merely wasteful.
    #[test]
    fn no_files_still_closes_properly() {
        let (out, boundary) = body("{}", &[]);
        let text = String::from_utf8_lossy(&out);
        assert!(text.ends_with(&format!("\r\n--{boundary}--\r\n")));
        assert!(!text.contains("files["));
    }

    /// Filenames are not always ours — a plugin names its own render, a user
    /// names an upload. A quote would close the header's quoted-string early
    /// and a newline would end the header outright, in both cases turning the
    /// rest of the filename into headers of our own message.
    ///
    /// The fix keeps the text *inside* the quoted string rather than deleting
    /// it, so the assertion is about where it lands, not whether it survives.
    #[test]
    fn a_hostile_filename_cannot_break_out_of_its_header() {
        let (out, _) = body(
            "{}",
            &[File {
                filename: "a\".png\r\nContent-Type: text/html\r\n\r\n<script>",
                content_type: "image/png",
                data: &[1],
            }],
        );
        let text = String::from_utf8_lossy(&out);

        // The injected text is still there, but as filename characters: no
        // line in the body *starts* a header it did not write.
        assert!(
            !text
                .lines()
                .any(|line| line.starts_with("Content-Type: text/html")),
            "injection became a header: {text}"
        );
        // The closing quote is escaped, so the value has not ended early.
        assert!(text.contains(r#"filename="a\".png"#), "{text}");
        // Exactly two real Content-Type headers: payload_json's and the file's.
        assert_eq!(
            text.lines()
                .filter(|line| line.starts_with("Content-Type: "))
                .count(),
            2
        );
    }
}
