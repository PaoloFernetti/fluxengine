struct VSOut {
    float4 position : SV_Position;
};

// Полноэкранный треугольник без vertex buffer (layout v1, ADR-0035).
VSOut vs_main(uint id : SV_VertexID) {
    VSOut output;
    float2 p = float2((id << 1) & 2, id & 2);
    output.position = float4(p * 2.0 - 1.0, 0.0, 1.0);
    return output;
}
