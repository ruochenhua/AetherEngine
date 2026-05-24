## ADDED Requirements

### Requirement: Triangle vertex buffer upload
The system SHALL create a GPU vertex buffer containing three colored vertices.

#### Scenario: Vertex buffer creation
- **WHEN** the example initializes
- **THEN** a vertex buffer is created with 3 vertices
- **AND** each vertex has position (vec3) and color (vec3) attributes

### Requirement: Render pipeline creation
The system SHALL create a render pipeline with the triangle shaders.

#### Scenario: Pipeline compilation
- **WHEN** the example initializes
- **THEN** a render pipeline is compiled from the triangle vertex and fragment shaders
- **AND** the pipeline uses the correct vertex buffer layout

### Requirement: Triangle rendering
The system SHALL render the triangle to the swap chain each frame.

#### Scenario: Frame rendering
- **WHEN** each frame is rendered
- **THEN** the triangle is drawn with a color gradient (red-green-blue)
- **AND** the output is presented to the swap chain

### Requirement: Render pass encoder
The system SHALL begin a render pass targeting the swap chain texture.

#### Scenario: Render pass execution
- **WHEN** rendering a frame
- **THEN** a render pass is begun with the current swap chain texture view
- **AND** the pass clears the screen to a dark color
- **AND** the triangle draw command is recorded
