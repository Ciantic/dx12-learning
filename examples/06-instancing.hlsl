
cbuffer SceneBuffer : register(b0)
{
    float4x4 proj;
    float4x4 view;
};

// In this example, this is not used, as instancing data have it already
cbuffer ObjectBuffer : register(b1)
{
    float4x4 world;
};

struct PSInput
{
    float4 position : SV_POSITION;
    float4 color : COLOR;
};

struct InstanceData {
    float4x4 world;
};

StructuredBuffer<InstanceData> instance_data : register(t0, space1);

PSInput VSMain(float4 position : POSITION, float4 color : COLOR, uint instance_id : SV_InstanceID)
{
    PSInput result;

    result.position = position;
    result.position = mul(result.position, instance_data[instance_id].world);
    result.position = mul(result.position, view);
    result.position = mul(result.position, proj);

    result.color = color;
    return result;
}

float4 PSMain(PSInput input) : SV_TARGET
{
    return input.color;
}