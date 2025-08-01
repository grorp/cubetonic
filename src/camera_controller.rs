use cgmath::{InnerSpace, Rotation3};
use winit::event::{DeviceEvent, ElementState, KeyEvent, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

use crate::camera;

pub struct CameraController {
    rotation_sensitivity: f32,
    movement_speed: f32,

    yaw: f32,
    pitch: f32,

    forward: bool,
    backward: bool,
    right: bool,
    left: bool,

    up: bool,
    down: bool,
}

impl CameraController {
    pub fn new() -> CameraController {
        CameraController {
            rotation_sensitivity: 0.1,
            movement_speed: 0.1,

            yaw: 0.0,
            pitch: 0.0,

            forward: false,
            backward: false,
            right: false,
            left: false,

            up: false,
            down: false,
        }
    }

    pub fn process_window_event(&mut self, event: &WindowEvent) -> bool {
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
                    KeyCode::Space => {
                        self.up = pressed;
                        true
                    }
                    KeyCode::ShiftLeft | KeyCode::ShiftRight => {
                        self.down = pressed;
                        true
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }

    pub fn process_device_event(&mut self, event: &DeviceEvent) -> bool {
        match event {
            DeviceEvent::MouseMotion { delta } => {
                // TODO: yaw/pitch are inverted(?)
                self.yaw -= delta.0 as f32 * self.rotation_sensitivity;
                self.pitch += delta.1 as f32 * self.rotation_sensitivity;

                // don't allow the camera to flip over :)
                self.pitch = self.pitch.clamp(-90.0, 90.0);

                true
            }
            _ => false,
        }
    }

    pub fn update_camera(&self, camera: &mut camera::Camera, dtime: f32) {
        let rot_yaw = cgmath::Quaternion::from_angle_y(cgmath::Deg(self.yaw));
        let rot_pitch = cgmath::Quaternion::from_angle_x(cgmath::Deg(self.pitch));

        camera.params.dir = rot_yaw * rot_pitch * cgmath::Vector3::unit_z();

        let mut movement = cgmath::Vector3::new(0.0, 0.0, 0.0);

        if self.forward {
            movement.z += 1.0;
        }
        if self.backward {
            movement.z -= 1.0;
        }
        // TODO: x axis is inverted
        if self.right {
            movement.x -= 1.0;
        }
        if self.left {
            movement.x += 1.0;
        }
        // avoids NaN from normalize
        if movement.magnitude2() != 0.0 {
            movement = rot_yaw * movement.normalize();
        }

        if self.up {
            movement.y += 1.0;
        }
        if self.down {
            movement.y -= 1.0;
        }

        movement = movement * self.movement_speed * dtime;
        camera.params.pos += movement;

        println!(
            "[CameraController] pos: ({:.1}, {:.1}, {:.1}) yaw: {:.1} pitch: {:.1}",
            camera.params.pos.x, camera.params.pos.y, camera.params.pos.z, self.yaw, self.pitch
        );
    }
}
