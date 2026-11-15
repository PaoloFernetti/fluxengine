// Тени (ADR-0050): vertex pulling, camera-relative light space.
// Инстансы уже camera-relative, поэтому light view-projection строится
// вокруг начала координат (камеры) и не зависит от мировых координат.

struct Vertex {
    float3 position;
    float3 normal;
};

struct Instance {
    float3 position;
    float scale;
    float3 albedo;
    float roughness;
    float metallic;
    float3 padding;
};

[[vk::binding(0, 0)]] StructuredBuffer<Vertex> vertices : register(t0);
[[vk::binding(1, 0)]] StructuredBuffer<Instance> instances : register(t1);

struct ShadowConstants {
    float4x4 light_view_projection;
};

[[vk::push_constant]] ConstantBuffer<ShadowConstants> frame : register(b0);

float4 vs_main(uint vertex_id : SV_VertexID, uint instance_id : SV_InstanceID) : SV_Position {
    Vertex vertex = vertices[vertex_id];
    Instance instance = instances[instance_id];
    float3 world = vertex.position * instance.scale + instance.position;
    return mul(frame.light_view_projection, float4(world, 1.0));
}
