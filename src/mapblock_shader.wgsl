struct CameraUniform {
    view_proj: mat4x4<f32>,
}
@group(0) @binding(0)
var<uniform> camera: CameraUniform;

@group(1) @binding(0)
var textures: binding_array<texture_2d<f32>>;

@group(1) @binding(1)
var samplers: binding_array<sampler>;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
}

@vertex
fn vs_main(
    model: VertexInput,
) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(model.position, 1.0);
    out.position = model.position;
    out.normal = model.normal;
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

    var color: vec3<f32> = textureSample(textures[201], samplers[201], in.position.xz).rgb;

    if (abs(in.normal.x) > 0.001) {
        // +x or -x
        color *= 0.6;
    } else if (abs(in.normal.z) > 0.001) {
        // +z or -z
        color *= 0.8;
    } else if (in.normal.y < -0.001) {
        // -y
        color *= 0.2; 
    }
    // +y = 1.0

    return vec4<f32>(color, 1.0);
}
