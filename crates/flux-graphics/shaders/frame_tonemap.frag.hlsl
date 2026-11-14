// Tonemap-проход (ADR-0018/ADR-0042): bloom-combine + ACES (Narkowicz).
//
// Совпадает с `render::post::bloom_combine` и `render::tonemap::aces`.

struct FrameConstants {
    float4 params0; // inv_size.x, inv_size.y, exposure, bloom_threshold
    float4 params1; // sun.xyz, bloom_intensity
    float4 params2; // radiance.xyz, taa_alpha
    float4 params3; // ssao_radius, unused...
};

[[vk::push_constant]] ConstantBuffer<FrameConstants> frame : register(b0);

[[vk::binding(0, 0)]] Texture2D<float4> hdr_tex : register(t0);
[[vk::binding(1, 0)]] Texture2D<float4> bloom_tex : register(t1);
[[vk::binding(2, 0)]] SamplerState samp : register(s0);

struct VSOut {
    float4 position : SV_Position;
};

float3 aces(float3 color) {
    float3 x = max(color, 0.0);
    float3 numerator = x * (x * 2.51 + 0.03);
    float3 denominator = x * (x * 2.43 + 0.59) + 0.14;
    return saturate(numerator / max(denominator, 1e-6));
}

float4 fs_main(VSOut input) : SV_Target {
    float2 uv = input.position.xy * frame.params0.xy;
    float exposure = frame.params0.z;
    float intensity = frame.params1.w;
    float3 hdr = hdr_tex.SampleLevel(samp, uv, 0.0).rgb;
    float3 bloom = bloom_tex.SampleLevel(samp, uv, 0.0).rgb;
    float3 exposed = (hdr + bloom * max(intensity, 0.0)) * exp2(exposure);
    return float4(aces(exposed), 1.0);
}
