// SSAO-проход кадра (ADR-0042): сравнение глубины с 4 соседями.
//
// Формула совпадает с CPU-эталоном `render::post::ssao_factor`:
// сосед ближе центра (Reversed-Z: больше) и в пределах radius — окклюзия.

struct FrameConstants {
    float4 params0; // inv_size.x, inv_size.y, exposure, bloom_threshold
    float4 params1; // sun.xyz, bloom_intensity
    float4 params2; // radiance.xyz, taa_alpha
    float4 params3; // ssao_radius, unused...
};

[[vk::push_constant]] ConstantBuffer<FrameConstants> frame : register(b0);

[[vk::binding(0, 0)]] Texture2D<float> depth_tex : register(t0);
[[vk::binding(1, 0)]] SamplerState samp : register(s0);

struct VSOut {
    float4 position : SV_Position;
};

float4 fs_main(VSOut input) : SV_Target {
    float2 inv_size = frame.params0.xy;
    float2 uv = input.position.xy * inv_size;
    float radius = frame.params3.x;
    float center = depth_tex.SampleLevel(samp, uv, 0.0);

    float occluded = 0.0;
    [unroll] for (int i = 0; i < 4; ++i) {
        float2 offset = float2(i == 0 ? 1.0 : (i == 1 ? -1.0 : 0.0),
                               i == 2 ? 1.0 : (i == 3 ? -1.0 : 0.0));
        float2 suv = saturate(uv + offset * inv_size);
        float depth = depth_tex.SampleLevel(samp, suv, 0.0);
        if (depth > center && (depth - center) <= radius) {
            occluded += 1.0;
        }
    }
    float ao = 1.0 - occluded / 4.0;
    return float4(ao, ao, ao, 1.0);
}
