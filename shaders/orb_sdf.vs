#version 330

// Vertex shader for the fullscreen SDF orb quad.
// raylib supplies vertices in pixel coordinates and provides an `mvp`
// matrix uniform that converts them into clip space. Skipping the mvp
// transform (as a previous version did) silently drew the quad off
// screen, leaving the SDF body invisible.

in vec3 vertexPosition;
in vec2 vertexTexCoord;
in vec4 vertexColor;

uniform mat4 mvp;

out vec2 fragTexCoord;
out vec4 fragColor;

void main() {
    fragTexCoord = vertexTexCoord;
    fragColor = vertexColor;
    gl_Position = mvp * vec4(vertexPosition, 1.0);
}
