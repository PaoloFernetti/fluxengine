// Deferred lighting (ADR-0017/ADR-0042): G-buffer + SSAO -> HDR.
//
// Формулы GGX/Smith/Fresnel/multiscatter совпадают с `render::pbr`.

struct FrameConstants {
    float4 params0; // inv_size.x, inv_size.y, exposure, bloom_threshold
    float4 params1; // sun.xyz, bloom_intensity
    float4 params2; // radiance.xyz, taa_alpha
    float4 params3; // ssao_radius, unused...
};

[[vk::push_constant]] ConstantBuffer<FrameConstants> frame : register(b0);

[[vk::binding(0, 0)]] Texture2D<float4> albedo_tex : register(t0);
[[vk::binding(1, 0)]] Texture2D<float4> normal_tex : register(t1);
[[vk::binding(2, 0)]] Texture2D<float4> orm_tex : register(t2);
[[vk::binding(3, 0)]] Texture2D<float4> ao_tex : register(t3);
[[vk::binding(4, 0)]] SamplerState samp : register(s0);

static const float PI = 3.141592653589793;

struct VSOut {
    float4 position : SV_Position;
};

float3 oct_decode(float2 e) {
    float3 n = float3(e.x, e.y, 1.0 - abs(e.x) - abs(e.y));
    if (n.z < 0.0) {
        n.xy = (1.0 - abs(n.yx)) * float2(n.x >= 0.0 ? 1.0 : -1.0, n.y >= 0.0 ? 1.0 : -1.0);
    }
    return normalize(n);
}

float distribution_ggx(float n_dot_h, float roughness) {
    float a = clamp(roughness, 0.04, 1.0);
    float a2 = a * a;
    float d = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    return a2 / max(PI * d * d, 1e-7);
}

float visibility_smith(float n_dot_v, float n_dot_l, float roughness) {
    float a = clamp(roughness, 0.04, 1.0);
    float a2 = a * a;
    float gv = n_dot_l * sqrt(n_dot_v * n_dot_v * (1.0 - a2) + a2);
    float gl = n_dot_v * sqrt(n_dot_l * n_dot_l * (1.0 - a2) + a2);
    return 0.5 / max(gv + gl, 1e-5);
}

float3 fresnel_schlick(float cos_theta, float3 f0) {
    float factor = pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
    return f0 + (1.0 - f0) * factor;
}

float4 fs_main(VSOut input) : SV_Target {
    float2 uv = input.position.xy * frame.params0.xy;
    float4 albedo = albedo_tex.SampleLevel(samp, uv, 0.0);
    float4 normal_sample = normal_tex.SampleLevel(samp, uv, 0.0);
    float4 orm = orm_tex.SampleLevel(samp, uv, 0.0);
    float ao = ao_tex.SampleLevel(samp, uv, 0.0).r;

    float3 n = oct_decode(normal_sample.xy);
    float3 v = float3(0.0, 0.0, 1.0);
    float3 l = normalize(frame.params1.xyz);
    float3 radiance = frame.params2.xyz;
    float metallic = orm.b;
    float roughness = clamp(orm.g, 0.04, 1.0);
    float3 base_color = albedo.rgb;

    float n_dot_l = max(dot(n, l), 0.0);
    if (n_dot_l <= 0.0) {
        return float4(0.0, 0.0, 0.0, 1.0);
    }
    float n_dot_v = max(dot(n, v), 1e-4);
    float3 h = normalize(v + l);
    float n_dot_h = max(dot(n, h), 0.0);
    float v_dot_h = max(dot(v, h), 0.0);

    float3 f0 = lerp(0.04, base_color, metallic);
    float3 fresnel = fresnel_schlick(v_dot_h, f0);
    float3 multiscatter = 1.0 + f0 * (1.0 / max(fresnel, 1e-4) - 1.0) * (1.0 - roughness);
    float3 specular = distribution_ggx(n_dot_h, roughness)
        * visibility_smith(n_dot_v, n_dot_l, roughness)
        * fresnel
        * multiscatter;
    float3 diffuse = (1.0 - fresnel) * (1.0 - metallic) * base_color * (1.0 / PI);
    float3 color = (diffuse + specular) * radiance * n_dot_l * ao;
    return float4(color, 1.0);
}
