cbuffer SceneBuffer : register(b0)
{
    float4x4 proj;
    float4x4 view;
};

cbuffer ObjectBuffer : register(b1)
{
    float4x4 world;
};

struct PSInput
{
    float4 position : SV_POSITION;
    float4 color : COLOR;
};

PSInput VSMain(float4 position : POSITION, float4 color : COLOR)
{
    PSInput result;

    result.position = position;
    result.position = mul(result.position, world);
    result.position = mul(result.position, view);
    result.position = mul(result.position, proj);

    result.color = color;
    return result;
}

float4 PSMain(PSInput input) : SV_TARGET
{
    return input.color;
}