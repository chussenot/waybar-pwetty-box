//! Tile-level background shader: a Shadertoy-style GLSL fragment shader rendered
//! on the GPU into an offscreen texture, read back as RGBA so the live widget can
//! composite it via Cairo (like the femtovg background layer).
//!
//! Requires a current GL context (the engine's [`crate::offscreen::OffscreenGl`]).

use glow::HasContext;

/// Full-screen-triangle vertex shader (no vertex buffers needed).
const VERTEX_SRC: &str = "#version 300 es\n\
    void main() {\n\
        vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));\n\
        gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);\n\
    }\n";

/// Wrap Shadertoy-style user code (which defines
/// `void mainImage(out vec4 fragColor, in vec2 fragCoord)`) into a complete
/// GLES3 fragment shader exposing the standard `iResolution`/`iTime`/`iFrame`.
pub fn wrap_fragment(user: &str) -> String {
    format!(
        "#version 300 es\n\
         precision highp float;\n\
         uniform vec3 iResolution;\n\
         uniform float iTime;\n\
         uniform int iFrame;\n\
         out vec4 _pwetty_fragColor;\n\
         {user}\n\
         void main() {{ mainImage(_pwetty_fragColor, gl_FragCoord.xy); }}\n"
    )
}

/// A compiled background shader plus its offscreen render target.
pub struct ShaderPass {
    gl: glow::Context,
    program: glow::Program,
    vao: glow::VertexArray,
    target: Option<(glow::Framebuffer, glow::Texture, i32, i32)>,
}

impl ShaderPass {
    /// Compile `user_fragment` (Shadertoy `mainImage`). A GL context must be
    /// current. Returns the shader info log on compile/link failure.
    pub fn new(user_fragment: &str) -> Result<Self, String> {
        crate::gl::ensure_loaded();
        let gl = unsafe { glow::Context::from_loader_function(crate::gl::proc_addr) };
        unsafe {
            let program = link(&gl, VERTEX_SRC, &wrap_fragment(user_fragment))?;
            let vao = gl.create_vertex_array()?;
            Ok(Self {
                gl,
                program,
                vao,
                target: None,
            })
        }
    }

    /// Render one frame at `w`×`h` device pixels and read it back as RGBA8,
    /// top-left origin (ready for [`crate::paint_rgba`]).
    pub fn render(&mut self, w: i32, h: i32, time: f32, frame: i32) -> Vec<u8> {
        let mut buf = vec![0u8; (w.max(1) * h.max(1) * 4) as usize];
        unsafe { self.ensure_target(w, h) };
        let gl = &self.gl;
        unsafe {
            let (fbo, ..) = self.target.unwrap();
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.viewport(0, 0, w, h);
            gl.use_program(Some(self.program));
            if let Some(loc) = gl.get_uniform_location(self.program, "iResolution") {
                gl.uniform_3_f32(Some(&loc), w as f32, h as f32, 1.0);
            }
            if let Some(loc) = gl.get_uniform_location(self.program, "iTime") {
                gl.uniform_1_f32(Some(&loc), time);
            }
            if let Some(loc) = gl.get_uniform_location(self.program, "iFrame") {
                gl.uniform_1_i32(Some(&loc), frame);
            }
            gl.bind_vertex_array(Some(self.vao));
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
            gl.read_pixels(
                0,
                0,
                w,
                h,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut buf)),
            );
        }
        flip_rows(&mut buf, w.max(1) as usize, h.max(1) as usize);
        buf
    }

    /// (Re)create the color texture + FBO when the size changes.
    unsafe fn ensure_target(&mut self, w: i32, h: i32) {
        if self.target.map(|(_, _, tw, th)| (tw, th)) == Some((w, h)) {
            return;
        }
        let gl = &self.gl;
        if let Some((fbo, tex, _, _)) = self.target.take() {
            gl.delete_framebuffer(fbo);
            gl.delete_texture(tex);
        }
        let tex = gl.create_texture().unwrap();
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            w,
            h,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(None),
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MIN_FILTER,
            glow::LINEAR as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MAG_FILTER,
            glow::LINEAR as i32,
        );
        let fbo = gl.create_framebuffer().unwrap();
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
        gl.framebuffer_texture_2d(
            glow::FRAMEBUFFER,
            glow::COLOR_ATTACHMENT0,
            glow::TEXTURE_2D,
            Some(tex),
            0,
        );
        self.target = Some((fbo, tex, w, h));
    }
}

unsafe fn link(gl: &glow::Context, vs: &str, fs: &str) -> Result<glow::Program, String> {
    let v = compile(gl, glow::VERTEX_SHADER, vs)?;
    let f = compile(gl, glow::FRAGMENT_SHADER, fs)?;
    let program = gl.create_program()?;
    gl.attach_shader(program, v);
    gl.attach_shader(program, f);
    gl.link_program(program);
    let ok = gl.get_program_link_status(program);
    gl.delete_shader(v);
    gl.delete_shader(f);
    if ok {
        Ok(program)
    } else {
        Err(gl.get_program_info_log(program))
    }
}

unsafe fn compile(gl: &glow::Context, kind: u32, src: &str) -> Result<glow::Shader, String> {
    let shader = gl.create_shader(kind)?;
    gl.shader_source(shader, src);
    gl.compile_shader(shader);
    if gl.get_shader_compile_status(shader) {
        Ok(shader)
    } else {
        Err(gl.get_shader_info_log(shader))
    }
}

/// Flip image rows in place (GL's bottom-left origin → top-left for Cairo).
fn flip_rows(buf: &mut [u8], w: usize, h: usize) {
    let stride = w * 4;
    for y in 0..h / 2 {
        let (top, bot) = (y * stride, (h - 1 - y) * stride);
        for i in 0..stride {
            buf.swap(top + i, bot + i);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::wrap_fragment;

    #[test]
    fn wrap_adds_boilerplate_and_user_code() {
        let s = wrap_fragment("void mainImage(out vec4 c, in vec2 p){ c = vec4(1.0); }");
        assert!(s.starts_with("#version 300 es"));
        assert!(s.contains("uniform float iTime;"));
        assert!(s.contains("uniform vec3 iResolution;"));
        assert!(s.contains("void mainImage")); // user code present
        assert!(s.contains("mainImage(_pwetty_fragColor, gl_FragCoord.xy)"));
    }
}
