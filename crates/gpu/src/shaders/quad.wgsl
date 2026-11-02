// Two pictures and a table read, a picture written: `ta` and `tb` (all
// three pictures `w` × `h`, rows from the top), the table `tt`, `dst`.

@group(0) @binding(1) var<storage, read> ta: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> tb: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read> tt: array<vec4<i32>>;
@group(0) @binding(4) var<storage, read_write> dst: array<vec4<f32>>;
