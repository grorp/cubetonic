use cgmath::InnerSpace;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

use crate::camera;

pub struct CameraController {
    forward: bool,
    backward: bool,
    right: bool,
    left: bool,
}

impl CameraController {
    pub fn new() -> CameraController {
        CameraController {
            forward: false,
            backward: false,
            right: false,
            left: false,
        }
    }

    pub fn process_event(&mut self, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        state,
                        physical_key: PhysicalKey::Code(keycode),
                        ..
                    },
                ..
            } => {
                let pressed = *state == ElementState::Pressed;
                match keycode {
                    KeyCode::KeyW => {
                        self.forward = pressed;
                        true
                    }
                    KeyCode::KeyS => {
                        self.backward = pressed;
                        true
                    }
                    KeyCode::KeyD => {
                        self.right = pressed;
                        true
                    }
                    KeyCode::KeyA => {
                        self.left = pressed;
                        true
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }

    pub fn update_camera(&self, camera: &mut camera::Camera, speed: f32) {
        let mut movement = cgmath::Vector3::new(0.0, 0.0, 0.0);
        if self.forward {
            movement.z += 1.0;
        }
        if self.backward {
            movement.z -= 1.0;
        }
        if self.right {
            movement.x += 1.0;
        }
        if self.left {
            movement.x -= 1.0;
        }

        if movement.magnitude2() == 0.0 {
            return;
        }

        movement = movement.normalize() * speed;
        camera.params.pos += movement;
    }
}
