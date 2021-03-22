#version 450

layout(location=0) in vec2 v_tex_coords;
layout(location=0) out vec4 f_color;

layout(set = 0, binding = 0) uniform texture2D t_diffuse;
layout(set = 0, binding = 1) uniform utexture2D t_placement;
layout(set = 0, binding = 2) uniform sampler s_diffuse;

void main() {
    uint id = texture(usampler2D(t_placement, s_diffuse), v_tex_coords).r;
    if(id == 0)
        discard;

    vec2 local_tex = fract(v_tex_coords * textureSize(usampler2D(t_placement, s_diffuse), 0)) / 8;

    f_color = texture(sampler2D(t_diffuse, s_diffuse), vec2(mod(id - 1, 8), (id - 1) / 8) / 8 + local_tex);
}