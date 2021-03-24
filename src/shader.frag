#version 450

#extension GL_EXT_samplerless_texture_functions : require

layout(location=0) in vec2 v_tex_coords;
layout(location=0) out vec4 f_color;

layout(set = 0, binding = 0) uniform texture2DArray t_diffuse_array;
layout(set = 0, binding = 1) uniform utexture2D t_placement;
layout(set = 0, binding = 2) uniform sampler s_diffuse;

void main() {
    vec2 scaledTexCoord =  v_tex_coords * textureSize(t_placement, 0);
    uint id = texelFetch(t_placement, ivec2(scaledTexCoord), 0).r;
    if(id == 0)
        discard;

    vec2 dx = dFdx(scaledTexCoord);
    vec2 dy = dFdy(scaledTexCoord);
    vec2 local_tex = fract(scaledTexCoord);

    f_color = textureGrad(sampler2DArray(t_diffuse_array, s_diffuse), vec3(local_tex, float(id - 1)), dx, dy);
}