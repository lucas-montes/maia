//! MET (metabolic equivalent of task) values for the exercise catalogue.
//!
//! A MET is energy cost relative to sitting at rest: kcal ≈ MET × body weight
//! (kg) × hours. Values follow the Compendium of Physical Activities (Ainsworth
//! et al.), using the closest listed activity. They are estimates, good to
//! maybe ±20%, which is the precision a calorie estimate can honestly claim.
//!
//! The 90 cardio entries are mapped one by one because their cost spans 3.5
//! (walking) to 12.3 (fast rope jumping). Everything else is resistance or
//! mobility work, where the cost depends on pace and rest far more than on
//! which lift it is, so it gets one value per kind of work.
//!
//! Compiled in rather than read from `data/`: the schema migration that fills
//! existing databases must not depend on the catalogue directory being
//! deployed next to the binary.

/// Resistance training, multiple exercises, 8–15 reps (Compendium 02054).
const RESISTANCE: f64 = 3.5;
/// Calisthenics, moderate effort: push-ups, sit-ups, lunges (02022).
const CALISTHENICS_MODERATE: f64 = 3.8;
/// Calisthenics, vigorous effort: jumping jacks, burpees, jump squats (02020).
const CALISTHENICS_VIGOROUS: f64 = 8.0;
/// Olympic and power lifting, vigorous effort (02050).
const WEIGHTLIFTING_VIGOROUS: f64 = 6.0;
/// Hatha yoga (02150).
const YOGA: f64 = 2.5;
/// Stretching, mild; also foam rolling and other self-massage (02101).
const STRETCHING: f64 = 2.3;
/// Fallback for a catalogue cardio entry missing from [`cardio_met`]:
/// moderate continuous effort, roughly jogging or general cycling.
const CARDIO_DEFAULT: f64 = 7.0;

/// MET for one catalogue exercise, keyed by its id (the parsed file stem).
///
/// Returns `None` for rows without a `body_part`. Those are user-created, and
/// their cost is unknown until the user sets it; a guessed value would be
/// indistinguishable from a curated one.
pub fn met_for(id: &str, body_part: Option<&str>, equipment: Option<&str>) -> Option<f64> {
    let body_part = body_part?.trim();
    if body_part.is_empty() || body_part.eq_ignore_ascii_case("null") {
        return None;
    }
    let body = body_part.to_ascii_lowercase();
    if body.contains("cardio") {
        return Some(cardio_met(id).unwrap_or(CARDIO_DEFAULT));
    }
    let equipment = equipment.unwrap_or("").to_ascii_lowercase();
    Some(if body.contains("plyometrics") {
        CALISTHENICS_VIGOROUS
    } else if body.contains("yoga") {
        YOGA
    } else if body.contains("stretching")
        || matches!(equipment.as_str(), "roll" | "rollball" | "stick" | "vibrate plate")
    {
        STRETCHING
    } else if body == "weightlifting" {
        WEIGHTLIFTING_VIGOROUS
    } else if equipment == "body weight" {
        CALISTHENICS_MODERATE
    } else {
        RESISTANCE
    })
}

/// Per-exercise values for the catalogue's cardio entries. The comment on each
/// group names the Compendium activity used.
fn cardio_met(id: &str) -> Option<f64> {
    Some(match id {
        // Walking, level, 2.8–3.2 mph, moderate pace (17190).
        "walk" | "walking" | "walking_on_treadmill" | "marching_on_spot" => 3.5,
        // Walking, 3.5 mph, brisk pace (17220).
        "briskly_walking" => 4.3,
        // Walking uphill, 1–5% grade, 2.9–3.5 mph (17210).
        "walking_on_incline_treadmill" => 5.3,

        // Running, 5 mph / 12 min per mile (12030).
        "run" | "run_on_treadmill" | "assault_run" => 8.3,
        // Running in place and short running drills (12170 area).
        "place_jog" | "quickly_trot_in_place" | "short_stride_run" | "quick_feet_run"
        | "wheel_run" | "backwards_run" | "band_assisted_sprinter_run" => 8.0,

        // Rope jumping, moderate (15552) and fast (15551).
        "jump_rope" | "high_jump_rope" | "high_knee_jump_rope" | "side-to-side_jump_rope"
        | "skip_jump_rope" => 11.8,
        "double_under_jump_rope" => 12.3,

        // Stationary cycling: very light (02011), vigorous (02014).
        "stationary_bike_walk" | "bicycle_recline_walk" => 3.5,
        "stationary_bike_run" | "assault_bike_run" => 8.8,
        // Arm cycling has no Compendium entry of its own; upper-body-only
        // ergometry runs well below leg cycling at the same effort.
        "hands_bike" => 4.0,

        // Elliptical trainer, moderate (02048).
        "elliptical_machine_walk" | "walking_on_elliptical_machine"
        | "walk_elliptical_cross_trainer" | "cycle_cross_trainer" => 5.0,
        // Ski machine, general (02060).
        "elliptical_machine_skiing" => 6.8,
        // Rowing, stationary ergometer, moderate (02071).
        "rowing" => 7.0,
        // Stair-treadmill ergometer, general (02065).
        "lever_stepper" => 9.0,

        // Battle ropes: whole-body vigorous intervals; seated and kneeling
        // variants drop the leg work.
        "battling_ropes" | "battling_ropes_alternating_waves_with_kneeling_get-up" => 8.0,
        "battling_ropes_seated" | "battling_ropes_half_kneeling" => 6.0,

        // Calisthenics, vigorous (02020): jumping, burpees, climbers, sprints
        // in place.
        "astride_jumps" | "cross_jacks" | "jack_jump" | "jumping_jack" | "low_jacks"
        | "plyo_jacks" | "rotational_jacks" | "split_jacks" | "squat_jacks" | "star_jump"
        | "scissor_jumps" | "jack_press" | "semi_squat_jump" | "frog_hops" | "skater"
        | "skater_hops" | "ski_step" | "jump_lunge_to_feet_jack" | "jump_step-up"
        | "lateral_twist_box_jump" | "burpee" | "burpee_with_push-up" | "dumbbell_burpee"
        | "jack_burpee" | "jack_burpee_with_push-up" | "mountain_climber"
        | "mountain_climber_lunge" | "vertical_mountain_climber" | "high_knee_run"
        | "high_knee_sprints" | "high_knees_butt_kicks" | "high_knee_skips"
        | "run_and_jack_jump" | "run_and_semi_squat_jump" | "run_and_half_knee_bend"
        | "run_and_hop" | "push_to_run" | "bear_crawl" | "duck_walk" | "swing_360" => {
            CALISTHENICS_VIGOROUS
        }

        // Calisthenics, moderate (02022): low-impact stepping, twisting and
        // knee drives.
        "back_and_forth_step" | "jack_step" | "step_up_crossover"
        | "step-up_opposite_elbow_to_knee_twist" | "elbow-to-knee_twists"
        | "elbow_to_knee_twists" | "high_knee_twist" | "high_knee_against_wall"
        | "half_knee_bends" | "high_knee_squat" | "high_knees_lunge"
        | "walking_high_knees_lunge" => CALISTHENICS_MODERATE,

        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_created_rows_stay_unknown() {
        assert_eq!(met_for("custom", None, None), None);
        assert_eq!(met_for("custom", Some(""), None), None);
        assert_eq!(met_for("custom", Some("null"), None), None);
    }

    #[test]
    fn cardio_uses_the_per_exercise_table() {
        assert_eq!(met_for("walking", Some("Cardio"), Some("Body weight")), Some(3.5));
        assert_eq!(met_for("jump_rope", Some("Cardio"), Some("Rope")), Some(11.8));
        assert_eq!(met_for("rowing", Some("Cardio"), Some("Leverage machine")), Some(7.0));
        // An unmapped cardio entry falls back rather than being left unknown.
        assert_eq!(met_for("new_cardio", Some("Cardio"), None), Some(CARDIO_DEFAULT));
    }

    #[test]
    fn non_cardio_maps_by_kind_of_work() {
        assert_eq!(met_for("x", Some("Chest"), Some("Barbell")), Some(RESISTANCE));
        assert_eq!(met_for("x", Some("Chest"), Some("Body weight")), Some(CALISTHENICS_MODERATE));
        assert_eq!(met_for("x", Some("Plyometrics"), Some("Body weight")), Some(CALISTHENICS_VIGOROUS));
        assert_eq!(met_for("x", Some("Weightlifting"), Some("Barbell")), Some(WEIGHTLIFTING_VIGOROUS));
        assert_eq!(met_for("x", Some("Yoga"), Some("Body weight")), Some(YOGA));
        assert_eq!(met_for("x", Some("Back"), Some("Rollball")), Some(STRETCHING));
    }

    /// Every cardio entry in the shipped catalogue has a curated value, so the
    /// fallback only ever applies to entries added later.
    #[test]
    fn every_catalogue_cardio_entry_is_mapped() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data/parsed");
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return; // Catalogue not checked out; nothing to cross-check.
        };
        let mut missing = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else { continue };
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
            let body = v.get("body_part").and_then(|b| b.as_str()).unwrap_or("");
            if body.to_ascii_lowercase().contains("cardio") && cardio_met(stem).is_none() {
                missing.push(stem.to_string());
            }
        }
        missing.sort();
        assert!(missing.is_empty(), "cardio entries without a MET: {missing:?}");
    }
}
