// Bloom-проход (ADR-0042): порог + box-блюр 5 отсчётов.
//
// Совпадает с CPU-эталоном `render::post::bloom_threshold`; блюр — среднее
// пороговых значений по центру и 4 соседям (координаты clamp).

struct FrameConstants {
    float4 params0; // inv_size.x, inv_size.y, exposure, bloom_threshold
    float4 params1; // sun.xyz, bloom_intensity
    float4 params2; // radiance.xyz, taa_alpha
    float4 params3; // ssao_radius, unused...
};

[[vk::push_constant]] ConstantBuffer<FrameConstants> frame : register(b0);

[[vk::binding(0, 0)]] Texture2D<float4> hdr_tex : register(t0);
[[vk::binding(1, 0)]] SamplerState samp : register(s0);

struct VSOut {
    float4 position : SV_Position;
};

float3 bright(float3 color, float threshold) {
    return max(color - threshold, 0.0);
}

float4 fs_main(VSOut input) : SV_Target {
    float2 inv_size = frame.params0.xy;
    float threshold = frame.params0.w;
    float2 uv = input.position.xy * inv_size;
    float3 sum = bright(hdr_tex.SampleLevel(samp, uv, 0.0).rgb, threshold);
    [unroll] for (int i = 0; i < 4; ++i) {
        float2 offset = float2(i == 0 ? 1.0 : (i == 1 ? -1.0 : 0.0),
                               i == 2 ? 1.0 : (i == 3 ? -1.0 : 0.0));
        float2 suv = saturate(uv + offset * inv_size);
        sum += bright(hdr_tex.SampleLevel(samp, suv, 0.0).rgb, threshold);
    }
    return float4(sum / 5.0, 1.0);
}
