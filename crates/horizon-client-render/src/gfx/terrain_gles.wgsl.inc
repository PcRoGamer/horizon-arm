// Legacy GLES sprite selection; included into terrain.wgsl, not a standalone shader.
fn sprite_grad(uv: vec2<f32>, layer: u32, dx: vec2<f32>, dy: vec2<f32>) -> vec4<f32> {
    let l = i32(layer % SPRITE_SPLIT);
    let array_index = layer / SPRITE_SPLIT;
    if SPRITE_ARRAYS == 1u { return textureSampleGrad(sprites, sprite_sampler, uv, l, dx, dy); }
    if array_index == 0u { return textureSampleGrad(sprites, sprite_sampler, uv, l, dx, dy); }
    if array_index == 1u { return textureSampleGrad(sprites_1, sprite_sampler, uv, l, dx, dy); }
    if array_index == 2u { return textureSampleGrad(sprites_2, sprite_sampler, uv, l, dx, dy); }
    if array_index == 3u { return textureSampleGrad(sprites_3, sprite_sampler, uv, l, dx, dy); }
    if array_index == 4u { return textureSampleGrad(sprites_4, sprite_sampler, uv, l, dx, dy); }
    if array_index == 5u { return textureSampleGrad(sprites_5, sprite_sampler, uv, l, dx, dy); }
    if array_index == 6u { return textureSampleGrad(sprites_6, sprite_sampler, uv, l, dx, dy); }
    return textureSampleGrad(sprites_7, sprite_sampler, uv, l, dx, dy);
}
fn sprite_level(uv: vec2<f32>, layer: u32, lod: f32) -> vec4<f32> {
    let l = i32(layer % SPRITE_SPLIT);
    let array_index = layer / SPRITE_SPLIT;
    if SPRITE_ARRAYS == 1u { return textureSampleLevel(sprites, sprite_sampler, uv, l, lod); }
    if array_index == 0u { return textureSampleLevel(sprites, sprite_sampler, uv, l, lod); }
    if array_index == 1u { return textureSampleLevel(sprites_1, sprite_sampler, uv, l, lod); }
    if array_index == 2u { return textureSampleLevel(sprites_2, sprite_sampler, uv, l, lod); }
    if array_index == 3u { return textureSampleLevel(sprites_3, sprite_sampler, uv, l, lod); }
    if array_index == 4u { return textureSampleLevel(sprites_4, sprite_sampler, uv, l, lod); }
    if array_index == 5u { return textureSampleLevel(sprites_5, sprite_sampler, uv, l, lod); }
    if array_index == 6u { return textureSampleLevel(sprites_6, sprite_sampler, uv, l, lod); }
    return textureSampleLevel(sprites_7, sprite_sampler, uv, l, lod);
}
