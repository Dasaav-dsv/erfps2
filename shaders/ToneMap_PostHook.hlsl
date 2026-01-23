Texture2D<float4> g_SourceTexture : register(t0);

cbuffer cbPostProcessCommon : register(b4)
{
    float2 g_dynamicScreenPercentage;
    float2 g_texSizeReciprocal;
    float2 g_dynamicScreenPercentage_Primary;
    float2 g_primaryTexSizeReciprocal;
    float2 g_dynamicScreenPercentage_Prev;
    float2 g_prevTexSizeReciprocal;
    float2 g_dynamicScreenPercentage_PrevPrimary;
    float2 g_prevPrimaryTexSizeReciprocal;
};

cbuffer cbToneMap : register(b1)
{
    float3 g_ToneMapInvSceneLumScale;
    int g_ErfpsFlags;
    float4 g_ReinhardParam;
    float4 g_ToneMapParam;
    float4 g_ToneMapSceneLumScale;
    float4 g_AdaptParam;
    float4 g_AdaptCenterWeight;
    float4 g_BrightPassThreshold;
    float4 g_GlareLuminance;
    float4 g_BloomBoostColor;
    float4 g_vBloomFinalColor;
    float4 g_vBloomScaleParam;
    float4x3 g_mtxColorMultiplyer;
    float4 g_vChromaticAberrationRG;
    float2 g_vChromaticAberrationB;
    float2 g_ErfpsCorrectParam;
    int4 g_bEnableFlags;
    float4 g_vFeedBackBlurParam;
    float4 g_vVignettingParam;
    float4 g_vHDRDisplayParam;
    float4 g_vChromaticAberrationShapeParam;
    float4 g_vScreenSize;
    float4 g_vSampleDistanceAdjust;
    int4 g_vMaxSampleCount;
    float4 g_vScenePreExposure;
    float2 g_vCameraParam;
    float2 g_ErfpsCrosshairScaleReciprocal;
};

SamplerState SS_ClampLinear : register(s1);

// Simple fisheye distortion shader.
float2 MapUvFisheye(float2 uv)
{
    float2 c = uv - 0.5;

    float r2 = c.x * c.x;
    float strength = g_ErfpsCorrectParam.x;

    float f = 1.0 + strength * sqrt(r2) * r2;
    float fMax = 1.0 + strength * 0.125;

    return c * f / fMax + 0.5;
}

// Source: https://www.decarpentier.nl/lens-distortion
//
// Copyright (c) 2015, Giliam de Carpentier
// All rights reserved.
// 
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met:
// 
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer.
// 
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the 
// documentation and/or other materials provided with the distribution.
// 
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED 
// TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR 
// CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, 
// PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF 
// LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS 
// SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
float2 MapUvBarrel(float2 uv)
{
    float aspectRatio = g_vCameraParam.x;
    float cylindricalRatio = g_ErfpsCorrectParam.x;

    float scaledHeight = g_ErfpsCorrectParam.y * g_vCameraParam.y;
    float cylAspectRatio = aspectRatio * cylindricalRatio;
    float aspectDiagSq = aspectRatio * aspectRatio + 1.0;
    float diagSq = scaledHeight * scaledHeight * aspectDiagSq;
    float2 signedUV = 2.0 * uv - 1.0;

    float z = 0.5 * sqrt(diagSq + 1.0) + 0.5;
    float ny = (z - 1.0) / (cylAspectRatio * cylAspectRatio + 1.0);

    float2 vUVDot = sqrt(ny) * float2(cylAspectRatio, 1.0) * signedUV;
    float3 vUV = float3(0.5, 0.5, 1.0) * z + float3(-0.5, -0.5, 0.0);
    vUV.xy += uv;

    float3 uvp = vUV - dot(vUVDot, vUVDot) * float3(0.5, 0.5, 1.0);
    return uvp.xy / uvp.z;
}

bool CrosshairTest(float2 uv)
{
    float2 c = (uv - 0.5) * g_ErfpsCrosshairScaleReciprocal;
    float2 cScreen = c * float2(g_vCameraParam.x, 1.0) * g_dynamicScreenPercentage;

    int crosshairKind = (g_ErfpsFlags >> 2) & 7;
    switch (crosshairKind)
    {
        case 1: {
            cScreen = abs(cScreen);
            return any(cScreen < 0.0014) && all(cScreen < 0.0080);
        }
        case 2: {
            float r = length(cScreen);
            return r < 0.0018;
        }
        case 3: {
            float r = length(cScreen);
            return r > 0.0066 && r < 0.0080;
        }
        case 4: {
            float r = length(cScreen);
            return (r > 0.0066 && r < 0.0080) || r < 0.0012;
        }
        case 5: {
            cScreen = abs(cScreen);
            float s = cScreen.x + cScreen.y * 3.0;
            return cScreen.x > 0.007 && s > 0.015 && s < 0.02;
        }
        default:
            return false;
    }
}

float4 PSMain(float4 position : SV_Position, float3 coord : TEXCOORD) : SV_TARGET
{
    float2 xy = coord.xy;

    if (CrosshairTest(xy)) {
        // Draw crosshair.
        float4 rgba = g_SourceTexture.SampleLevel(SS_ClampLinear, xy, 0);
        return float4((1.0 - rgba.rgb) * 0.9, rgba.a);
    }

    if (g_ErfpsFlags & 1) {
        // Apply FOV correction.
        if (g_ErfpsFlags & 2) {
            xy = MapUvBarrel(xy);
        } else {
            xy = MapUvFisheye(xy);
        }
    }

    float2 xy2m1 = xy * 2.0 - 1.0;

    float2 chromaR = g_vChromaticAberrationRG.xy;
    float2 chromaG = g_vChromaticAberrationRG.zw;
    float2 chromaB = g_vChromaticAberrationB;

    float2 dynamicScreenPercentage = g_dynamicScreenPercentage;
    float2 texSizeReciprocal = g_texSizeReciprocal;
    float2 texEdge = dynamicScreenPercentage - texSizeReciprocal * 0.5;

    float2 xy2m1ChromaR = xy2m1 * chromaR + xy;
    float2 rCoord = min(xy2m1ChromaR * dynamicScreenPercentage, texEdge);
    float r = g_SourceTexture.SampleLevel(SS_ClampLinear, rCoord, 0).r;

    float2 xy2m1ChromaG = xy2m1 * chromaG + xy;
    float2 gCoord = min(xy2m1ChromaG * dynamicScreenPercentage, texEdge);
    float g = g_SourceTexture.SampleLevel(SS_ClampLinear, gCoord, 0).g;
    
    float2 xy2m1ChromaB = xy2m1 * chromaB + xy;
    float2 bCoord = min(xy2m1ChromaB * dynamicScreenPercentage, texEdge);
    float b = g_SourceTexture.SampleLevel(SS_ClampLinear, bCoord, 0).b;

    return float4(r, g, b, 1.0);
}