use super::*;

/// The pixel `code` gives at `uv`, the first picture red and the second
/// blue.
fn pixel(code: &str, progress: f32, uv: [f32; 2]) -> [f32; 4] {
    let p = Program::compile(code).unwrap_or_else(|e| panic!("{e}"));
    pixel_of(&p, progress, uv, &[])
}

fn pixel_of(p: &Program, progress: f32, uv: [f32; 2], values: &[Val]) -> [f32; 4] {
    let sample = |which: u8, _: [f32; 2]| {
        if which == 0 {
            [1.0, 0.0, 0.0, 1.0]
        } else {
            [0.0, 0.0, 1.0, 1.0]
        }
    };
    let inputs = Inputs {
        progress,
        ratio: 16.0 / 9.0,
        values,
    };
    p.run(&mut Vec::new(), &inputs, &sample, uv)
}

fn error(code: &str) -> Error {
    Program::compile(code).expect_err("an error")
}

const FADE: &str = "// Author: gre
// License: MIT

vec4 transition (vec2 uv) {
  return mix(
    getFromColor(uv),
    getToColor(uv),
    progress
  );
}
";

#[test]
fn the_fade_mixes_the_pictures() {
    assert_eq!(pixel(FADE, 0.25, [0.5, 0.5]), [0.75, 0.0, 0.25, 1.0]);
    // Through the samplers, as older GL Transitions read them.
    let old = "vec4 transition(vec2 uv) { return mix(texture2D(fromImage, uv), texture2D(toImage, uv), progress); }";
    assert_eq!(pixel(old, 0.5, [0.5, 0.5]), [0.5, 0.0, 0.5, 1.0]);
}

#[test]
fn vectors_matrices_and_swizzles_work_as_in_glsl() {
    let code = "
        vec4 transition(vec2 uv) {
            vec3 v = vec3(uv, 3.0);
            v.xy *= 2.0;
            mat2 r = mat2(0.0, 1.0, -1.0, 0.0);
            vec2 t = r * vec2(1.0, 0.0);
            float d = dot(v, vec3(1.0));
            ivec2 i = ivec2(7, 2);
            int q = i.x / i.y;
            vec4 c = vec4(0.0);
            c.ba = vec2(1.0, 2.0);
            c[1] = 3.0;
            return vec4(v.zyx.x - 2.0, t.y, d / 4.5 * float(q) / 3.0,
                        length(vec2(3.0, 4.0)) / 5.0 * c.g / 3.0 * c.a / 2.0 * c.b);
        }";
    assert_eq!(pixel(code, 0.0, [0.25, 0.5]), [1.0; 4]);
}

#[test]
fn functions_loops_and_out_parameters() {
    let code = "
        float twice(float x) { return 2.0 * x; }
        void split(in vec2 v, out float a, inout float b) { a = v.x; b += v.y; }
        float twice(int x) { return float(x) * 20.0; }
        vec4 transition(vec2 uv) {
            float sum = 0.0;
            for (int i = 0; i < 10; i++) {
                if (i == 5) continue;
                if (i == 8) break;
                sum += float(i);
            }
            float a;
            float b = 1.0;
            split(vec2(4.0, 5.0), a, b);
            int n = 0;
            while (n < 3) n++;
            do { n += 10; } while (n < 0);
            return vec4(sum / 23.0, twice(a) / 8.0, b / 6.0 * twice(1) / 20.0, n == 13 ? 1.0 : 0.0);
        }";
    assert_eq!(pixel(code, 0.0, [0.0, 0.0]), [1.0; 4]);
}

#[test]
fn uniforms_take_their_defaults_and_the_preprocessor_reads_on() {
    let code = "
        #ifdef GL_ES
        precision mediump float;
        #endif
        #define SCALE 2.0
        #ifndef GL_ES
        this is left out
        #else
        uniform float amplitude; // = 100.0
        #endif
        uniform vec2 direction; // = vec2(0.0, 1.0)
        uniform bool flip; // = true
        uniform int count; // = 3
        const float HALF = 0.5;
        /* a comment
           over lines */
        vec4 transition(vec2 uv) {
            return vec4(amplitude / 100.0, direction.y, flip ? 1.0 : 0.0,
                        float(count) * SCALE / 6.0 * HALF * 2.0);
        }";
    let p = Program::compile(code).unwrap();
    let names: Vec<&str> = p.uniforms.iter().map(|u| u.name.as_str()).collect();
    assert_eq!(names, ["amplitude", "direction", "flip", "count"]);
    assert_eq!(p.uniforms[1].default[..2], [0.0, 1.0]);
    assert_eq!(p.uniforms[1].ty, Ty::VEC2);
    assert_eq!(pixel_of(&p, 0.0, [0.0, 0.0], &[]), [1.0; 4]);
    // Values given win over the defaults.
    let mut half = [0.0; 16];
    half[0] = 50.0;
    assert_eq!(pixel_of(&p, 0.0, [0.0, 0.0], &[half])[0], 0.5);
}

#[test]
fn built_in_functions_answer_as_glsl() {
    let code = "vec4 transition(vec2 uv) {
        return vec4(
            smoothstep(0.0, 1.0, 0.5) * 2.0,
            step(0.5, 0.7) * mod(-1.0, 3.0) / 2.0,
            clamp(fract(2.25) * 4.0, 0.0, 1.0),
            distance(vec2(0.0), vec2(3.0, 4.0)) / 5.0 * determinant(mat2(2.0, 0.0, 0.0, 0.5))
                * float(all(lessThan(vec2(0.0), uv + 1.0))) * (inverse(mat2(2.0))[1][1] * 2.0));
    }";
    assert_eq!(pixel(code, 0.0, [0.5, 0.5]), [1.0; 4]);
}

#[test]
fn errors_say_what_and_where() {
    let e = error("vec4 transition(vec2 uv) {\n  return foo;\n}");
    assert_eq!(e.line, 2);
    assert!(e.message.contains("'foo'"), "{e}");
    let e = error("vec4 transition(vec2 uv) {\n  vec3 a = vec2(1.0);\n  return vec4(a, 1.0);\n}");
    assert_eq!(e.line, 2, "{e}");
    assert!(
        error(
            "float f(float x) { return f(x); }\nvec4 transition(vec2 uv) { return vec4(f(1.0)); }"
        )
        .message
        .contains("recursion")
    );
    assert!(
        error("vec4 main(vec2 uv) { return vec4(0.0); }")
            .message
            .contains("transition")
    );
    assert!(
        error("uniform sampler2D map;\nvec4 transition(vec2 uv) { return vec4(0.0); }")
            .message
            .contains("getFromColor")
    );
    assert!(
        error("vec4 transition(vec2 uv) { progress = 1.0; return vec4(0.0); }")
            .message
            .contains("cannot be changed")
    );
    assert_eq!(
        error("vec4 transition(vec2 uv) {\n\n  return vec4(0.0) +;\n}").line,
        3
    );
}

#[test]
fn a_loop_that_never_ends_is_cut_short_and_discard_leaves_nothing() {
    let endless =
        "vec4 transition(vec2 uv) { float x = 0.0; while (true) { x += 1.0; } return vec4(1.0); }";
    assert_eq!(pixel(endless, 0.0, [0.0, 0.0]), [1.0; 4]);
    let gone = "vec4 transition(vec2 uv) { if (uv.x < 0.5) discard; return vec4(1.0); }";
    assert_eq!(pixel(gone, 0.0, [0.25, 0.0]), [0.0; 4]);
    assert_eq!(pixel(gone, 0.0, [0.75, 0.0]), [1.0; 4]);
}

/// How long a 1920 × 1080 frame of the collection's Ripple takes on one
/// thread (`cargo test --release -p bv-effects glsl_speed -- --ignored
/// --nocapture`).
#[test]
#[ignore]
fn glsl_speed() {
    let code = "uniform float amplitude; // = 100.0
        uniform float speed; // = 50.0
        vec4 transition (vec2 uv) {
          vec2 dir = uv - vec2(.5);
          float dist = length(dir);
          vec2 offset = dir * (sin(progress * dist * amplitude - progress * speed) + .5) / 30. * progress;
          return mix(getFromColor(uv + offset), getToColor(uv), smoothstep(0.2, 1.0, progress));
        }";
    let p = Program::compile(code).unwrap();
    let sample = |which: u8, q: [f32; 2]| [q[0], q[1], f32::from(which), 1.0];
    let inputs = Inputs {
        progress: 0.4,
        ratio: 16.0 / 9.0,
        values: &[],
    };
    let mut slots = Vec::new();
    let start = std::time::Instant::now();
    let mut sum = 0.0;
    for y in 0..1080 {
        for x in 0..1920 {
            let uv = [x as f32 / 1920.0, y as f32 / 1080.0];
            sum += p.run(&mut slots, &inputs, &sample, uv)[0];
        }
    }
    eprintln!("1080p frame: {:?} ({sum})", start.elapsed());
}

#[test]
fn arrays_and_macros_with_parameters() {
    let code = "
        #define SQ(x) ((x) * (x))
        #define MIX3(a, b, t) mix(a, b, t)
        const int N = 3;
        const float W[N] = float[N](0.25, 0.5, 0.25);
        vec2 pts[2];
        vec4 transition(vec2 uv) {
            float sum = 0.0;
            for (int i = 0; i < W.length(); i++) {
                sum += W[i];
            }
            pts[0] = vec2(1.0, 2.0);
            pts[1].y = 4.0;
            vec2 copy[2];
            copy = pts;
            float a[] = float[](1.0, 2.0, 3.0, 4.0);
            int k = 2;
            return vec4(sum, SQ(copy[1].y) / 16.0, MIX3(0.0, 2.0, 0.5),
                        a[k + 1] / 4.0 * float(a.length()) / 4.0);
        }";
    assert_eq!(pixel(code, 0.0, [0.0, 0.0]), [1.0; 4]);
    assert!(
        error("uniform float w[3];\nvec4 transition(vec2 uv) { return vec4(0.0); }")
            .message
            .contains("uniform arrays")
    );
    assert!(
        error("vec4 transition(vec2 uv) { float a[2]; return vec4(a); }")
            .message
            .contains("index it")
    );
}

#[test]
fn the_sine_and_cosine_are_close_to_the_true_ones() {
    let p = Program::compile(
        "uniform float kParamFloat0; vec4 transition(vec2 uv) { return vec4(sin(kParamFloat0), cos(kParamFloat0), 0.0, 1.0); }",
    )
    .unwrap();
    let mut slots = Vec::new();
    let none = |_: u8, _: [f32; 2]| [0.0f32; 4];
    for k in -4000..4000 {
        let x = k as f32 * 0.37 + 0.001;
        let mut v = [0.0f32; 16];
        v[0] = x;
        let inputs = Inputs {
            progress: 0.0,
            ratio: 1.0,
            values: &[v],
        };
        let c = p.run(&mut slots, &inputs, &none, [0.5, 0.5]);
        let (s, co) = (f64::from(x).sin(), f64::from(x).cos());
        assert!((f64::from(c[0]) - s).abs() < 2e-6, "sin {x}: {} {s}", c[0]);
        assert!(
            (f64::from(c[1]) - co).abs() < 2e-6,
            "cos {x}: {} {co}",
            c[1]
        );
    }
}
