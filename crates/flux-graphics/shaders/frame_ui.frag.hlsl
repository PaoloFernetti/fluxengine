// UI-stub и вывод в образ swapchain (4.5, ADR-0048).
//
// Читает финальное изображение кадра (tonemap/TAA, Rgba16Float) и пишет в
// acquired-образ цепочки (Bgra8Srgb): sRGB-кодирование делает формат.
// UI-stub — затемнённая панель в левом верхнем углу (30% × 25%): проверяет,
// что оверлей действительно попадает в presented-кадр, не требуя UI-системы.

struct FrameConstants {
    float4 params0; // inv_size.x, inv_size.y, exposure, bloom_threshold
    float4 params1; // sun.xyz, bloom_intensity
    float4 params2; // radiance.xyz, taa_alpha
    float4 params3; // ssao_radius, unused...
};

[[vk::push_constant]] ConstantBuffer<FrameConstants> frame : register(b0);

[[vk::binding(0, 0)]] Texture2D<float4> color : register(t0);
[[vk::binding(1, 0)]] SamplerState color_sampler : register(s0);

struct VSOut {
    float4 position : SV_Position;
};

float4 fs_main(VSOut input) : SV_Target0 {
    float2 uv = input.position.xy * frame.params0.xy;
    float4 scene = color.SampleLevel(color_sampler, uv, 0);
    bool in_panel = uv.x < 0.3 && uv.y < 0.25;
    float4 panel = float4(0.02, 0.02, 0.02, 1.0);
    return in_panel ? lerp(scene, panel, 0.8) : scene;
}
