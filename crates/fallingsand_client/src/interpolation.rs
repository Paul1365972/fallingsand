use bevy::prelude::*;
use fallingsand_core::TICK_RATE;

const BLEND_RATE: f32 = TICK_RATE as f32;
const BLEND_CARRY_DAMP: f32 = 0.9;
const BLEND_MAX: f32 = 2.0;

pub struct InterpolationPlugin;

impl Plugin for InterpolationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, interpolate);
    }
}

#[derive(Component)]
pub struct Interpolated {
    previous: (Vec2, f32),
    target: (Vec2, f32),
    blend: f32,
}

impl Interpolated {
    pub fn snapped(position: Vec2, angle: f32) -> Self {
        Self {
            previous: (position, angle),
            target: (position, angle),
            blend: 1.0,
        }
    }

    pub fn target_position(&self) -> Vec2 {
        self.target.0
    }

    pub fn record(&mut self, position: Vec2, angle: f32, snap: bool) {
        if snap {
            self.previous = (position, angle);
            self.blend = 1.0;
        } else {
            self.previous = self.target;
            self.blend = ((self.blend - 1.0) * BLEND_CARRY_DAMP).max(-1.0);
        }
        self.target = (position, angle);
    }
}

pub fn interpolate(time: Res<Time>, mut query: Query<(&mut Interpolated, &mut Transform)>) {
    for (mut interp, mut transform) in &mut query {
        interp.blend = (interp.blend + time.delta_secs() * BLEND_RATE).min(BLEND_MAX);
        let alpha = interp.blend.clamp(0.0, 1.0);
        let position = interp.previous.0.lerp(interp.target.0, alpha);
        let angle = interp.previous.1 + (interp.target.1 - interp.previous.1) * alpha;
        transform.translation.x = position.x;
        transform.translation.y = position.y;
        transform.rotation = Quat::from_rotation_z(angle);
    }
}
