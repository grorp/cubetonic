// This is https://learnopengl.com/Guest-Articles/2021/Scene/Frustum-Culling

use glam::Vec3;

use crate::camera::CameraParams;

pub struct Plane {
    normal: Vec3,
    // distance from origin to the nearest point in the plane
    distance: f32,
}

impl Plane {
    pub fn new(p1: Vec3, normal: Vec3) -> Self {
        let normal = normal.normalize();
        Self {
            normal,
            distance: normal.dot(p1),
        }
    }

    pub fn get_signed_distance_to_plane(&self, point: Vec3) -> f32 {
        self.normal.dot(point) - self.distance
    }
}

pub struct Frustum {
    top_face: Plane,
    bottom_face: Plane,

    right_face: Plane,
    left_face: Plane,

    far_face: Plane,
    near_face: Plane,
}

impl Frustum {
    pub fn new(params: &CameraParams) -> Self {
        let right = params.dir.cross(Vec3::Y).normalize();
        let up = right.cross(params.dir).normalize();

        let half_v_side = params.z_far * (params.fov_y * 0.5).tan();
        let aspect = params.size.width as f32 / params.size.height as f32;
        let half_h_side = half_v_side * aspect;
        let front_mult_far = params.z_far * params.dir;

        Self {
            near_face: Plane::new(params.pos + params.z_near * params.dir, params.dir),
            far_face: Plane::new(params.pos + front_mult_far, -params.dir),

            right_face: Plane::new(params.pos, (front_mult_far - right * half_h_side).cross(up)),
            left_face: Plane::new(params.pos, up.cross(front_mult_far + right * half_h_side)),

            top_face: Plane::new(params.pos, right.cross(front_mult_far - up * half_v_side)),
            bottom_face: Plane::new(params.pos, (front_mult_far + up * half_v_side).cross(right)),
        }
    }
}

pub struct BoundingSphere {
    pub center: Vec3,
    pub radius: f32,
}

impl BoundingSphere {
    pub fn is_on_or_forward_plane(&self, plane: &Plane) -> bool {
        return plane.get_signed_distance_to_plane(self.center) > -self.radius;
    }

    pub fn is_on_frustum(&self, frustum: &Frustum) -> bool {
        self.is_on_or_forward_plane(&frustum.left_face)
            && self.is_on_or_forward_plane(&frustum.right_face)
            && self.is_on_or_forward_plane(&frustum.far_face)
            && self.is_on_or_forward_plane(&frustum.near_face)
            && self.is_on_or_forward_plane(&frustum.top_face)
            && self.is_on_or_forward_plane(&frustum.bottom_face)
    }
}
