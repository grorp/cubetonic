struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
}

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
) -> VertexOutput {
    var out: VertexOutput;
    switch vertex_index {
        case 0: {
            // Top-middle
            out.clip_position = vec4<f32>(0.0, 1.0, 0.0, 1.0);
        }
        case 1: {
            // Bottom-left
            out.clip_position = vec4<f32>(-1.0, -1.0, 0.0, 1.0);
        }
        case 2: {
            // Bottom-right
            out.clip_position = vec4<f32>(1.0, -1.0, 0.0, 1.0);
        }
        default: {} // Unreachable
    }
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(1.0, 0.0, 0.0, 1.0);
}
