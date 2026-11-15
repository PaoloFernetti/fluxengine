// GPU-геометрия сцены (4.5, ADR-0047): vertex pulling.
//
// Вершины и инстансы читаются из storage-буферов по SV_VertexID /
// SV_InstanceID — vertex buffer не нужен. Инстансы camera-relative:
// позиция уже в пространстве камеры, view-матрица — только поворот.

struct SceneVertex {
    float3 position;
    float3 normal;
};

struct SceneInstance {
    float3 position;
    float scale;
    float3 albedo;
    float roughness;
    float metallic;
    float3 padding;
};

[[vk::binding(0, 0)]] StructuredBuffer<SceneVertex> vertices : register(t0);
[[vk::binding(1, 0)]] StructuredBuffer<SceneInstance> instances : register(t1);

struct SceneConstants {
    float4x4 view_projection;
};

[[vk::push_constant]] ConstantBuffer<SceneConstants> frame : register(b0);

struct VSOut {
    float4 position : SV_Position;
    float3 normal : NORMAL;
    float3 albedo : COLOR0;
    float2 material : TEXCOORD0; // roughness, metallic
};

VSOut vs_main(uint vertex_id : SV_VertexID, uint instance_id : SV_InstanceID) {
    SceneVertex vertex = vertices[vertex_id];
    SceneInstance instance = instances[instance_id];

    float3 world = instance.position + vertex.position * instance.scale;

    VSOut output;
    output.position = mul(frame.view_projection, float4(world, 1.0));
    output.normal = vertex.normal;
    output.albedo = instance.albedo;
    output.material = float2(instance.roughness, instance.metallic);
    return output;
}
