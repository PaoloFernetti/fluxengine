struct VSOut {
    float4 position : SV_Position;
};

// Константный цвет: минимальный fragment для PSO-смоука.
float4 fs_main(VSOut input) : SV_Target {
    return float4(1.0, 0.5, 0.25, 1.0);
}
