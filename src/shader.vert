#version 450

layout(location=0) in vec2 a_position;
layout(location=1) in vec3 a_color;

layout(location=0) out vec3 v_color;

layout(set=0, binding=0) // 1.
uniform Uniforms {
    mat4 cam; // 2.
};


void main() {
    v_color = a_color;
    gl_Position = cam * vec4(a_position, 0.0, 1.0);
}