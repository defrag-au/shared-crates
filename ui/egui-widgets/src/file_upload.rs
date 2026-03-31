//! File upload widget — opens a browser file picker and reads the selected file.
//!
//! # Usage
//!
//! ```ignore
//! // Store alongside your UI state:
//! let mut uploader = FileUploadButton::new("upload_sources");
//!
//! // Each frame:
//! if let Some(file) = uploader.show(ui, "Upload Image", "image/*") {
//!     // file.name, file.mime_type, file.data are available
//!     send_to_server(&file);
//! }
//! ```
//!
//! The widget is wasm32-only (browser file APIs).

/// A file selected by the user, read into memory.
#[derive(Debug, Clone)]
pub struct UploadedFile {
    /// Original filename (e.g. "alien-laser-eyes.png").
    pub name: String,
    /// MIME type (e.g. "image/png").
    pub mime_type: String,
    /// Raw file bytes.
    pub data: Vec<u8>,
}

/// A button that opens the browser file picker and reads the selected file.
///
/// Results are delivered asynchronously via an internal inbox —
/// call [`show()`](FileUploadButton::show) each frame to check for results.
pub struct FileUploadButton {
    inbox: egui_inbox::UiInbox<UploadedFile>,
    id: String,
}

impl FileUploadButton {
    /// Create a new upload button with a unique ID (used for the hidden input element).
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            inbox: egui_inbox::UiInbox::new(),
            id: id.into(),
        }
    }

    /// Show the upload button. Returns `Some(UploadedFile)` when a file has been read.
    ///
    /// - `label`: button text (e.g. "Upload Image")
    /// - `accept`: file type filter (e.g. "image/*", ".png,.jpg,.webp")
    #[cfg(target_arch = "wasm32")]
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        label: &str,
        accept: &str,
    ) -> Option<UploadedFile> {
        if ui.button(label).clicked() {
            self.trigger_file_picker(accept);
        }

        // Check for async result from a previous pick.
        self.inbox.read(ui.ctx()).last()
    }

    /// Trigger the hidden file input element.
    #[cfg(target_arch = "wasm32")]
    fn trigger_file_picker(&self, accept: &str) {
        use wasm_bindgen::prelude::*;
        use wasm_bindgen::JsCast;

        let window = match web_sys::window() {
            Some(w) => w,
            None => return,
        };
        let document = match window.document() {
            Some(d) => d,
            None => return,
        };

        // Remove any previous hidden input with this ID.
        if let Some(existing) = document.get_element_by_id(&self.id) {
            existing.remove();
        }

        // Create a hidden <input type="file">.
        let input: web_sys::HtmlInputElement = match document
            .create_element("input")
            .ok()
            .and_then(|el| el.dyn_into::<web_sys::HtmlInputElement>().ok())
        {
            Some(el) => el,
            None => return,
        };

        input.set_type("file");
        input.set_accept(accept);
        input.set_id(&self.id);
        let _ = input
            .style()
            .set_property("display", "none");

        // Append to body so it's part of the DOM.
        if let Some(body) = document.body() {
            let _ = body.append_child(&input);
        }

        // Listen for the change event.
        let sender = self.inbox.sender();
        let input_clone = input.clone();
        let closure = Closure::once(move || {
            read_selected_file(&input_clone, sender);
        });
        let _ = input.add_event_listener_with_callback("change", closure.as_ref().unchecked_ref());
        closure.forget(); // leak — the DOM element is removed after use

        // Trigger the file picker dialog.
        input.click();
    }
}

/// Read the first selected file from an input element via FileReader.
#[cfg(target_arch = "wasm32")]
fn read_selected_file(
    input: &web_sys::HtmlInputElement,
    sender: egui_inbox::UiInboxSender<UploadedFile>,
) {
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;

    let files = match input.files() {
        Some(f) => f,
        None => return,
    };
    let file = match files.get(0) {
        Some(f) => f,
        None => return,
    };

    let name = file.name();
    let mime_type = file.type_();

    let reader = match web_sys::FileReader::new() {
        Ok(r) => r,
        Err(_) => return,
    };

    let reader_clone = reader.clone();
    let input_id = input.id();
    let onload = Closure::once(move || {
        let result = match reader_clone.result() {
            Ok(r) => r,
            Err(_) => return,
        };
        let array_buf = match result.dyn_into::<js_sys::ArrayBuffer>() {
            Ok(buf) => buf,
            Err(_) => return,
        };
        let uint8 = js_sys::Uint8Array::new(&array_buf);
        let data = uint8.to_vec();

        let _ = sender.send(UploadedFile {
            name,
            mime_type,
            data,
        });

        // Clean up the hidden input element.
        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
            if let Some(el) = doc.get_element_by_id(&input_id) {
                el.remove();
            }
        }
    });

    let _ = reader
        .add_event_listener_with_callback("loadend", onload.as_ref().unchecked_ref());
    onload.forget();

    let _ = reader.read_as_array_buffer(&file);
}
