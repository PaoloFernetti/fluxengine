// G-buffer из геометрии сцены (4.5, ADR-0047).
//
// RT0 — albedo (RGBA8 sRGB), RT1 — oct-normal (RGBA16F), RT2 — ORM
// (ao=1, roughness, metallic, bindless=0), SV_Depth — Reversed-Z:
// SV_Position.z уже в NDC 0..1, ближе — больше.

struct VSOut {
    float4 position : SV_Position;
    float3 normal : NORMAL;
    float3 albedo : COLOR0;
    float2 material : TEXCOORD0;
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
    float3 normal = normalize(input.normal);
    output.rt0 = float4(input.albedo, 1.0);
    output.rt1 = float4(oct_encode(normal), 0.0, 0.0);
    output.rt2 = float4(1.0, saturate(input.material.x), saturate(input.material.y), 0.0);
    output.depth = input.position.z;
    return output;
}
