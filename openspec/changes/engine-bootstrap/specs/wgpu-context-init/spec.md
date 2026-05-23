## ADDED Requirements

### Requirement: wgpu context initialization
The system SHALL create a wgpu Instance, Surface, Adapter, Device, and Queue on startup.

#### Scenario: Successful GPU initialization
- **WHEN** the example application starts
- **THEN** wgpu selects a suitable adapter (preferring HighPerformance)
- **AND** a Device and Queue are created with default limits
- **AND** the Surface is configured with sRGB format and FIFO present mode

### Requirement: Surface resize handling
The system SHALL reconfigure the wgpu Surface when the window is resized.

#### Scenario: Window resize
- **WHEN** the window is resized by the user
- **THEN** the Surface configuration is updated with new dimensions
- **AND** rendering continues without crash or error
