struct CameraUniform {
    view: mat4x4<f32>,
    view_proj: mat4x4<f32>,
    // order of the following two is intentional to avoid needing additional
    // alignment
    fog_color: vec3<f32>,
    z_far: f32,
}
@group(0) @binding(0)
var<uniform> camera: CameraUniform;

@group(1) @binding(0)
var textures: binding_array<texture_2d<f32>>;

@group(1) @binding(1)
var the_sampler: sampler;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) normal: vec3<f32>,
    @location(3) texture_index: u32,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) normal: vec3<f32>,
    @location(3) texture_index: u32,
    @location(4) view_position: vec3<f32>,
}

@vertex
fn vs_main(
    model: VertexInput,
) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(model.position, 1.0);
    out.position = model.position;
    out.uv = model.uv;
    out.normal = model.normal;
    out.texture_index = model.texture_index;
    out.view_position = (camera.view * vec4<f32>(model.position, 1.0)).xyz;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    /*
    let material_color = vec3<f32>(0.95, 0.95, 1.0);

    let ambient = 0.2;

    let light_pos = vec3<f32>(-10000.0, 0.0, -11000.0);
    let light_dir = normalize(light_pos - in.position);
    let diffuse = max(dot(in.normal, light_dir), 0.0);

    let light = ambient + diffuse;
    return vec4<f32>(material_color * light, 1.0);
    */

    var tex_color: vec4<f32> = textureSample(textures[in.texture_index], the_sampler, in.uv);
    // TODO: this is probably not the proper way to do this
    if tex_color.a == 0.0 {
        discard;
    }

    var color: vec3<f32> = tex_color.rgb;

    if abs(in.normal.x) > 0.001 {
        // +x or -x
        color *= 0.6;
    } else if abs(in.normal.z) > 0.001 {
        // +z or -z
        color *= 0.8;
    } else if in.normal.y < -0.001 {
        // -y
        color *= 0.2;
    }
    // +y = 1.0

    let fog_color = camera.fog_color;
    let fog_end = camera.z_far;
    let fog_start = fog_end * 0.8;

    let distance = length(in.view_position);
    let factor = smoothstep(fog_start, fog_end, distance);
    color = mix(color, fog_color, factor);

    return vec4<f32>(color, 1.0);
}
