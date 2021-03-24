#version 450

layout(location=0) in vec2 v_tex_coords;
layout(location=0) out vec4 f_color;

layout(set = 0, binding = 0) uniform texture2DArray t_diffuse_array;
layout(set = 0, binding = 1) uniform utexture2D t_placement;
layout(set = 0, binding = 2) uniform sampler s_diffuse;

void main() {
    ivec2 tex_size = textureSize(usampler2D(t_placement, s_diffuse), 0);
    uint id = texelFetch(usampler2D(t_placement, s_diffuse), ivec2(tex_size * v_tex_coords), 0).r;
    if(id == 0)
        discard;

    vec2 local_tex = fract(v_tex_coords * tex_size);

    f_color = texture(sampler2DArray(t_diffuse_array, s_diffuse), vec3(local_tex, float(id - 1)));
}