use crate::Calendar;
use std::f32::consts::{PI, TAU};

const SUN_DISC_RADIUS: f32 = 0.090;
const MOON_DISC_RADIUS: f32 = 0.096;
pub const SHADE_DISC_RADIUS: f32 = 0.125;
const SHADE_TRACK: f32 = 1.06;
const TRACK_FLATTEN: f32 = 0.62;
const HUB_AMPLITUDE: f32 = 0.30;
const MOON_ECCENTRE: f32 = 0.48;
const MOON_BREATH: f32 = 0.2;
const MOON_LIGHT_MAX: f32 = 0.5;
const SKYGLOW: f32 = 0.03;
const STARS_BEGIN: f32 = -0.0175;
const STARS_FULL: f32 = -0.2756;
const STARS_LIGHT_BEGIN: f32 = 0.30;
const STARS_LIGHT_FULL: f32 = 0.08;
const MOON_STAR_WASH: f32 = 0.6;
const SEASON_PHASE: f32 = 0.125;

pub fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn covered_fraction(covered_radius: f32, cover_radius: f32, distance: f32) -> f32 {
    if distance >= covered_radius + cover_radius {
        return 0.0;
    }
    let covered_area = PI * covered_radius * covered_radius;
    if distance <= (covered_radius - cover_radius).abs() {
        let inner = covered_radius.min(cover_radius);
        return PI * inner * inner / covered_area;
    }
    let a = covered_radius * covered_radius;
    let b = cover_radius * cover_radius;
    let x = (distance * distance + a - b) / (2.0 * distance);
    let y = distance - x;
    let lens = a * (x / covered_radius).clamp(-1.0, 1.0).acos() - x * (a - x * x).max(0.0).sqrt()
        + b * (y / cover_radius).clamp(-1.0, 1.0).acos()
        - y * (b - y * y).max(0.0).sqrt();
    (lens / covered_area).clamp(0.0, 1.0)
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CelestialState {
    pub sun_position: [f32; 2],
    pub moon_position: [f32; 2],
    pub shade_position: [f32; 2],
    pub sun_altitude: f32,
    pub sun_direction: [f32; 2],
    pub illumination: f32,
    pub solar_occlusion: f32,
    pub lunar_shadow: f32,
    pub moon_radius_scale: f32,
    pub daylight: f32,
    pub light: f32,
    pub star_visibility: f32,
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
        let hub = HUB_AMPLITUDE * ((self.year_fraction() - SEASON_PHASE) * TAU).sin();
        let hour_angle = (self.day_fraction() - 0.5) * TAU;
        let (sin_h, cos_h) = hour_angle.sin_cos();
        let sun_position = [sin_h, hub + TRACK_FLATTEN * cos_h];
        let sun_altitude = sun_position[1].clamp(-1.0, 1.0);
        let shade_position = [
            -SHADE_TRACK * sin_h,
            hub - SHADE_TRACK * TRACK_FLATTEN * cos_h,
        ];

        let (sin_anomaly, cos_anomaly) = (self.anomalistic_fraction() * TAU).sin_cos();
        let moon_radius_scale = 1.0 + MOON_BREATH * cos_anomaly;
        let true_elongation = self.elongation() + MOON_BREATH * sin_anomaly;
        let rail = 1.0 + MOON_ECCENTRE * (self.eccentre_fraction() * TAU).cos();
        let (sin_hm, cos_hm) = (hour_angle - true_elongation).sin_cos();
        let moon_position = [rail * sin_hm, hub + rail * TRACK_FLATTEN * cos_hm];
        let moon_radius = MOON_DISC_RADIUS * moon_radius_scale;

        let to_sun_x = sun_position[0] - moon_position[0];
        let to_sun_y = sun_position[1] - moon_position[1];
        let to_sun_length = (to_sun_x * to_sun_x + to_sun_y * to_sun_y).sqrt();
        let solar_occlusion = covered_fraction(SUN_DISC_RADIUS, moon_radius, to_sun_length);
        let sun_direction = if to_sun_length > 1e-6 {
            [to_sun_x / to_sun_length, to_sun_y / to_sun_length]
        } else {
            [0.0, 0.0]
        };

        let to_shade_x = shade_position[0] - moon_position[0];
        let to_shade_y = shade_position[1] - moon_position[1];
        let shade_distance = (to_shade_x * to_shade_x + to_shade_y * to_shade_y).sqrt();
        let lunar_shadow = covered_fraction(moon_radius, SHADE_DISC_RADIUS, shade_distance);

        let illumination = (1.0 - true_elongation.cos()) / 2.0;
        let daylight = smoothstep(-0.12, 0.10, sun_position[1]);
        let sunlight = daylight * (1.0 - solar_occlusion);
        let moon_above_horizon = smoothstep(-0.10, 0.10, moon_position[1]);
        let moonlight = illumination
            * moon_above_horizon
            * (1.0 - lunar_shadow)
            * MOON_LIGHT_MAX
            * moon_radius_scale
            * moon_radius_scale;
        let light = sunlight.max(moonlight + SKYGLOW).clamp(0.0, 1.0);

        let star_visibility = ((1.0 - smoothstep(STARS_FULL, STARS_BEGIN, sun_position[1]))
            * (1.0 - MOON_STAR_WASH * moonlight))
            .max(1.0 - smoothstep(STARS_LIGHT_FULL, STARS_LIGHT_BEGIN, light));

        CelestialState {
            sun_position,
            moon_position,
            shade_position,
            sun_altitude,
            sun_direction,
            illumination,
            solar_occlusion,
            lunar_shadow,
            moon_radius_scale,
            daylight,
            light,
            star_visibility,
        }
    }
}
