use crate::Calendar;
use std::f32::consts::TAU;

pub const ORBIT_ASPECT: f32 = 1.4;
pub const INCLINATION_MAX: f32 = 0.576;
pub const SUN_DISC: f32 = 0.090;
pub const MOON_DISC: f32 = 0.096;
pub const UMBRA_RADIUS: f32 = 0.153;
pub const ECLIPSE_WINDOW: f32 = 0.05;
pub const MOON_LIGHT_MAX: f32 = 0.5;
pub const SKYGLOW: f32 = 0.03;

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CelestialState {
    pub sun_position: [f32; 2],
    pub moon_position: [f32; 2],
    pub sun_altitude: f32,
    pub sun_direction: [f32; 2],
    pub illumination: f32,
    pub solar_occlusion: f32,
    pub lunar_shadow: f32,
    pub daylight: f32,
    pub light: f32,
}

impl CelestialState {
    pub fn is_solar_eclipse(self) -> bool {
        self.solar_occlusion > 0.5
    }

    pub fn is_lunar_eclipse(self) -> bool {
        self.lunar_shadow > 0.5
    }
}

impl Calendar {
    pub fn celestial(self) -> CelestialState {
        let sun_angle = (self.day_fraction() - 0.25) * TAU;
        let (sun_sin, sun_cos) = sun_angle.sin_cos();
        let sun_position = [sun_cos * ORBIT_ASPECT, sun_sin];

        let moon_angle = sun_angle - self.elongation();
        let (moon_sin, moon_cos) = moon_angle.sin_cos();
        let inclination_offset = INCLINATION_MAX * self.ecliptic_latitude();
        let moon_position = [
            moon_cos * (ORBIT_ASPECT + inclination_offset),
            moon_sin * (1.0 + inclination_offset),
        ];

        let synodic = self.synodic_fraction();
        let newness = (1.0 - synodic.min(1.0 - synodic) / ECLIPSE_WINDOW).clamp(0.0, 1.0);
        let fullness = (1.0 - (synodic - 0.5).abs() / ECLIPSE_WINDOW).clamp(0.0, 1.0);

        let separation_x = moon_position[0] - sun_position[0];
        let separation_y = moon_position[1] - sun_position[1];
        let separation = (separation_x * separation_x + separation_y * separation_y).sqrt();
        let overlap = (1.0 - separation / (SUN_DISC + MOON_DISC)).clamp(0.0, 1.0);
        let solar_occlusion = overlap * newness;

        let antisolar_x = moon_position[0] + sun_position[0];
        let antisolar_y = moon_position[1] + sun_position[1];
        let shadow_separation = (antisolar_x * antisolar_x + antisolar_y * antisolar_y).sqrt();
        let lunar_shadow =
            (1.0 - shadow_separation / (UMBRA_RADIUS + MOON_DISC)).clamp(0.0, 1.0) * fullness;

        let illumination = self.moon_illumination();
        let daylight = smoothstep(-0.12, 0.10, sun_sin);
        let sunlight = daylight * (1.0 - solar_occlusion);
        let moon_above_horizon = smoothstep(-0.10, 0.10, moon_position[1].clamp(-1.0, 1.0));
        let moonlight = illumination * moon_above_horizon * (1.0 - lunar_shadow) * MOON_LIGHT_MAX;
        let light = sunlight.max(moonlight + SKYGLOW).clamp(0.0, 1.0);

        let to_sun_x = sun_position[0] - moon_position[0];
        let to_sun_y = sun_position[1] - moon_position[1];
        let to_sun_length = (to_sun_x * to_sun_x + to_sun_y * to_sun_y).sqrt();
        let sun_direction = if to_sun_length > 1e-6 {
            [to_sun_x / to_sun_length, to_sun_y / to_sun_length]
        } else {
            [0.0, 0.0]
        };

        CelestialState {
            sun_position,
            moon_position,
            sun_altitude: sun_sin,
            sun_direction,
            illumination,
            solar_occlusion,
            lunar_shadow,
            daylight,
            light,
        }
    }
}
