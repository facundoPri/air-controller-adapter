//! Optional visualization driven by relative HID deltas, not absolute orientation.
use super::remote_scene::Camera;

#[derive(Default)]
pub(super) struct FollowMovement {
    target: Option<Camera>,
}

impl FollowMovement {
    pub fn enabled(&self) -> bool {
        self.target.is_some()
    }

    pub fn toggle(&mut self, camera: Camera) {
        self.target = if self.enabled() { None } else { Some(camera) };
    }

    pub fn stop(&mut self) {
        self.target = None;
    }

    pub fn reset(&mut self) -> Camera {
        let camera = Camera::default();
        if self.enabled() {
            self.target = Some(camera);
        }
        camera
    }

    /// Called on the existing 50 ms UI poll. Clamp the target before easing so
    /// movement at the bounds cannot build up invisible rotational debt.
    pub fn step(&mut self, camera: Camera, motion: (i32, i32), active: bool) -> Camera {
        let Some(target) = &mut self.target else {
            return camera;
        };
        if !active {
            *target = camera;
            return camera;
        }
        // Orbiting the camera turns the visible model in the opposite direction.
        *target = target.rotated(-(motion.0 as f32) * 0.004, motion.1 as f32 * 0.004);
        let ease = |current: f32, goal: f32| {
            if (goal - current).abs() < 0.001 {
                goal
            } else {
                current + (goal - current) * 0.35
            }
        };
        Camera {
            yaw: ease(camera.yaw, target.yaw),
            pitch: ease(camera.pitch, target.pitch),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follow_is_opt_in_and_stops_without_replaying_motion() {
        let start = Camera::default();
        let mut follow = FollowMovement::default();
        assert_eq!(follow.step(start, (40, 20), true), start);
        follow.toggle(start);
        let moved = follow.step(start, (40, 20), true);
        assert!(moved.yaw < start.yaw && moved.pitch > start.pitch);
        assert_eq!(follow.step(moved, (90, 90), false), moved);
        assert_eq!(follow.step(moved, (0, 0), true), moved);
        follow.toggle(moved);
        assert_eq!(follow.step(moved, (40, 20), true), moved);
    }

    #[test]
    fn reset_recenters_and_manual_rotation_cancels_following() {
        let start = Camera::default();
        let mut follow = FollowMovement::default();
        follow.toggle(start);
        follow.step(start, (100, 100), true);
        assert_eq!(follow.reset(), start);
        assert!(follow.enabled());
        assert_eq!(follow.step(start, (0, 0), true), start);
        follow.stop();
        let manual = start.rotated(-0.5, -0.3);
        assert_eq!(follow.step(manual, (100, 100), true), manual);
        assert!(!follow.enabled());
    }

    #[test]
    fn large_movements_stay_visible_and_reverse_without_rotational_debt() {
        let mut follow = FollowMovement::default();
        let mut camera = Camera::default();
        follow.toggle(camera);
        for _ in 0..100 {
            camera = follow.step(camera, (i32::MAX, i32::MAX), true);
            assert!((-1.3..=1.3).contains(&camera.yaw) && (0.05..=1.2).contains(&camera.pitch));
        }
        let reversed = follow.step(camera, (-40, -40), true);
        assert!(reversed.yaw > camera.yaw && reversed.pitch < camera.pitch);
        for _ in 0..100 {
            camera = follow.step(camera, (i32::MIN, i32::MIN), true);
            assert!((-1.3..=1.3).contains(&camera.yaw) && (0.05..=1.2).contains(&camera.pitch));
        }
    }
}
