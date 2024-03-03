//! In lieu of his warlock punch in the air, Ganondorf has been given the ability to
//! float. In his float state, Ganondorf can move freely in the air, and perform all
//! of his aerials. His specials will remove his float status; however, given the right
//! control inputs, his side-special can get some serious distance.
use super::utils::*;
use skyline_smash::app::BattleObjectModuleAccessor;
use smash::app::lua_bind::*;
use smash::lib::lua_const::*;
use smash_script::macros;
use std::f32::consts::PI;
use {
    smash::{hash40, lua2cpp::*},
    smashline::*,
};

const MAX_FLOAT_FRAMES: i16 = 91; // Float by this amount
const TELEPORT_TO_FLOAT_FRAMES: i16 = 40; // Teleport into float frames.
const STARTING_FLOAT_FRAME: f32 = 2.0; // When the float frame will start.
const X_MAX: f32 = 1.155; // Maximum velocity that can be achieved for X movements.
const X_ACCEL_MULT: f32 = 0.12; // Multiplier for internal calculations
const Y_MAX: f32 = X_MAX; // Same as `X_MAX`, but for Y movement
const Y_ACCEL_MULT: f32 = X_ACCEL_MULT; // Ditto

/// Adjust speed to Ganondorf's float depending on the current control stick positions.
unsafe extern "C" fn adjust_float_velocity(boma: *mut BattleObjectModuleAccessor, iv: &InitValues) {
    let new_speed = Position2D::calculate_new_speed(
        ControlModule::get_stick_x(boma) * PostureModule::lr(boma),
        ControlModule::get_stick_y(boma),
        KineticModule::get_sum_speed_x(boma, *KINETIC_ENERGY_RESERVE_ATTRIBUTE_MAIN),
        KineticModule::get_sum_speed_y(boma, *KINETIC_ENERGY_RESERVE_ATTRIBUTE_MAIN),
    );
    println!("Calculated speed additions: {:#?}", new_speed);
    KineticModule::add_speed(
        boma,
        &smash::phx::Vector3f {
            x: new_speed.x,
            y: new_speed.y,
            z: 0.0,
        },
    );
    GS[iv.entry_id].speed = Position2D {
        x: GS[iv.entry_id].speed.x + new_speed.x,
        y: GS[iv.entry_id].speed.y + new_speed.y,
    };
    println!("New speed: {:#?}", GS[iv.entry_id].speed);
}

/// This function will keep Ganondorf's float consistent. He will remain in the air
/// by constantly changing his `KineticType`, and will ensure that his speed remains
/// consistent after he performs an attack.
///
/// - [x] Address bug where after Ganondorf performs an attack, his float velocity stops.
unsafe extern "C" fn check_float_velocity(boma: *mut BattleObjectModuleAccessor, iv: &InitValues) {
    if KineticModule::get_kinetic_type(boma) != *FIGHTER_KINETIC_TYPE_MOTION_AIR {
        KineticModule::change_kinetic(boma, *FIGHTER_KINETIC_TYPE_MOTION_AIR);
        if iv.prev_status_kind == FIGHTER_STATUS_KIND_ATTACK_AIR {
            KineticModule::add_speed(
                boma,
                &KineticModule::get_sum_speed3f(boma, *KINETIC_ENERGY_RESERVE_ATTRIBUTE_MAIN),
            );
        }
    }
}

/// Cosmetic effect that will further show Ganondorf's float status.
unsafe extern "C" fn float_effect(fighter: &mut L2CFighterCommon) {
    macros::EFFECT_FOLLOW(
        fighter,
        Hash40::new("ganon_rekkikyaku"),
        Hash40::new("kneer"),
        12,
        -1.5,
        0,
        0,
        0,
        0,
        0.5,
        true,
    );
    macros::EFFECT_FOLLOW(
        fighter,
        Hash40::new("ganon_rekkikyaku"),
        Hash40::new("kneel"),
        12,
        -1.5,
        0,
        0,
        0,
        0,
        0.5,
        true,
    );
}

/// Implement how Ganondorf's current float status is handled.
impl FloatStatus {
    /// Ganondorf can regain his ability to float when...
    /// - he is in a neutral state, (i.e. is on the ground, started a new match)
    /// - he catches an oppoent with side-special or up-special
    fn transition_to_can_float_if_able(self: Self, init_values: &InitValues) -> FloatStatus {
        if [
            *FIGHTER_STATUS_KIND_SPECIAL_LW,
            *FIGHTER_STATUS_KIND_SPECIAL_HI,
            *FIGHTER_GANON_STATUS_KIND_SPECIAL_AIR_S_CATCH,
            *FIGHTER_GANON_STATUS_KIND_SPECIAL_AIR_S_END,
            *FIGHTER_STATUS_KIND_WIN,
            *FIGHTER_STATUS_KIND_LOSE,
            *FIGHTER_STATUS_KIND_DEAD,
        ]
        .contains(&init_values.status_kind)
            || init_values.situation_kind != SITUATION_KIND_AIR
        {
            return FloatStatus::CanFloat;
        }
        return self;
    }

    /// Ganondorf will lose his float after he...
    /// - Performs any special move
    /// - Gets hit (and launched) while floating
    /// - His float timer naturall expires
    /// - Performs an air dodge.
    fn transition_to_cannot_float_if_able(self: Self, init_values: &InitValues) -> FloatStatus {
        if let FloatStatus::Floating(i) = self {
            if i == 0
                || init_values.situation_kind != SITUATION_KIND_AIR
                || [
                    *FIGHTER_STATUS_KIND_JUMP_AERIAL,
                    *FIGHTER_STATUS_KIND_DAMAGE_FLY,
                    *FIGHTER_STATUS_KIND_DAMAGE_FLY_METEOR,
                    *FIGHTER_STATUS_KIND_DAMAGE_FLY_ROLL,
                    *FIGHTER_STATUS_KIND_ESCAPE_AIR,
                    *FIGHTER_STATUS_KIND_ESCAPE_AIR_SLIDE,
                    *FIGHTER_GANON_STATUS_KIND_SPECIAL_N_TURN,
                ]
                .contains(&init_values.status_kind)
                || (init_values.status_kind == *FIGHTER_STATUS_KIND_DAMAGE_AIR
                    && init_values.teleport_into_float)
            {
                return FloatStatus::CannotFloat;
            }
        }
        return self;
    }

    /// Switch to a float status if the special button is pressed and in the air.
    fn transition_to_floating_if_able(self: Self, init_values: &InitValues) -> FloatStatus {
        if init_values.is_start_of_float() {
            return FloatStatus::Floating(MAX_FLOAT_FRAMES);
        }
        if init_values.teleport_into_float {
            return FloatStatus::Floating(TELEPORT_TO_FLOAT_FRAMES);
        }
        return self;
    }
}

/// Controls air velocity when floating.
impl Position2D {
    fn calculate_new_speed(stick_x: f32, stick_y: f32, speed_x: f32, speed_y: f32) -> Position2D {
        Position2D {
            x: Position2D::calculate_new_speed_helper(stick_x, speed_x),
            y: Position2D::calculate_new_speed_helper(stick_y, speed_y),
        }
    }

    /// Top speed in either direction: 1.26
    /// Formula: f(x) = 1.26 * sin^2 * (πx / 2)
    /// Where -1 <= x <= 1
    fn calculate_new_speed_helper(stick: f32, curr_speed: f32) -> f32 {
        let max_speed: f32 = 1.26; // Max speed of the float.
        let max_additional_speed: f32 = max_speed / 4.0;
        let speed_loss: f32 = 25.0; // How many frames of idle stick to reach 0 velocity.
        let mut new_speed = 0.0;
        println!("Current speed: {}", curr_speed);
        println!("Stick: {}", stick);
        if stick < 0.1 && stick > -0.1 {
            if curr_speed >= 0.08 || curr_speed <= -0.08 {
                if stick.is_sign_negative() {
                    new_speed = curr_speed / speed_loss;
                } else {
                    new_speed = -curr_speed / speed_loss;
                }
                println!("Slowing to neutral.");
            } else {
                println!("No change needed!");
            }
        } else {
            let abs_curr_speed = curr_speed.abs();
            println!("Absolute curr speed {}", abs_curr_speed);
            if abs_curr_speed != max_speed {
                new_speed = max_additional_speed * (PI * stick / 2.0).sin().powi(2);
                if abs_curr_speed + new_speed >= max_speed {
                    println!("Overflow, correcting.");
                    new_speed = max_speed - abs_curr_speed;
                }
            }
        }
        println!("Raw new speed: {}", new_speed);
        if stick < 0.0 {
            return -new_speed;
        }
        return new_speed;
    }
}

impl InitValues {
    fn is_special_air_n(self: &Self) -> bool {
        self.motion_kind == hash40("special_air_n")
    }

    fn is_start_of_float(self: &Self) -> bool {
        self.motion_module_frame == STARTING_FLOAT_FRAME && self.is_special_air_n()
    }
}

/// The main driver logic for floating, given the current frame, this _main_ block will
/// determine the current float status and handle each case.
pub unsafe extern "C" fn ganon_float(fighter: &mut L2CFighterCommon, iv: &InitValues) {
    let boma = fighter.module_accessor;
    if WorkModule::is_flag(boma, GANON_TELEPORT_INTO_FLOAT_INIT_FLAG) {
        println!("Teleport into float!");
        WorkModule::set_flag(boma, false, GANON_TELEPORT_INTO_FLOAT_INIT_FLAG);
        WorkModule::set_flag(boma, true, GANON_TELEPORT_INTO_FLOAT_HANDLE_FLAG);
        MotionModule::change_motion_kind(boma, Hash40::new("special_air_n"));
    }
    println!("Original float state: {}", GS[iv.entry_id].fs);
    GS[iv.entry_id].fs = match GS[iv.entry_id].fs {
        FloatStatus::CanFloat => GS[iv.entry_id].fs.transition_to_floating_if_able(&iv),
        FloatStatus::CannotFloat => GS[iv.entry_id].fs.transition_to_can_float_if_able(&iv),
        FloatStatus::Floating(_) => {
            let fs = GS[iv.entry_id].fs.transition_to_cannot_float_if_able(&iv);
            if matches!(fs, FloatStatus::Floating(_)) {
                GS[iv.entry_id].fs.transition_to_can_float_if_able(&iv)
            } else {
                fs
            }
        }
    };
    println!("New float state: {}", GS[iv.entry_id].fs);
    match GS[iv.entry_id].fs {
        FloatStatus::CannotFloat => {
            if iv.is_start_of_float() {
                StatusModule::change_status_request_from_script(
                    boma,
                    *FIGHTER_STATUS_KIND_FALL_AERIAL,
                    true,
                );
            } else if iv.status_kind == FIGHTER_STATUS_KIND_ATTACK_AIR {
                KineticModule::change_kinetic(boma, *FIGHTER_KINETIC_TYPE_MOTION_FALL);
            }
            if iv.teleport_into_float {
                WorkModule::turn_off_flag(boma, GANON_TELEPORT_INTO_FLOAT_HANDLE_FLAG);
                macros::WHOLE_HIT(fighter, *HIT_STATUS_NORMAL);
                VisibilityModule::set_whole(boma, true);
                GS[iv.entry_id].fs = FloatStatus::CanFloat;
            }
        }
        FloatStatus::Floating(i) => {
            if i == TELEPORT_TO_FLOAT_FRAMES && iv.teleport_into_float {
                StatusModule::change_status_request_from_script(
                    boma,
                    FIGHTER_STATUS_KIND_ATTACK_AIR.into(),
                    false.into(),
                );
            }
            println!("Current speed: {:#?}", GS[iv.entry_id].speed);
            if iv.is_start_of_float() {
                macros::PLAY_SE(fighter, Hash40::new("se_ganon_special_l01"));
                CancelModule::enable_cancel(boma);
            }
            if i % 30 == 0 {
                float_effect(fighter);
            }
            GS[iv.entry_id].fs = FloatStatus::Floating(i - 1);
            check_float_velocity(boma, &iv);
            if i - 1 == 0 {
                KineticModule::change_kinetic(boma, *FIGHTER_KINETIC_TYPE_MOTION_FALL);
            } else if !iv.teleport_into_float {
                adjust_float_velocity(boma, &iv);
            }
        }
        _ => GS[iv.entry_id].speed = Position2D::reset(),
    }
}
