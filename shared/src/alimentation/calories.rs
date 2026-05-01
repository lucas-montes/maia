use core::{marker::PhantomData, unreachable};


struct Bmr {
    age: u32,
    weight: f32,
    height: f32,
}

impl Bmr {
    pub fn new(age: u32, weight: f32, height: f32) -> Self {
        Self { age, weight, height }
    }

    pub fn value(&self) -> f32 {
        // Using the Mifflin-St Jeor Equation for BMR calculation
        (10.0 * self.weight) + (6.25 * self.height) - (5.0 * self.age as f32) + 5.0
    }
}

struct Tdee {
    bmr: Bmr,
    activity_factor: f32,
}

impl Tdee {
    pub fn new(bmr: Bmr, activity_factor: f32) -> Self {
        Self { bmr, activity_factor }
    }

    pub fn value(&self) -> f32 {
        self.bmr.value() * self.activity_factor
    }
}

struct HypoteticalTdee{
    bmr: Bmr,
    average_calories_burned_per_day: f32,
}

impl HypoteticalTdee {
    pub fn new(bmr: Bmr, average_calories_burned_per_day: f32) -> Self {
        Self { bmr, average_calories_burned_per_day }
    }

    pub fn value(&self) -> f32 {
        self.bmr.value() + self.average_calories_burned_per_day
    }
}


enum Fats {
    GrPerKg(f32),
    PercentageOfCalories(f32),
}

impl Fats {
    pub fn gr_from_percentage(&self, calories: f32) -> f32 {
        match self {
            Fats::GrPerKg(_) => unreachable!("Cannot calculate fats in grams based on grams per kg without knowing weight"),
            Fats::PercentageOfCalories(percentage) => (percentage / 100.0) * calories / 9.0, // 1 gram of fat has 9 calories
        }
    }

    pub fn gr_per_gk(&self, weight: f32) -> f32 {
        match self {
            Fats::GrPerKg(grams_per_kg) => grams_per_kg *weight,
            Fats::PercentageOfCalories(_) => unreachable!("Cannot calculate fats based on percentage of calories without knowing total calories"),
        }
    }
}

struct Metadata {
    age: u32,
    weight: f32,
    height: f32,
}

impl Metadata {
    pub fn new(age: u32, weight: f32, height: f32) -> Self {
        Self { age, weight, height }
    }
}


struct MacrosCalculator {
    metadata: Metadata,
    tdee: Tdee,
    bmr: Bmr,
    hypotetical_tdee: HypoteticalTdee,
}


struct Macros {
    calories: f32,
    proteins: f32,
    carbs: f32,
    fats: f32,
    fiber: f32,
}
