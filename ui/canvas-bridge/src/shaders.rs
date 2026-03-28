//! Shader compilation and passthrough program for FBO blit.

use web_sys::{WebGl2RenderingContext as GL, WebGlProgram, WebGlShader};

pub const BLIT_VERTEX_SHADER: &str = r#"#version 300 es
precision mediump float;
layout(location = 0) in vec2 a_position;
layout(location = 1) in vec2 a_uv;
out vec2 v_uv;
void main() {
    v_uv = a_uv;
    gl_Position = vec4(a_position, 0.0, 1.0);
}
"#;

pub const BLIT_FRAGMENT_SHADER: &str = r#"#version 300 es
precision mediump float;
in vec2 v_uv;
out vec4 fragColor;
uniform sampler2D u_texture;
void main() {
    fragColor = texture(u_texture, v_uv);
}
"#;

/// Fullscreen quad — two triangles covering NDC (-1..1) with UV (0..1).
#[rustfmt::skip]
pub const QUAD_VERTICES: [f32; 24] = [
    // pos.x  pos.y   uv.x  uv.y
    -1.0,  1.0,    0.0, 1.0,
    -1.0, -1.0,    0.0, 0.0,
     1.0, -1.0,    1.0, 0.0,
    -1.0,  1.0,    0.0, 1.0,
     1.0, -1.0,    1.0, 0.0,
     1.0,  1.0,    1.0, 1.0,
];

pub fn compile_program(
    gl: &GL,
    vert_src: &str,
    frag_src: &str,
) -> Result<WebGlProgram, String> {
    let vert = compile_shader(gl, GL::VERTEX_SHADER, vert_src)?;
    let frag = compile_shader(gl, GL::FRAGMENT_SHADER, frag_src)?;

    let program = gl.create_program().ok_or("Failed to create program")?;
    gl.attach_shader(&program, &vert);
    gl.attach_shader(&program, &frag);
    gl.link_program(&program);

    if gl
        .get_program_parameter(&program, GL::LINK_STATUS)
        .as_bool()
        .unwrap_or(false)
    {
        Ok(program)
    } else {
        let log = gl.get_program_info_log(&program).unwrap_or_default();
        Err(format!("Program link failed: {log}"))
    }
}

fn compile_shader(
    gl: &GL,
    shader_type: u32,
    source: &str,
) -> Result<WebGlShader, String> {
    let shader = gl
        .create_shader(shader_type)
        .ok_or("Failed to create shader")?;
    gl.shader_source(&shader, source);
    gl.compile_shader(&shader);

    if gl
        .get_shader_parameter(&shader, GL::COMPILE_STATUS)
        .as_bool()
        .unwrap_or(false)
    {
        Ok(shader)
    } else {
        let log = gl.get_shader_info_log(&shader).unwrap_or_default();
        Err(format!("Shader compile failed: {log}"))
    }
}
