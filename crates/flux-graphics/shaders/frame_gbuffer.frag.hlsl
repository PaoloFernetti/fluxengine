// G-buffer-проход кадра (ADR-0017/ADR-0042): процедурная поверхность.
//
// RT0 — albedo (RGBA8 sRGB, alpha не используется), RT1 — oct-normal
// (RGBA16F), RT2 — ORM (occlusion seed, roughness, metallic, bindless=0),
// SV_Depth — Reversed-Z (ближе — больше).

struct FrameConstants {
    float4 params0; // inv_size.x, inv_size.y, exposure, bloom_threshold
    float4 params1; // sun.xyz, bloom_intensity
    float4 params2; // radiance.xyz, taa_alpha
    float4 params3; // ssao_radius, unused...
};

[[vk::push_constant]] ConstantBuffer<FrameConstants> frame : register(b0);

struct VSOut {
    float4 position : SV_Position;
};

struct GBufferOut {
    float4 rt0 : SV_Target0;
    float4 rt1 : SV_Target1;
    float4 rt2 : SV_Target2;
    float depth : SV_Depth;
};

float2 oct_encode(float3 n) {
    n /= (abs(n.x) + abs(n.y) + abs(n.z));
    float2 e = n.z >= 0.0
        ? n.xy
        : (1.0 - abs(n.yx)) * float2(n.x >= 0.0 ? 1.0 : -1.0, n.y >= 0.0 ? 1.0 : -1.0);
    return e;
}

GBufferOut fs_main(VSOut input) {
    GBufferOut output;
    float2 inv_size = frame.params0.xy;
    float2 uv = input.position.xy * inv_size;
    float2 p = uv * 2.0 - 1.0;
    float r2 = dot(p, p);
    float h = saturate(1.0 - r2);
    float3 n = normalize(float3(-p.x * 2.0, -p.y * 2.0, 0.5 + h));

    float3 albedo = lerp(float3(0.15, 0.2, 0.25), float3(0.85, 0.55, 0.3), h);
    float roughness = lerp(0.85, 0.25, h);
    float metallic = lerp(0.0, 0.9, saturate(h * 1.5 - 0.5));
    float ao_seed = saturate(0.6 + 0.4 * h);

    output.rt0 = float4(albedo, 1.0);
    output.rt1 = float4(oct_encode(n), 0.0, 0.0);
    output.rt2 = float4(ao_seed, roughness, metallic, 0.0);
    output.depth = 1.0 - h * 0.5;
    return output;
}
