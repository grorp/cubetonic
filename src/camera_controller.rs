use winit::event::{DeviceEvent, ElementState, KeyEvent, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

use crate::camera;

pub struct CameraController {
    rotation_sensitivity: f32,
    movement_speed: f32,

    pub yaw: f32,
    pub pitch: f32,

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
            movement_speed: 20.0,

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
                // yaw is flipped compared to how it is applied in the
                // "event handler" by Luanti, but Luanti actually flips it
                // later in camera.cpp
                self.yaw += delta.0 as f32 * self.rotation_sensitivity;
                self.pitch += delta.1 as f32 * self.rotation_sensitivity;

                // don't allow the camera to flip over :)
                self.pitch = self.pitch.clamp(-90.0, 90.0);

                true
            }
            _ => false,
        }
    }

    pub fn update_camera(&self, params: &mut camera::CameraParams, dtime: f32) {
        let rot_yaw = glam::Quat::from_rotation_y(self.yaw.to_radians());
        let rot_pitch = glam::Quat::from_rotation_x(self.pitch.to_radians());

        params.dir = rot_yaw * rot_pitch * glam::Vec3::Z;

        let mut movement = glam::Vec3::ZERO;

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
        // avoids NaN from normalize
        if movement.length_squared() != 0.0 {
            movement = rot_yaw * movement.normalize();
        }

        if self.up {
            movement.y += 1.0;
        }
        if self.down {
            movement.y -= 1.0;
        }

        movement = movement * self.movement_speed * dtime;
        params.pos += movement;

        /*
        println!(
            "[CameraController] dtime: {:.4} pos: ({:.1}, {:.1}, {:.1}) dir: ({:.1}, {:.1}, {:.1}) yaw: {:.1} pitch: {:.1}",
            dtime,
            params.pos.x,
            params.pos.y,
            params.pos.z,
            params.dir.x,
            params.dir.y,
            params.dir.z,
            self.yaw,
            self.pitch
        );
        */
        println!("dtime: {:.4}", dtime);
    }
}
