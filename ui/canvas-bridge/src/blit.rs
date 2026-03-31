//! FBO-based canvas bridge for compositing external rendering into egui.

use wasm_bindgen::JsCast;
use web_sys::{
    HtmlCanvasElement, WebGl2RenderingContext as GL, WebGlBuffer, WebGlFramebuffer, WebGlProgram,
    WebGlTexture, WebGlUniformLocation, WebGlVertexArrayObject,
};

use crate::shaders;

/// Manages an offscreen FBO for capturing external renderer output and
/// blitting it into an egui PaintCallback viewport.
///
/// The bridge uses raw `WebGl2RenderingContext` (not glow) to avoid slotmap
/// isolation issues when multiple glow versions coexist.
pub struct CanvasBridge {
    gl: GL,
    fbo: WebGlFramebuffer,
    texture: WebGlTexture,
    quad_vao: WebGlVertexArrayObject,
    _quad_vbo: WebGlBuffer,
    program: WebGlProgram,
    u_texture_loc: WebGlUniformLocation,
    fbo_width: u32,
    fbo_height: u32,
}

impl CanvasBridge {
    /// Create a new bridge from an HTML canvas element.
    ///
    /// The canvas must already have a WebGL2 context (e.g., created by eframe
    /// or femtovg). This retrieves the existing context via `getContext("webgl2")`.
    pub fn from_canvas(
        canvas: &HtmlCanvasElement,
        width: u32,
        height: u32,
    ) -> Result<Self, String> {
        let gl: GL = canvas
            .get_context("webgl2")
            .map_err(|e| format!("get_context failed: {e:?}"))?
            .ok_or("No WebGL2 context on canvas")?
            .dyn_into::<GL>()
            .map_err(|_| "Context is not WebGL2")?;

        Self::new(gl, width, height)
    }

    /// Create a new bridge from an existing WebGL2 context.
    pub fn new(gl: GL, width: u32, height: u32) -> Result<Self, String> {
        let width = width.max(1);
        let height = height.max(1);

        // Create FBO + texture
        let fbo = gl
            .create_framebuffer()
            .ok_or("Failed to create framebuffer")?;
        let texture = gl.create_texture().ok_or("Failed to create texture")?;

        gl.bind_framebuffer(GL::FRAMEBUFFER, Some(&fbo));
        gl.bind_texture(GL::TEXTURE_2D, Some(&texture));
        gl.tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_opt_u8_array(
            GL::TEXTURE_2D,
            0,
            GL::RGBA8 as i32,
            width as i32,
            height as i32,
            0,
            GL::RGBA,
            GL::UNSIGNED_BYTE,
            None,
        )
        .map_err(|e| format!("tex_image_2d: {e:?}"))?;

        gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_MIN_FILTER, GL::LINEAR as i32);
        gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_MAG_FILTER, GL::LINEAR as i32);
        gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_WRAP_S, GL::CLAMP_TO_EDGE as i32);
        gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_WRAP_T, GL::CLAMP_TO_EDGE as i32);

        gl.framebuffer_texture_2d(
            GL::FRAMEBUFFER,
            GL::COLOR_ATTACHMENT0,
            GL::TEXTURE_2D,
            Some(&texture),
            0,
        );

        let status = gl.check_framebuffer_status(GL::FRAMEBUFFER);
        if status != GL::FRAMEBUFFER_COMPLETE {
            return Err(format!("FBO not complete: {status}"));
        }

        gl.bind_framebuffer(GL::FRAMEBUFFER, None);
        gl.bind_texture(GL::TEXTURE_2D, None);

        // Compile blit shader
        let program = shaders::compile_program(
            &gl,
            shaders::BLIT_VERTEX_SHADER,
            shaders::BLIT_FRAGMENT_SHADER,
        )?;
        let u_texture_loc = gl
            .get_uniform_location(&program, "u_texture")
            .ok_or("Missing u_texture uniform")?;

        // Create fullscreen quad VAO
        let vao = gl.create_vertex_array().ok_or("Failed to create VAO")?;
        let vbo = gl.create_buffer().ok_or("Failed to create VBO")?;

        gl.bind_vertex_array(Some(&vao));
        gl.bind_buffer(GL::ARRAY_BUFFER, Some(&vbo));

        unsafe {
            let data = js_sys::Float32Array::view(&shaders::QUAD_VERTICES);
            gl.buffer_data_with_array_buffer_view(GL::ARRAY_BUFFER, &data, GL::STATIC_DRAW);
        }

        let stride = 4 * std::mem::size_of::<f32>() as i32;
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_with_i32(0, 2, GL::FLOAT, false, stride, 0);
        gl.enable_vertex_attrib_array(1);
        gl.vertex_attrib_pointer_with_i32(
            1,
            2,
            GL::FLOAT,
            false,
            stride,
            2 * std::mem::size_of::<f32>() as i32,
        );

        gl.bind_vertex_array(None);

        tracing::info!("CanvasBridge created: {width}x{height}");

        Ok(Self {
            gl,
            fbo,
            texture,
            quad_vao: vao,
            _quad_vbo: vbo,
            program,
            u_texture_loc,
            fbo_width: width,
            fbo_height: height,
        })
    }

    /// Current FBO width in logical pixels.
    pub fn fbo_width(&self) -> u32 {
        self.fbo_width
    }

    /// Current FBO height in logical pixels.
    pub fn fbo_height(&self) -> u32 {
        self.fbo_height
    }

    /// Access the raw WebGL2 context.
    pub fn gl(&self) -> &GL {
        &self.gl
    }

    /// Resize the FBO texture if dimensions changed.
    pub fn ensure_size(&mut self, width: u32, height: u32) {
        let width = width.max(1);
        let height = height.max(1);
        if self.fbo_width == width && self.fbo_height == height {
            return;
        }
        self.fbo_width = width;
        self.fbo_height = height;
        self.gl.bind_texture(GL::TEXTURE_2D, Some(&self.texture));
        let _ = self
            .gl
            .tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_opt_u8_array(
                GL::TEXTURE_2D,
                0,
                GL::RGBA8 as i32,
                width as i32,
                height as i32,
                0,
                GL::RGBA,
                GL::UNSIGNED_BYTE,
                None,
            );
        self.gl.bind_texture(GL::TEXTURE_2D, None);
    }

    /// Capture the default framebuffer into the FBO texture, then clear
    /// the area on the default framebuffer to prevent residue.
    ///
    /// Call this immediately after your external renderer's `flush()`.
    /// The renderer draws to the default framebuffer (can't be avoided
    /// with renderers like femtovg), and we grab its output here.
    pub fn capture_default_fb(&self) {
        let gl = &self.gl;
        let w = self.fbo_width as i32;
        let h = self.fbo_height as i32;

        // Blit default framebuffer → our FBO
        gl.bind_framebuffer(GL::READ_FRAMEBUFFER, None);
        gl.bind_framebuffer(GL::DRAW_FRAMEBUFFER, Some(&self.fbo));
        gl.blit_framebuffer(0, 0, w, h, 0, 0, w, h, GL::COLOR_BUFFER_BIT, GL::NEAREST);

        gl.bind_framebuffer(GL::READ_FRAMEBUFFER, None);
        gl.bind_framebuffer(GL::DRAW_FRAMEBUFFER, None);

        // Clear the residue on the default framebuffer
        gl.enable(GL::SCISSOR_TEST);
        gl.scissor(0, 0, w, h);
        gl.clear_color(0.0, 0.0, 0.0, 0.0);
        gl.clear(GL::COLOR_BUFFER_BIT);
        gl.disable(GL::SCISSOR_TEST);
    }

    /// Blit the FBO texture to the current GL viewport.
    ///
    /// Call this after setting the viewport to the egui PaintCallback region.
    pub fn blit_to_viewport(&self) {
        let gl = &self.gl;

        gl.disable(GL::DEPTH_TEST);
        gl.disable(GL::STENCIL_TEST);
        gl.enable(GL::BLEND);
        gl.blend_func(GL::ONE, GL::ZERO);

        gl.use_program(Some(&self.program));
        gl.active_texture(GL::TEXTURE0);
        gl.bind_texture(GL::TEXTURE_2D, Some(&self.texture));
        gl.uniform1i(Some(&self.u_texture_loc), 0);

        gl.bind_vertex_array(Some(&self.quad_vao));
        gl.draw_arrays(GL::TRIANGLES, 0, 6);

        // Restore GL state for egui_glow
        gl.bind_vertex_array(None);
        gl.use_program(None);
        gl.bind_texture(GL::TEXTURE_2D, None);
        gl.disable(GL::BLEND);
    }

    /// Convenience: capture from default FB, set viewport from egui callback
    /// info, and blit to the correct position. Combines the full pipeline
    /// into a single call.
    ///
    /// Call this inside a `PaintCallback` after your renderer has flushed.
    pub fn capture_and_blit(&self, info: &egui::PaintCallbackInfo) {
        self.capture_default_fb();

        let vp = info.viewport_in_pixels();
        self.gl
            .viewport(vp.left_px, vp.from_bottom_px, vp.width_px, vp.height_px);
        self.gl
            .scissor(vp.left_px, vp.from_bottom_px, vp.width_px, vp.height_px);
        self.gl.enable(GL::SCISSOR_TEST);

        self.blit_to_viewport();

        self.gl.disable(GL::SCISSOR_TEST);
    }
}
