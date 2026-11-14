// TAA-проход (ADR-0042): смешивание истории и текущего кадра.
//
// Совпадает с `render::post::taa_blend`; первый кадр истории — чёрный
// (изображение очищается перед проходом).

struct FrameConstants {
    float4 params0; // inv_size.x, inv_size.y, exposure, bloom_threshold
    float4 params1; // sun.xyz, bloom_intensity
    float4 params2; // radiance.xyz, taa_alpha
    float4 params3; // ssao_radius, unused...
};

[[vk::push_constant]] ConstantBuffer<FrameConstants> frame : register(b0);

[[vk::binding(0, 0)]] Texture2D<float4> current_tex : register(t0);
[[vk::binding(1, 0)]] Texture2D<float4> history_tex : register(t1);
[[vk::binding(2, 0)]] SamplerState samp : register(s0);

struct VSOut {
    float4 position : SV_Position;
};

float4 fs_main(VSOut input) : SV_Target {
    float2 uv = input.position.xy * frame.params0.xy;
    float alpha = saturate(frame.params2.w);
    float3 current = current_tex.SampleLevel(samp, uv, 0.0).rgb;
    float3 history = history_tex.SampleLevel(samp, uv, 0.0).rgb;
    return float4(lerp(history, current, alpha), 1.0);
}
