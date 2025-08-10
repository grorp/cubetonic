use glam::Vec3;
use winit::event::{DeviceEvent, ElementState, KeyEvent, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

use crate::camera::CameraParams;

#[derive(Default, Debug, Clone)]
pub struct PlayerPos {
    pub pos: Vec3,
    // Yaw is stored inverted compared to Luanti. Luanti actually inverts it when
    // it is applied, e.g. in camera.cpp. This means we have to invert yaw values
    // when sending to and receiving from the network, and when handling mouse
    // input.
    pub yaw: f32,
    pub pitch: f32,
}

pub struct CameraController {
    // The CameraController is the source of truth for this data
    pos: PlayerPos,

    rotation_sensitivity: f32,
    movement_speed: f32,

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
            pos: PlayerPos::default(),

            rotation_sensitivity: 0.1,
            movement_speed: 20.0,

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
                self.pos.yaw += delta.0 as f32 * self.rotation_sensitivity;
                self.pos.pitch += delta.1 as f32 * self.rotation_sensitivity;

                // don't allow the camera to flip over :)
                // 89 instead of 90 so the forward/up vectors don't end up being parallel
                // (would cause flashing)
                self.pos.pitch = self.pos.pitch.clamp(-89.0, 89.0);

                true
            }
            _ => false,
        }
    }

    pub fn set_pos(&mut self, pos: PlayerPos) {
        self.pos = pos;
    }

    pub fn get_pos(&self) -> &PlayerPos {
        &self.pos
    }

    pub fn step(&mut self, dtime: f32, params: &mut CameraParams) {
        let rot_yaw = glam::Quat::from_rotation_y(self.pos.yaw.to_radians());
        let rot_pitch = glam::Quat::from_rotation_x(self.pos.pitch.to_radians());

        params.dir = rot_yaw * rot_pitch * CameraParams::WORLD_FORWARD;

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
        self.pos.pos += movement;

        params.pos = self.pos.pos;

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
            self.pos.yaw,
            self.pos.pitch
        );
        */
        // println!("dtime: {:.4}", dtime);
    }
}
